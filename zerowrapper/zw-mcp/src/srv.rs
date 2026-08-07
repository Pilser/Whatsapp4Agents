use std::path::Path;
use serde_json::{json, Value};
use zw_db::serverless::{Agg, Op, SrvFilter, parse_filter};
use zw_db::serverless::SrvApp;
use zw_db::DbPool;

const USAGE: &str =
    "usage: srv <group> <verb> [args]\n\
     groups:\n\
       apps\n\
       records\n\
files\n\
      apps:\n\
        srv apps create --title \"<title>\" [--json <schema>] [--public-reads on|off] [--unique '<$.path>']\n\
        srv apps list\n\
        srv apps show <board_id>\n\
        srv apps schema <board_id>\n\
        srv apps update <board_id> [--title \"<title>\"] [--json <schema> | --clear-schema] [--public-reads on|off] [--unique '<$.path>' | --clear-unique]\n\
        srv apps delete <board_id>\n\
      records:\n\
        srv records submit <board_id> <json> [--writer <id>] [--upsert]\n\
        srv records list <board_id> [--limit N] [--offset N]\n\
        srv records get <board_id> --seq N\n\
        srv records update <board_id> <seq> '<json>' [--writer <id>]\n\
        srv records patch <board_id> <seq> '<ops>'\n\
        srv records query <board_id> '<$.path>' [--eq value]\n\
        srv records query <board_id> --filter '<json>' [--order '<$.path>' asc|desc] [--limit N] [--offset N]\n\
        srv records search <board_id> '<query>' [--limit N] [--offset N] [--hl]\n\
        srv records count <board_id>\n\
        srv records aggregate <board_id> --op sum|avg|min|max|count --field '<$.path>' [--group_by '<$.path>'] [--filter '<json>']\n\
        srv records delete <board_id> [--seq N | --all]\n\
     filter ops: eq neq gt gte lt lte contains not_contains in search\n\
     filter json: {\"field\":\"$.price\",\"op\":\"gte\",\"value\":500} | {\"$.price\":{\"gte\":500}} | {\"search\":\"q\",\"price\":{\"gte\":100}}\n\
     search = ranked full-text (BM25) across the whole record payload, combinable with other ops.\n\
     files:\n\
       srv files list <board_id>\n\
       srv files get <board_id> <file_id> --out <path>";

type CmdResult = Result<Value, Value>;

const APPS_USAGE: &str =
    "srv apps <verb> [args]\n\
     create --title \"<title>\" [--json <schema>] [--public-reads on|off] [--unique '<$.path>']\n\
     list\n\
     show <board_id>\n\
     schema <board_id>\n\
     update <board_id> [--title \"t\"] [--json <schema> | --clear-schema] [--public-reads on|off] [--unique '<$.path>' | --clear-unique]\n\
     delete <board_id>\n";

const RECORDS_USAGE: &str =
    "srv records <verb> [args]\n\
     submit <board_id> <json> [--writer <id>] [--upsert]\n\
     list <board_id> [--limit N] [--offset N]\n\
     get <board_id> --seq N\n\
     update <board_id> <seq> '<json>' [--writer <id>]\n\
     patch <board_id> <seq> '<ops>'\n\
     query <board_id> '<$.path>' [--eq value]\n\
     query <board_id> --filter '<json>' [--order '<$.path>' asc|desc] [--limit N] [--offset N]\n\
     search <board_id> '<query>' [--limit N] [--offset N] [--hl]\n\
     count <board_id>\n\
     aggregate <board_id> --op sum|avg|min|max|count --field '<$.path>' [--group_by '<$.path>'] [--filter '<json>']\n\
     delete <board_id> [--seq N | --all]\n\
     query <path> uses JSONPath: '<$.name>', '<$.a.b>', '<$.arr[0]>' (leading $ required).\n\
     patch json: {\"$set\":{\"color\":\"black\"},\"$inc\":{\"votes\":1},\"$dec\":{...},\"$mul\":{...},\"$unset\":[\"field\",\"a.b[0]\"]}\n\
     writer filter form: {\"writer\":{\"eq\":\"alice\"}}\n\
     --unique: extra submit of the same unique value is rejected unless --upsert; upsert replaces the existing record keeping its seq.\n\
     filter ops: eq neq gt gte lt lte contains not_contains in search\n\
     filter json: {\"field\":\"$.price\",\"op\":\"gte\",\"value\":500} | {\"$.price\":{\"gte\":500}} | {\"search\":\"q\",\"price\":{\"gte\":5}}\n\
     search does ranked full-text (BM25) across the whole record payload; combines with other filter ops.\n";

const FILES_USAGE: &str =
    "srv files <verb> [args]\n\
     list <board_id>\n\
     get <board_id> <file_id> --out <path>\n";

fn err_json(error: &str, command: &str, usage: &str, hint: &str) -> Value {
    json!({
        "status": "error",
        "error": error,
        "command": command,
        "hint": hint,
        "usage": usage,
    })
}

fn val_after<'a>(args: &'a [&String], flag: &str) -> Option<String> {
    for (i, tok) in args.iter().enumerate() {
        if tok.as_str() == flag {
            return args.get(i + 1).map(|v| v.as_str().trim_matches('"').to_string());
        }
    }
    None
}

fn has_flag(args: &[&String], flag: &str) -> bool {
    args.iter().any(|t| t.as_str() == flag)
}

fn order_from(args: &[&String]) -> Option<(String, bool)> {
    let idx = args.iter().position(|t| t.as_str() == "--order")?;
    let path = args.get(idx + 1).map(|p| unquote(p)).filter(|p| !p.is_empty())?;
    let desc = match args.get(idx + 2).map(|d| d.as_str()) {
        Some("asc") => false,
        Some("desc") => true,
        _ => false,
    };
    Some((path, desc))
}

fn split_search(conds: &[zw_db::filters::FilterCond]) -> Option<(String, Vec<zw_db::filters::FilterCond>)> {
    let mut query = None;
    let mut others = Vec::new();
    for c in conds {
        if c.op == Op::Search {
            query = c.value.as_str().map(|s| s.to_string());
        } else {
            others.push(c.clone());
        }
    }
    query.map(|q| (q, others))
}

fn unquote(s: &str) -> String {
    let s = s.trim_start_matches('\"').trim_start_matches('\'');
    let s = s.trim_end_matches('\"').trim_end_matches('\'');
    s.to_string()
}

/// Rebuild a flag's value from the possibly-many whitespace-split tokens that follow
/// it (JSON schemas and titles contain spaces), stopping at the next `--` flag.
fn joined_flag(args: &[&String], flag: &str) -> Option<String> {
    for (i, tok) in args.iter().enumerate() {
        if tok.as_str() == flag {
            let parts: Vec<&str> = args[i + 1..].iter().map(|v| v.as_str()).collect();
            let parts: Vec<&str> = parts.iter().take_while(|p| !p.starts_with("--")).map(|p| *p).collect();
            if parts.is_empty() {
                return None;
            }
            return Some(unquote(&parts.join(" ")));
        }
    }
    None
}

fn public_mode(args: &[&String], raw: &str, verb: &str) -> Result<bool, Value> {
    match val_after(args, "--public-reads") {
        Some(v) if v == "on" => Ok(true),
        Some(v) if v == "off" => Ok(false),
        Some(other) => Err(err_json(
            &format!("expected on or off for --public-reads, got '{other}'"),
            raw,
            verb,
            verb,
        )),
        None => Ok(false),
    }
}

async fn require_board(db: &DbPool, board_id: &str, sid: &str, raw: &str) -> Result<SrvApp, Value> {
    db.srv_app_owned(board_id, sid)
        .await
        .map_err(|e| err_json(&e.to_string(), raw, "srv <group> <verb>", "srv apps list"))?
        .ok_or_else(|| {
            err_json(
                &format!("board {board_id} not found or not owned"),
                raw,
                "srv <group> <verb>",
                "srv apps list",
            )
        })
}

async fn cmd_apps(db: &DbPool, sid: &str, verb: &str, args: &[&String], raw: &str) -> CmdResult {
    if verb == "--help" {
        return Ok(json!({ "status": "ok", "help": APPS_USAGE }));
    }
    match verb {
        "create" => {
            let title = joined_flag(args, "--title").unwrap_or_else(|| "Untitled".to_string());
            let schema = joined_flag(args, "--json");
            let public_reads = public_mode(args, raw, "srv apps create")?;
            let unique_key = joined_flag(args, "--unique");
            let board_id = db
                .srv_app_create(sid, &title, schema.as_deref(), public_reads, unique_key.as_deref())
                .await
                .map_err(|e| err_json(&e.to_string(), raw, "create", "create a new app"))?;
            Ok(json!({ "status": "ok", "board_id": board_id }))
        }
        "list" => {
            let apps = db
                .srv_app_list(sid)
                .await
                .map_err(|e| err_json(&e.to_string(), raw, "list", "list apps"))?;
            Ok(json!({ "status": "ok", "apps": apps }))
        }
        "show" => {
            let board_id = args.first().ok_or_else(|| {
                err_json("missing <board_id>", raw, "srv apps show <board_id>", "srv apps show <board_id>")
            })?;
            let app = require_board(db, board_id, sid, raw).await?;
            Ok(json!({ "status": "ok", "app": app }))
        }
        "schema" => {
            let board_id = args.first().ok_or_else(|| {
                err_json("missing <board_id>", raw, "srv apps schema <board_id>", "srv apps schema <board_id>")
            })?;
            let app = require_board(db, board_id, sid, raw).await?;
            Ok(json!({
                "status": "ok",
                "board_id": app.board_id,
                "schema": app.schema_json,
            }))
        }
        "update" => {
            let board_id = args.first().ok_or_else(|| {
                err_json("missing <board_id>", raw, "srv apps update <board_id>", "srv apps update <board_id>")
            })?;
            require_board(db, board_id, sid, raw).await?;
            let title = joined_flag(args, "--title");
            let schema: Option<Option<String>> = if has_flag(args, "--clear-schema") {
                Some(None)
            } else if let Some(s) = joined_flag(args, "--json") {
                Some(Some(s))
            } else {
                None
            };
            let unique_key: Option<Option<String>> = if has_flag(args, "--clear-unique") {
                Some(None)
            } else if let Some(u) = joined_flag(args, "--unique") {
                Some(Some(u))
            } else {
                None
            };
            let public_reads: Option<bool> = match val_after(args, "--public-reads") {
                Some(v) if v == "on" => Some(true),
                Some(v) if v == "off" => Some(false),
                Some(other) => {
                    return Err(err_json(
                        &format!("expected on or off for --public-reads, got '{other}'"),
                        raw,
                        "srv apps update <board_id>",
                        "srv apps update <board_id>",
                    ))
                }
                None => None,
            };
            db.srv_app_update(
                board_id,
                sid,
                title.as_deref(),
                schema.as_ref().map(|s| s.as_deref()),
                public_reads,
                unique_key.as_ref().map(|u| u.as_deref()),
            )
            .await
            .map_err(|e| err_json(&e.to_string(), raw, "srv apps update <board_id>", "srv apps update <board_id>"))?;
            Ok(json!({ "status": "ok", "updated": true }))
        }
        "delete" => {
            let board_id = args.first().ok_or_else(|| {
                err_json("missing <board_id>", raw, "srv apps delete <board_id>", "srv apps delete <board_id>")
            })?;
            require_board(db, board_id, sid, raw).await?;
            db.srv_app_delete(board_id, sid)
                .await
                .map_err(|e| err_json(&e.to_string(), raw, "srv apps delete", "srv apps delete <board_id>"))?;
            Ok(json!({ "status": "ok", "deleted": true }))
        }
        other => Err(err_json(
            &format!("unknown apps verb '{other}'"),
            raw,
            "srv apps <verb>",
            USAGE,
        )),
    }
}

async fn cmd_records(db: &DbPool, sid: &str, verb: &str, args: &[&String], raw: &str) -> CmdResult {
    if verb == "--help" {
        return Ok(json!({ "status": "ok", "help": RECORDS_USAGE }));
    }
    let board_id = args
        .first()
        .ok_or_else(|| err_json("missing <board_id>", raw, "srv records <verb>", "srv records list"))?;
    require_board(db, board_id, sid, raw).await?;
    let rest = &args[1..];
    match verb {
        "submit" => {
            if rest.is_empty() {
                return Err(err_json(
                    "missing <json> payload",
                    raw,
                    "srv records submit <board_id> <json>",
                    "srv records submit <board_id> '{\"k\":1}'",
                ));
            }
            let json_tokens: Vec<_> = rest
                .iter()
                .take_while(|t| !t.starts_with("--"))
                .map(|v| v.as_str())
                .collect();
            let json_str = unquote(&json_tokens.join(" "));
            let payload: Value = serde_json::from_str(&json_str).map_err(|_| {
                err_json(
                    "invalid JSON payload",
                    raw,
                    "srv records submit <board_id> <json>",
                    "srv records submit <board_id> '{\"k\":1}'",
                )
            })?;
            let writer = joined_flag(rest, "--writer");
            db.srv_validate(board_id, &payload)
                .await
                .map_err(|e| err_json(&e.to_string(), raw, "srv records submit", "srv records submit <board_id> <json>"))?;
            let seq = db
                .srv_record_insert(board_id, &payload, writer.as_deref(), has_flag(rest, "--upsert"))
                .await
                .map_err(|e| err_json(&e.to_string(), raw, "srv records submit", "srv records submit <board_id> <json>"))?;
            Ok(json!({ "status": "ok", "seq": seq }))
        }
        "list" => {
            let limit: i64 = val_after(rest, "--limit")
                .and_then(|v| v.parse().ok())
                .filter(|l| *l > 0)
                .unwrap_or(50);
            let offset: i64 = val_after(rest, "--offset")
                .and_then(|v| v.parse().ok())
                .filter(|o| *o >= 0)
                .unwrap_or(0);
            let records = db
                .srv_record_list(board_id, limit, None, offset)
                .await
                .map_err(|e| err_json(&e.to_string(), raw, "srv records list", "srv records list <board_id>"))?;
            Ok(json!({ "status": "ok", "records": records }))
        }
        "get" => {
            let seq: i64 = rest
                .iter()
                .position(|t| t.as_str() == "--seq")
                .and_then(|i| rest.get(i + 1))
                .and_then(|v| v.parse().ok())
                .ok_or_else(|| err_json("missing --seq N", raw, "srv records get <board_id> --seq N", "srv records get <board_id> --seq 1"))?;
            let record = db
                .srv_record_get(board_id, seq)
                .await
                .map_err(|e| err_json(&e.to_string(), raw, "srv records get", "srv records get <board_id> --seq N"))?;
            Ok(json!({ "status": "ok", "record": record }))
        }
        "update" => {
            let seq: i64 = rest
                .first()
                .and_then(|v| v.parse().ok())
                .ok_or_else(|| err_json("missing <seq>", raw, "srv records update <board_id> <seq> <json>", "srv records update <board_id> 1 '{\"k\":1}'"))?;
            let json_tokens: Vec<_> = rest[1..]
                .iter()
                .take_while(|t| !t.starts_with("--"))
                .map(|v| v.as_str())
                .collect();
            let json_str = unquote(&json_tokens.join(" "));
            let payload: Value = serde_json::from_str(&json_str).map_err(|_| {
                err_json(
                    "invalid JSON payload",
                    raw,
                    "srv records update <board_id> <seq> <json>",
                    "srv records update <board_id> 1 '{\"k\":1}'",
                )
            })?;
            let writer = joined_flag(rest, "--writer");
            db.srv_record_set(board_id, seq, &payload, writer.as_deref())
                .await
                .map_err(|e| err_json(&e.to_string(), raw, "srv records update", "srv records update <board_id> <seq> <json>"))?;
            Ok(json!({ "status": "ok", "seq": seq }))
        }
        "patch" => {
            let seq: i64 = rest
                .first()
                .and_then(|v| v.parse().ok())
                .ok_or_else(|| err_json("missing <seq>", raw, "srv records patch <board_id> <seq> <ops>", "srv records patch <board_id> 1 '{\"$inc\":{\"votes\":1}}'"))?;
            let ops_str = unquote(&rest[1..].iter().map(|v| v.as_str()).collect::<Vec<_>>().join(" "));
            let ops: Value = serde_json::from_str(&ops_str).map_err(|_| {
                err_json(
                    "invalid JSON ops",
                    raw,
                    "srv records patch <board_id> <seq> <ops>",
                    "srv records patch <board_id> 1 '{\"$inc\":{\"votes\":1}}'",
                )
            })?;
            let merged = db
                .srv_record_patch(board_id, seq, &ops)
                .await
                .map_err(|e| err_json(&e.to_string(), raw, "srv records patch", "srv records patch <board_id> <seq> <ops>"))?;
            Ok(json!({ "status": "ok", "seq": seq, "payload": merged }))
        }
        "query" => {
            if let Some(f) = joined_flag(rest, "--filter") {
                let parsed: Value = serde_json::from_str(&f).map_err(|_| {
                    err_json(
                        "invalid --filter json",
                        raw,
                        "srv records query <board_id> --filter <json>",
                        "srv records query <board_id> --filter '{\"$.price\":{\"gte\":500}}'",
                    )
                })?;
                let filter = parse_filter(&parsed)
                    .map_err(|e| err_json(&e.to_string(), raw, "srv records query", "srv records query <board_id> --filter <json>"))?;
                let limit: i64 = val_after(rest, "--limit")
                    .and_then(|v| v.parse().ok())
                    .filter(|l| *l > 0)
                    .unwrap_or(50);
                let offset: i64 = val_after(rest, "--offset")
                    .and_then(|v| v.parse().ok())
                    .filter(|o| *o >= 0)
                    .unwrap_or(0);
                if let Some((q, others)) = split_search(&filter.conds) {
                    let records = db
                        .srv_record_search(&board_id, &q, &others, limit, offset, false)
                        .await
                        .map_err(|e| err_json(&e.to_string(), raw, "srv records query", "srv records query <board_id> --filter <json>"))?;
                    return Ok(json!({ "status": "ok", "records": records }));
                }
                let order = order_from(rest);
                let records = db
                    .srv_record_filter(&board_id, &filter.conds, order.as_ref().map(|(p, d)| (p.as_str(), *d)), limit, offset)
                    .await
                    .map_err(|e| err_json(&e.to_string(), raw, "srv records query", "srv records query <board_id> --filter <json>"))?;
                return Ok(json!({ "status": "ok", "records": records }));
            }
            let path = rest
                .first()
                .map(|p| unquote(p))
                .filter(|p| !p.is_empty())
                .ok_or_else(|| {
                    err_json(
                        "missing json path",
                        raw,
                        "srv records query <board_id> <path>",
                        "srv records query <board_id> '$.name'",
                    )
                })?;
            let eq = joined_flag(rest, "--eq");
            let records = db
                .srv_record_query(&board_id, &path, eq.as_deref())
                .await
                .map_err(|e| err_json(&e.to_string(), raw, "srv records query", "srv records query <board_id> <path>"))?;
            Ok(json!({ "status": "ok", "records": records }))
        }
        "search" => {
            if rest.is_empty() {
                return Err(err_json(
                    "missing <query>",
                    raw,
                    "srv records search <board_id> '<query>'",
                    "srv records search <board_id> 'laptop'",
                ));
            }
let limit: i64 = val_after(rest, "--limit")
                .and_then(|v| v.parse().ok())
                .filter(|l| *l > 0)
                .unwrap_or(50);
            let offset: i64 = val_after(rest, "--offset")
                .and_then(|v| v.parse().ok())
                .filter(|o| *o >= 0)
                .unwrap_or(0);
            let query = rest
                .iter()
                .take_while(|t| !t.starts_with("--"))
                .map(|t| unquote(t))
                .collect::<Vec<_>>()
                .join(" ")
                .trim()
                .to_string();
            if query.is_empty() {
                return Err(err_json("empty <query>", raw, "srv records search <board_id> '<query>'", "srv records search <board_id> 'laptop'"));
            }
            let records = db
                .srv_record_search(&board_id, &query, &[], limit, offset, has_flag(rest, "--hl"))
                .await
                .map_err(|e| err_json(&e.to_string(), raw, "srv records search", "srv records search <board_id> '<query>'"))?;
            Ok(json!({ "status": "ok", "records": records, "searched": query }))
        }
        "aggregate" => {
            let agg = val_after(rest, "--op")
                .and_then(|o| Agg::parse(&o).ok())
                .ok_or_else(|| err_json("missing --op sum|avg|min|max|count", raw, "srv records aggregate", "srv records aggregate <board_id> --op sum --field '$.price'"))?;
            let field = joined_flag(rest, "--field");
            let group_by = joined_flag(rest, "--group_by");
            let conds = match joined_flag(rest, "--filter") {
                Some(f) => {
                    let parsed: Value = serde_json::from_str(&f).map_err(|_| {
                        err_json("invalid --filter json", raw, "srv records aggregate", "srv records aggregate <board_id> --op sum")
                    })?;
                    parse_filter(&parsed)
                        .map_err(|e| err_json(&e.to_string(), raw, "srv records aggregate", "srv records aggregate <board_id> --op sum"))?
                }
                None => SrvFilter::default(),
            };
            let rows = db
                .srv_record_aggregate(&board_id, &conds.conds, agg, field.as_deref(), group_by.as_deref())
                .await
                .map_err(|e| err_json(&e.to_string(), raw, "srv records aggregate", "srv records aggregate <board_id> --op sum"))?;
            Ok(json!({ "status": "ok", "aggregate": rows }))
        }
        "count" => {
            let count = db
                .srv_record_count(board_id)
                .await
                .map_err(|e| err_json(&e.to_string(), raw, "srv records count", "srv records count <board_id>"))?;
            Ok(json!({ "status": "ok", "count": count }))
        }
        "delete" => {
            let seq: Option<i64> = if has_flag(rest, "--all") {
                None
            } else if let Some(v) = val_after(rest, "--seq") {
                Some(v.parse().map_err(|_| {
                    err_json("invalid --seq value", raw, "srv records delete --seq N", "srv records delete <board_id> --all")
                })?)
            } else {
                return Err(err_json(
                    "expected '--seq <n>' or '--all'",
                    raw,
                    "srv records delete <board_id> [--seq N | --all]",
                    "srv records delete <board_id> --all",
                ));
            };
            db.srv_record_delete(board_id, seq)
                .await
                .map_err(|e| err_json(&e.to_string(), raw, "srv records delete", "srv records delete <board_id>"))?;
            Ok(json!({ "status": "ok", "deleted": true }))
        }
        other => Err(err_json(
            &format!("unknown records verb '{other}'"),
            raw,
            "srv records <verb>",
            USAGE,
        )),
    }
}

async fn cmd_files(db: &DbPool, dir: &Path, sid: &str, verb: &str, args: &[&String], raw: &str) -> CmdResult {
    if verb == "--help" {
        return Ok(json!({ "status": "ok", "help": FILES_USAGE }));
    }
    let board_id = args
        .first()
        .ok_or_else(|| err_json("missing <board_id>", raw, "srv files <verb>", "srv files list"))?;
    require_board(db, board_id, sid, raw).await?;
    let rest = &args[1..];
    match verb {
        "list" => {
            let files = zw_db::serverless::srv_list_files(dir, sid, board_id)
                .map_err(|e| err_json(&e.to_string(), raw, "srv files list", "srv files list <board_id>"))?;
            let files: Vec<_> = files
                .into_iter()
                .map(|(file, size)| json!({ "file": file, "size": size }))
                .collect();
            Ok(json!({ "status": "ok", "files": files }))
        }
        "get" => {
            let file_id = rest.first().ok_or_else(|| {
                err_json(
                    "missing <file_id>",
                    raw,
                    "srv files get <board_id> <file_id> --out <path>",
                    "srv files get <board_id> <file_id> --out /tmp/x",
                )
            })?;
            let out = val_after(rest, "--out").ok_or_else(|| {
                err_json(
                    "missing --out <path>",
                    raw,
                    "srv files get <board_id> <file_id> --out <path>",
                    "srv files get <board_id> <file_id> --out /tmp/x",
                )
            })?;
            let src = zw_db::serverless::srv_board_file_path(dir, sid, board_id, file_id);
            if !src.is_file() {
                return Err(err_json(
                    "file does not exist",
                    raw,
                    "srv files get <board_id> <file_id>",
                    "srv files list <board_id>",
                ));
            }
            if let Some(parent) = Path::new(&out).parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)
                        .map_err(|e| err_json(&e.to_string(), raw, "srv files get --out", "srv files get --out /tmp/x"))?;
                }
            }
            std::fs::copy(&src, &out)
                .map_err(|e| err_json(&e.to_string(), raw, "srv files get", "srv files get <board_id> <file_id>"))?;
            Ok(json!({ "status": "ok", "saved": out }))
        }
        other => Err(err_json(
            &format!("unknown files verb '{other}'"),
            raw,
            "srv files <verb>",
            USAGE,
        )),
    }
}

pub async fn execute_srv(
    db: &DbPool,
    dir: &Path,
    sid: &str,
    rest: &[&String],
    raw: &str,
) -> Result<serde_json::Value, serde_json::Value> {
    let group = rest
        .first()
        .ok_or_else(|| err_json("missing group: apps|records|files", raw, "srv <group> <verb>", "srv apps list"))?;
    let verb = rest
        .get(1)
        .ok_or_else(|| err_json("missing verb", raw, "srv <group> <verb>", "srv apps list"))?;
    let args = &rest[2..];
    match group.as_str() {
        "apps" => cmd_apps(db, sid, verb, args, raw).await,
        "records" => cmd_records(db, sid, verb, args, raw).await,
        "files" => cmd_files(db, dir, sid, verb, args, raw).await,
        other => Err(err_json(&format!("unknown group '{other}'"), raw, "srv <group> <verb>", USAGE)),
    }
}