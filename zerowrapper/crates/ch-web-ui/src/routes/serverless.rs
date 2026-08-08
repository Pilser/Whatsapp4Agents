use topcoat::Result;
use topcoat::router::{Body, Response, StatusCode, route, path_param, to_bytes, headers, uri};
use topcoat::context::{Cx, app_context};

use crate::context::AppContext;
use zw_db::serverless::{Agg, FilterCond, Op, gen_file_id, parse_filter, srv_board_file_path, srv_board_files_dir};

#[path_param]
pub(crate) struct BoardId(str);

#[path_param]
struct Seq(str);

fn json_response(status: StatusCode, v: serde_json::Value) -> Response {
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(Body::from(v.to_string()))
        .unwrap()
}

fn query_params(cx: &Cx) -> std::collections::HashMap<&str, &str> {
    uri(cx)
        .query()
        .unwrap_or("")
        .split('&')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next()?;
            let val = parts.next().unwrap_or("");
            Some((key, val))
        })
        .collect()
}

fn params_to_json<'a>(records: &[zw_db::serverless::SrvRecord]) -> Vec<serde_json::Value> {
    records
        .iter()
        .map(|r| {
            let mut v = serde_json::json!({"seq": r.seq, "payload": r.payload, "created_at": r.created_at});
            if let Some(score) = r.score {
                v["score"] = serde_json::json!(score);
            }
            if let Some(w) = &r.writer {
                v["writer"] = serde_json::json!(w);
            }
            if let Some(s) = &r.snippet {
                v["snippet"] = serde_json::json!(s);
            }
            v
        })
        .collect()
}

fn url_decode(s: &str) -> String {
    fn hex(b: u8) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push((h << 4) | l);
                i += 3;
                continue;
            }
        } else if bytes[i] == b'+' {
            out.push(b' ');
            i += 1;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

#[route(POST "/api/srv/{board_id}/submit")]
pub async fn srv_submit(cx: &Cx, body: Body) -> Result<Response> {
    let ctx: &AppContext = app_context(cx);
    let board_id = path_param::<BoardId>(cx);
    let params = query_params(cx);
    let token = bearer_token(cx);
    if let Err(resp) = rate_action(cx, ctx, board_id, &token, "submit").await {
        return Ok(resp);
    }
    let body_bytes = match to_bytes(body, 1_000_000).await {
        Ok(b) => b,
        Err(_) => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "invalid json"}))),
    };
    let payload: serde_json::Value = match serde_json::from_slice(&body_bytes) {
        Ok(p) => p,
        Err(_) => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "invalid json"}))),
    };

    match ctx.db.srv_app_by_id(board_id).await {
        Ok(Some(_)) => {}
        _ => return Ok(json_response(StatusCode::NOT_FOUND, serde_json::json!({"error": "not found"}))),
    }

    if let Err(e) = ctx.db.srv_validate(board_id, &payload).await {
        return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": e.to_string()})));
    }

    let identity = match authorize_write(cx, ctx, board_id, true).await {
        Ok(id) => id,
        Err(resp) => return Ok(resp),
    };
    let writer = request_writer(cx, &identity);

    let upsert = params.get("upsert").is_some()
        || matches!(params.get("upsert").map(|v| *v), Some("1") | Some("true"));

    match ctx.db.srv_record_insert(board_id, &payload, writer.as_deref(), upsert).await {
        Ok(seq) => {
            publish_record_event(ctx, board_id, "created", seq).await;
            Ok(json_response(StatusCode::CREATED, serde_json::json!({"ok": true, "seq": seq})))
        }
        Err(e) => {
            tracing::warn!("srv_submit: insert error: {}", e);
            Ok(json_response(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({"error": "insert failed"})))
        }
    }
}

async fn require_public_readable(ctx: &AppContext, board_id: &str) -> Result<Option<zw_db::serverless::SrvApp>, Response> {
    match ctx.db.srv_app_by_id(board_id).await {
        Ok(Some(app)) if app.public_reads => Ok(Some(app)),
        Ok(Some(_)) => Err(json_response(StatusCode::FORBIDDEN, serde_json::json!({"error": "private board"}))),
        _ => Err(json_response(StatusCode::NOT_FOUND, serde_json::json!({"error": "not found"}))),
    }
}

pub(crate) enum SrvIdentity {
    LegacyAdmin,
    AppKey { role: String, writer: Option<String> },
}

fn role_ge(role: &str, need: &str) -> bool {
    fn rank(r: &str) -> i32 {
        match r {
            "admin" => 3,
            "writer" => 2,
            "list" => 1,
            "reader" => 1,
            _ => 0,
        }
    }
    rank(role) >= rank(need)
}

fn bearer_token(cx: &Cx) -> Option<String> {
    let auth = headers(cx).get("Authorization").and_then(|v| v.to_str().ok());
    let token = auth.and_then(|s| {
        let mut parts = s.splitn(2, ' ');
        if parts.next()?.eq_ignore_ascii_case("bearer") {
            let rest = parts.next()?.trim();
            if rest.is_empty() {
                None
            } else {
                Some(rest.to_string())
            }
        } else {
            None
        }
    });
    token.or_else(|| {
        query_params(cx)
            .get("key")
            .map(|k| k.to_string())
            .filter(|k| !k.is_empty())
    })
}

fn rate_client_key(cx: &Cx, board_id: &str, token: &Option<String>) -> String {
    if let Some(t) = token {
        return format!("{}:kw:{}", board_id, &zw_db::rate::token_digest(t)[..16]);
    }
    let ip = headers(cx)
        .get("X-Forwarded-For")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next().map(str::trim))
        .filter(|ip| !ip.is_empty());
    match ip {
        Some(ip) => format!("{}:ip:{}", board_id, ip),
        None => format!("{}:anon", board_id),
    }
}

async fn rate_action(cx: &Cx, ctx: &AppContext, board_id: &str, token: &Option<String>, action: &str) -> Result<(), Response> {
    let cfg = match ctx.db.srv_rate_config(board_id).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("rate_action: config error: {}", e);
            return Err(json_response(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({"error": "rate limit failed"})));
        }
    };
    let key = rate_client_key(cx, board_id, token);
    let (allowed, retry_after, remaining) = ctx.rates.allow(&key, action, cfg.limit(action), cfg.per_day).await;
    if allowed {
        return Ok(());
    }
    Err(Response::builder()
        .status(StatusCode::TOO_MANY_REQUESTS)
        .header("Retry-After", retry_after.to_string())
        .header("X-RateLimit-Remaining", remaining.to_string())
        .body(Body::from(serde_json::json!({"error": "rate limited", "retry_after": retry_after}).to_string()))
        .unwrap())
}

async fn authorize_identity(cx: &Cx, ctx: &AppContext, board_id: &str) -> Result<Option<SrvIdentity>, Response> {
    let Some(token) = bearer_token(cx) else {
        return Ok(None);
    };
    if token == board_id {
        return Ok(Some(SrvIdentity::LegacyAdmin));
    }
    match ctx.db.srv_key_auth(&token).await {
        Ok(Some(key)) if key.board_id == board_id => {
            Ok(Some(SrvIdentity::AppKey { role: key.role, writer: key.writer }))
        }
        Ok(Some(_)) => Err(json_response(StatusCode::FORBIDDEN, serde_json::json!({"error": "key not valid for this board"}))),
        Ok(None) => Err(json_response(StatusCode::FORBIDDEN, serde_json::json!({"error": "unauthorized"}))),
        Err(e) => {
            tracing::warn!("authorize_identity: auth error: {}", e);
            Err(json_response(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({"error": "auth failed"})))
        }
    }
}

async fn authorize_write(cx: &Cx, ctx: &AppContext, board_id: &str, enforce: bool) -> Result<Option<SrvIdentity>, Response> {
    let identity = match authorize_identity(cx, ctx, board_id).await {
        Ok(id) => id,
        Err(resp) => return Err(resp),
    };
    if let Some(SrvIdentity::AppKey { role, .. }) = &identity {
        if !role_ge(role, "writer") {
            return Err(json_response(StatusCode::FORBIDDEN, serde_json::json!({"error": "write requires an authorization key"})));
        }
    }
    if enforce && identity.is_none() {
        let has_keys = match ctx.db.srv_board_has_keys(board_id).await {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("authorize_write: board has_keys error: {}", e);
                return Err(json_response(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({"error": "auth check failed"})));
            }
        };
        if has_keys {
            return Err(json_response(StatusCode::FORBIDDEN, serde_json::json!({"error": "write requires an authorization key"})));
        }
    }
    Ok(identity)
}

fn request_writer(cx: &Cx, identity: &Option<SrvIdentity>) -> Option<String> {
    if let Some(SrvIdentity::AppKey { writer: Some(w), .. }) = identity {
        return Some(w.clone());
    }
    headers(cx)
        .get("X-Writer")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .or_else(|| query_params(cx).get("writer").map(|w| w.to_string()))
        .filter(|w| !w.is_empty())
}

pub(crate) async fn authorize_read(cx: &Cx, ctx: &AppContext, board_id: &str) -> Result<Option<SrvIdentity>, Response> {
    let identity = match authorize_identity(cx, ctx, board_id).await {
        Ok(id) => id,
        Err(resp) => return Err(resp),
    };
    match &identity {
        Some(SrvIdentity::LegacyAdmin) => Ok(identity),
        Some(SrvIdentity::AppKey { role, .. }) if role_ge(role, "reader") => Ok(identity),
        Some(SrvIdentity::AppKey { .. }) => {
            Err(json_response(StatusCode::FORBIDDEN, serde_json::json!({"error": "read requires an authorization key"})))
        }
        None => match require_public_readable(ctx, board_id).await {
            Ok(_) => Ok(None),
            Err(resp) => Err(resp),
        },
    }
}

#[route(GET "/api/srv/{board_id}/records")]
pub async fn srv_records(cx: &Cx) -> Result<Response> {
    let ctx: &AppContext = app_context(cx);
    let board_id = path_param::<BoardId>(cx);
    let params = query_params(cx);
    let token = bearer_token(cx);
    let action = match params.get("q").map(|q| url_decode(q)).filter(|q| !q.is_empty()) {
        Some(_) => "search",
        None => "read",
    };
    if let Err(resp) = rate_action(cx, ctx, board_id, &token, action).await {
        return Ok(resp);
    }
    let limit: i64 = params.get("limit").and_then(|l| l.parse().ok()).unwrap_or(50).clamp(1, 500);
    let offset: i64 = params.get("offset").and_then(|v| v.parse().ok()).unwrap_or(0).max(0);
    let hl = params.get("hl").and_then(|v| if *v == "1" || *v == "on" || *v == "true" { Some(()) } else { None }).is_some();
    let writer = params.get("writer").map(|w| url_decode(w)).filter(|w| !w.is_empty());
    let order = params.get("order").map(|o| url_decode(o)).filter(|o| !o.is_empty()).map(|o| {
        let desc = match params.get("dir").map(|d| *d) {
            Some("asc") => false,
            Some("desc") => true,
            _ => false,
        };
        (o, desc)
    });

    if let Err(resp) = authorize_read(cx, ctx, board_id).await {
        return Ok(resp);
    }

    let records = if let Some(q) = params.get("q").map(|q| url_decode(q)).filter(|q| !q.is_empty()) {
        let mut conds = if let Some(f) = params.get("filter").map(|f| url_decode(f)).filter(|f| !f.is_empty()) {
            let parsed: serde_json::Value = match serde_json::from_str(&f) {
                Ok(v) => v,
                Err(_) => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "invalid filter json"}))),
            };
            match parse_filter(&parsed) {
                Ok(sf) => sf.conds,
                Err(e) => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": e.to_string()}))),
            }
        } else {
            Vec::new()
        };
        if let Some(w) = &writer {
            conds.insert(0, FilterCond { field: "$.writer".to_string(), op: Op::Eq, value: serde_json::json!(w) });
        }
        match ctx.db.srv_record_search(board_id, &q, &conds, limit, offset, hl).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("srv_records: search error: {}", e);
                return Ok(json_response(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({"error": "search failed"})));
            }
        }
    } else if let Some(f) = params.get("filter").map(|f| url_decode(f)).filter(|f| !f.is_empty()) {
        let parsed: serde_json::Value = match serde_json::from_str(&f) {
            Ok(v) => v,
            Err(_) => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "invalid filter json"}))),
        };
        let mut filter = match parse_filter(&parsed) {
            Ok(sf) => sf,
            Err(e) => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": e.to_string()}))),
        };
        if let Some(w) = &writer {
            filter.conds.push(FilterCond { field: "$.writer".to_string(), op: Op::Eq, value: serde_json::json!(w) });
        }
        match ctx
            .db
            .srv_record_filter(board_id, &filter.conds, order.as_ref().map(|(p, d)| (p.as_str(), *d)), limit, offset)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("srv_records: filter error: {}", e);
                return Ok(json_response(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({"error": "query failed"})));
            }
        }
    } else if let Some(w) = &writer {
        let conds = vec![FilterCond { field: "$.writer".to_string(), op: Op::Eq, value: serde_json::json!(w) }];
        match ctx.db.srv_record_filter(board_id, &conds, None, limit, offset).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("srv_records: writer filter error: {}", e);
                return Ok(json_response(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({"error": "query failed"})));
            }
        }
    } else {
        match ctx.db.srv_record_list(board_id, limit, None, offset).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("srv_records: list error: {}", e);
                return Ok(json_response(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({"error": "list failed"})));
            }
        }
    };

    Ok(json_response(StatusCode::OK, serde_json::json!({"ok": true, "records": params_to_json(&records)})))
}

#[route(GET "/api/srv/{board_id}/aggregate")]
pub async fn srv_aggregate(cx: &Cx) -> Result<Response> {
    let ctx: &AppContext = app_context(cx);
    let board_id = path_param::<BoardId>(cx);
    let params = query_params(cx);
    let token = bearer_token(cx);
    if let Err(resp) = rate_action(cx, ctx, board_id, &token, "read").await {
        return Ok(resp);
    }
    let agg = match params.get("op").and_then(|o| Agg::parse(o).ok()) {
        Some(a) => a,
        None => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "missing or invalid op (count, sum, avg, min, max)"}))),
    };
    let field = params.get("field").map(|s| s.to_string());
    let group_by = params.get("group_by").map(|s| s.to_string());

    if let Err(resp) = authorize_read(cx, ctx, board_id).await {
        return Ok(resp);
    }

    let conds = if let Some(f) = params.get("filter").map(|f| url_decode(f)).filter(|f| !f.is_empty()) {
        let parsed: serde_json::Value = match serde_json::from_str(&f) {
            Ok(v) => v,
            Err(_) => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "invalid filter json"}))),
        };
        match parse_filter(&parsed) {
            Ok(sf) => sf.conds,
            Err(e) => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": e.to_string()}))),
        }
    } else {
        Vec::new()
    };

    match ctx
        .db
        .srv_record_aggregate(board_id, &conds, agg, field.as_deref(), group_by.as_deref())
        .await
    {
        Ok(rows) => Ok(json_response(StatusCode::OK, serde_json::json!({"ok": true, "aggregate": rows}))),
        Err(e) => {
            tracing::warn!("srv_aggregate: error: {}", e);
            Ok(json_response(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({"error": "aggregate failed"})))
        }
    }
}

#[route(GET "/api/srv/{board_id}/record")]
pub async fn srv_record(cx: &Cx) -> Result<Response> {
    let ctx: &AppContext = app_context(cx);
    let board_id = path_param::<BoardId>(cx);
    let params = query_params(cx);
    let token = bearer_token(cx);
    if let Err(resp) = rate_action(cx, ctx, board_id, &token, "read").await {
        return Ok(resp);
    }

    let seq: i64 = match params.get("seq").and_then(|s| s.parse().ok()) {
        Some(s) => s,
        None => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "missing or invalid seq"}))),
    };

    if let Err(resp) = authorize_read(cx, ctx, board_id).await {
        return Ok(resp);
    }

    match ctx.db.srv_record_get(board_id, seq).await {
        Ok(Some(record)) => {
            let records_json = params_to_json(&[record]);
            let rec = records_json[0].clone();
            Ok(json_response(StatusCode::OK, serde_json::json!({"ok": true, "record": rec})))
        }
        Ok(None) => Ok(json_response(StatusCode::NOT_FOUND, serde_json::json!({"error": "not found"}))),
        Err(e) => {
            tracing::warn!("srv_record: get error: {}", e);
            Ok(json_response(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({"error": "get failed"})))
        }
    }
}

#[route(PUT "/api/srv/{board_id}/records/{seq}")]
pub async fn srv_record_put(cx: &Cx, body: Body) -> Result<Response> {
    let ctx: &AppContext = app_context(cx);
    let board_id = path_param::<BoardId>(cx);
    let token = bearer_token(cx);
    if let Err(resp) = rate_action(cx, ctx, board_id, &token, "submit").await {
        return Ok(resp);
    }
    let seq: i64 = match path_param::<Seq>(cx).parse() {
        Ok(s) => s,
        Err(_) => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "invalid seq"}))),
    };

    match ctx.db.srv_app_by_id(board_id).await {
        Ok(Some(_)) => {}
        _ => return Ok(json_response(StatusCode::NOT_FOUND, serde_json::json!({"error": "not found"}))),
    }

    let identity = match authorize_write(cx, ctx, board_id, true).await {
        Ok(id) => id,
        Err(resp) => return Ok(resp),
    };

    let body_bytes = match to_bytes(body, 1_000_000).await {
        Ok(b) => b,
        Err(_) => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "invalid json"}))),
    };
    let payload: serde_json::Value = match serde_json::from_slice(&body_bytes) {
        Ok(p) => p,
        Err(_) => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "invalid json"}))),
    };

    if let Err(e) = ctx.db.srv_validate(board_id, &payload).await {
        return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": e.to_string()})));
    }

    match ctx.db.srv_record_get(board_id, seq).await {
        Ok(Some(_)) => {}
        Ok(None) => return Ok(json_response(StatusCode::NOT_FOUND, serde_json::json!({"error": "not found"}))),
        Err(e) => {
            tracing::warn!("srv_record_put: get error: {}", e);
            return Ok(json_response(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({"error": "get failed"})));
        }
    }

    let writer = request_writer(cx, &identity);

    match ctx.db.srv_record_set(board_id, seq, &payload, writer.as_deref()).await {
        Ok(()) => {
            publish_record_event(ctx, board_id, "updated", seq).await;
            Ok(json_response(StatusCode::OK, serde_json::json!({"ok": true, "seq": seq})))
        }
        Err(e) => {
            tracing::warn!("srv_record_put: set error: {}", e);
            Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": e.to_string()})))
        }
    }
}

#[route(PATCH "/api/srv/{board_id}/records/{seq}")]
pub async fn srv_record_patch_route(cx: &Cx, body: Body) -> Result<Response> {
    let ctx: &AppContext = app_context(cx);
    let board_id = path_param::<BoardId>(cx);
    let token = bearer_token(cx);
    if let Err(resp) = rate_action(cx, ctx, board_id, &token, "submit").await {
        return Ok(resp);
    }
    let seq: i64 = match path_param::<Seq>(cx).parse() {
        Ok(s) => s,
        Err(_) => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "invalid seq"}))),
    };

    match ctx.db.srv_app_by_id(board_id).await {
        Ok(Some(_)) => {}
        _ => return Ok(json_response(StatusCode::NOT_FOUND, serde_json::json!({"error": "not found"}))),
    }

    let _identity = match authorize_write(cx, ctx, board_id, true).await {
        Ok(id) => id,
        Err(resp) => return Ok(resp),
    };

    let body_bytes = match to_bytes(body, 1_000_000).await {
        Ok(b) => b,
        Err(_) => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "invalid json"}))),
    };
    let ops: serde_json::Value = match serde_json::from_slice(&body_bytes) {
        Ok(p) => p,
        Err(_) => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "invalid json"}))),
    };

    match ctx.db.srv_record_get(board_id, seq).await {
        Ok(Some(_)) => {}
        Ok(None) => return Ok(json_response(StatusCode::NOT_FOUND, serde_json::json!({"error": "not found"}))),
        Err(e) => {
            tracing::warn!("srv_record_patch_route: get error: {}", e);
            return Ok(json_response(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({"error": "get failed"})));
        }
    }

    match ctx.db.srv_record_patch(board_id, seq, &ops).await {
        Ok(merged) => {
            publish_record_event(ctx, board_id, "updated", seq).await;
            Ok(json_response(StatusCode::OK, serde_json::json!({"ok": true, "seq": seq, "payload": merged})))
        }
        Err(e) => Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": e.to_string()}))),
    }
}

async fn persist_upload(
    ctx: &AppContext,
    board_id: &str,
    app: &zw_db::serverless::SrvApp,
    filename: &str,
    content_type: &str,
    data: &[u8],
    extra: serde_json::Map<String, serde_json::Value>,
    writer: Option<&str>,
) -> Response {
    let handle = match ctx.whiteboard() {
        Some(h) => h,
        None => return json_response(StatusCode::SERVICE_UNAVAILABLE, serde_json::json!({"error": "whiteboard unavailable"})),
    };
    let root = handle.dir();

    let file_id = gen_file_id();
    let dest_dir = srv_board_files_dir(root, &app.owner_key, board_id);
    if let Err(e) = tokio::fs::create_dir_all(&dest_dir).await {
        tracing::warn!("srv_upload: create dir error: {}", e);
        return json_response(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({"error": "save failed"}));
    }
    let dest_path = srv_board_file_path(root, &app.owner_key, board_id, &file_id);
    if let Err(e) = tokio::fs::write(&dest_path, data).await {
        tracing::warn!("srv_upload: write error: {}", e);
        return json_response(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({"error": "save failed"}));
    }

    let mut payload = extra;
    payload.insert("file".to_string(), serde_json::json!(format!("files/{}.bin", file_id)));
    payload.insert("name".to_string(), serde_json::json!(filename));
    payload.insert("type".to_string(), serde_json::json!(content_type));
    payload.insert("size".to_string(), serde_json::json!(data.len()));

    match ctx.db.srv_record_insert(board_id, &serde_json::Value::Object(payload), writer, false).await {
        Ok(seq) => {
            publish_record_event(ctx, board_id, "created", seq).await;
            json_response(
                StatusCode::CREATED,
                serde_json::json!({"ok": true, "seq": seq, "file": format!("files/{}.bin", file_id)}),
            )
        },
        Err(e) => {
            tracing::warn!("srv_upload: insert error: {}", e);
            json_response(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({"error": "insert failed"}))
        }
    }
}

#[route(POST "/api/srv/{board_id}/upload")]
pub async fn srv_upload(cx: &Cx, body: Body) -> Result<Response> {
    let ctx: &AppContext = app_context(cx);
    let board_id = path_param::<BoardId>(cx);
    let token = bearer_token(cx);
    if let Err(resp) = rate_action(cx, ctx, board_id, &token, "upload").await {
        return Ok(resp);
    }

    let app = match ctx.db.srv_app_by_id(board_id).await {
        Ok(Some(app)) => app,
        _ => return Ok(json_response(StatusCode::NOT_FOUND, serde_json::json!({"error": "not found"}))),
    };

    let identity = match authorize_write(cx, ctx, board_id, true).await {
        Ok(id) => id,
        Err(resp) => return Ok(resp),
    };
    let writer = request_writer(cx, &identity);

    let content_type = headers(cx)
        .get("Content-Type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    let body_bytes = match to_bytes(body, 25_000_000).await {
        Ok(b) => b,
        Err(_) => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "invalid request"}))),
    };

    if body_bytes.is_empty() {
        return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "empty file"})));
    }

    if let Some(boundary) = crate::routes::multipart::boundary_from_content_type(&content_type) {
        let parts = match crate::routes::multipart::parse(&body_bytes, &boundary) {
            Ok(p) => p,
            Err(e) => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": format!("multipart: {e}")}))),
        };
        let mut extra = serde_json::Map::new();
        let mut file_field: Option<crate::routes::multipart::MultipartField> = None;
        for part in parts {
            if part.filename.is_some() {
                if file_field.is_none() {
                    file_field = Some(part);
                }
            } else if !part.name.is_empty() {
                extra.insert(part.name, serde_json::Value::String(String::from_utf8_lossy(&part.data).to_string()));
            }
        }
        let Some(file) = file_field else {
            return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "no file part in multipart form"})));
        };
        let filename = file.filename.unwrap_or_else(|| "upload".to_string());
        let file_type = file.content_type.unwrap_or_else(|| "application/octet-stream".to_string());
        return Ok(persist_upload(ctx, board_id, &app, &filename, &file_type, &file.data, extra, writer.as_deref()).await);
    }

    let filename = headers(cx)
        .get("X-Filename")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("upload")
        .to_string();

    Ok(persist_upload(ctx, board_id, &app, &filename, &content_type, &body_bytes, serde_json::Map::new(), writer.as_deref()).await)
}

#[route(GET "/api/srv/{board_id}/file")]
pub async fn srv_file(cx: &Cx) -> Result<Response> {
    let ctx: &AppContext = app_context(cx);
    let board_id = path_param::<BoardId>(cx);
    let params = query_params(cx);

    let file = match params.get("file") {
        Some(f) if f.starts_with("files/") && f.ends_with(".bin") => f.to_string(),
        _ => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "invalid file"}))),
    };
    let file_id = file
        .strip_prefix("files/")
        .and_then(|rest| rest.strip_suffix(".bin"))
        .unwrap_or_default()
        .to_string();
    if !file_id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "invalid file"})));
    }

    let app = match ctx.db.srv_app_by_id(board_id).await {
        Ok(Some(app)) if app.public_reads => app,
        Ok(Some(_)) => return Ok(json_response(StatusCode::FORBIDDEN, serde_json::json!({"error": "private board"}))),
        _ => return Ok(json_response(StatusCode::NOT_FOUND, serde_json::json!({"error": "not found"}))),
    };

    let handle = match ctx.whiteboard() {
        Some(h) => h,
        None => return Ok(json_response(StatusCode::SERVICE_UNAVAILABLE, serde_json::json!({"error": "whiteboard unavailable"}))),
    };
    let root = handle.dir();
    let files_dir = srv_board_files_dir(root, &app.owner_key, board_id);
    let path = srv_board_file_path(root, &app.owner_key, board_id, &file_id);
    if !path.starts_with(&files_dir) {
        return Ok(json_response(StatusCode::FORBIDDEN, serde_json::json!({"error": "forbidden"})));
    }
    if !path.is_file() {
        return Ok(json_response(StatusCode::NOT_FOUND, serde_json::json!({"error": "not found"})));
    }

    let mut content_type = "application/octet-stream".to_string();
    let mut filename = file_id.clone();
    if let Ok(records) = ctx.db.srv_record_query(board_id, "$.file", Some(&file)).await {
        if let Some(first) = records.first() {
            if let Some(t) = first.payload["type"].as_str() {
                content_type = t.to_string();
            }
            if let Some(n) = first.payload["name"].as_str() {
                filename = n.to_string();
            }
        }
    }

    let data = match tokio::fs::read(&path).await {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("srv_file: read error: {}", e);
            return Ok(json_response(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({"error": "read failed"})));
        }
    };

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", content_type)
        .header("Content-Disposition", format!("inline; filename=\"{}\"", filename))
        .body(Body::from(data))
        .unwrap())
}

#[route(POST "/api/srv/{board_id}/keys")]
pub async fn srv_keys_issue(cx: &Cx, body: Body) -> Result<Response> {
    let ctx: &AppContext = app_context(cx);
    let board_id = path_param::<BoardId>(cx);
    let token = bearer_token(cx);
    if let Err(resp) = rate_action(cx, ctx, board_id, &token, "submit").await {
        return Ok(resp);
    }

    let identity = match authorize_identity(cx, ctx, board_id).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return Ok(json_response(StatusCode::FORBIDDEN, serde_json::json!({"error": "admin authorization required"})))
        }
        Err(resp) => return Ok(resp),
    };
    let is_admin = match &identity {
        SrvIdentity::LegacyAdmin => true,
        SrvIdentity::AppKey { role, .. } => role == "admin",
    };
    if !is_admin {
        return Ok(json_response(StatusCode::FORBIDDEN, serde_json::json!({"error": "admin authorization required"})));
    }

    let app = match ctx.db.srv_app_by_id(board_id).await {
        Ok(Some(app)) => app,
        _ => return Ok(json_response(StatusCode::NOT_FOUND, serde_json::json!({"error": "not found"}))),
    };

    let body_bytes = match to_bytes(body, 1_000_000).await {
        Ok(b) => b,
        Err(_) => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "invalid json"}))),
    };
    let req: serde_json::Value = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(_) => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "invalid json"}))),
    };
    let role = req.get("role").and_then(|r| r.as_str()).unwrap_or("writer").to_string();
    let writer = req.get("writer").and_then(|w| w.as_str()).map(str::to_string);

    match ctx.db.srv_key_issue(board_id, &app.owner_key, &role, writer.as_deref()).await {
        Ok((bucket, secret)) => Ok(json_response(
            StatusCode::CREATED,
            serde_json::json!({"ok": true, "bucket": bucket, "key": secret, "role": role, "writer": writer}),
        )),
        Err(e) => Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": e.to_string()}))),
    }
}

#[route(POST "/api/srv/{board_id}/hooks")]
pub async fn srv_hooks_register(cx: &Cx, body: Body) -> Result<Response> {
    let ctx: &AppContext = app_context(cx);
    let board_id = path_param::<BoardId>(cx);

    let identity = match authorize_identity(cx, ctx, board_id).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return Ok(json_response(StatusCode::FORBIDDEN, serde_json::json!({"error": "admin authorization required"})))
        }
        Err(resp) => return Ok(resp),
    };
    let is_admin = match &identity {
        SrvIdentity::LegacyAdmin => true,
        SrvIdentity::AppKey { role, .. } => role == "admin",
    };
    if !is_admin {
        return Ok(json_response(StatusCode::FORBIDDEN, serde_json::json!({"error": "admin authorization required"})));
    }

    let app = match ctx.db.srv_app_by_id(board_id).await {
        Ok(Some(app)) => app,
        _ => return Ok(json_response(StatusCode::NOT_FOUND, serde_json::json!({"error": "not found"}))),
    };

    let body_bytes = match to_bytes(body, 1_000_000).await {
        Ok(b) => b,
        Err(_) => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "invalid json"}))),
    };
    let req: serde_json::Value = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(_) => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "invalid json"}))),
    };
    let url = match req.get("url").and_then(|u| u.as_str()) {
        Some(u) if u.starts_with("http://") || u.starts_with("https://") => u.to_string(),
        _ => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "url must start with http:// or https://"}))),
    };
    let secret = req.get("secret").and_then(|s| s.as_str()).filter(|s| !s.is_empty()).map(str::to_string);

    match ctx.db.srv_hook_register(board_id, &app.owner_key, &url, secret.as_deref()).await {
        Ok(()) => Ok(json_response(StatusCode::CREATED, serde_json::json!({"ok": true}))),
        Err(e) => Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": e.to_string()}))),
    }
}

#[route(DELETE "/api/srv/{board_id}/hooks")]
pub async fn srv_hooks_remove(cx: &Cx) -> Result<Response> {
    let ctx: &AppContext = app_context(cx);
    let board_id = path_param::<BoardId>(cx);
    let params = query_params(cx);

    let identity = match authorize_identity(cx, ctx, board_id).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return Ok(json_response(StatusCode::FORBIDDEN, serde_json::json!({"error": "admin authorization required"})))
        }
        Err(resp) => return Ok(resp),
    };
    let is_admin = match &identity {
        SrvIdentity::LegacyAdmin => true,
        SrvIdentity::AppKey { role, .. } => role == "admin",
    };
    if !is_admin {
        return Ok(json_response(StatusCode::FORBIDDEN, serde_json::json!({"error": "admin authorization required"})));
    }

    let app = match ctx.db.srv_app_by_id(board_id).await {
        Ok(Some(app)) => app,
        _ => return Ok(json_response(StatusCode::NOT_FOUND, serde_json::json!({"error": "not found"}))),
    };

    let url = params.get("url").map(|u| url_decode(u)).filter(|u| !u.is_empty());
    match url {
        Some(u) => match ctx.db.srv_hook_remove(board_id, &app.owner_key, &u).await {
            Ok(true) => Ok(json_response(StatusCode::OK, serde_json::json!({"ok": true}))),
            Ok(false) => Ok(json_response(StatusCode::NOT_FOUND, serde_json::json!({"error": "hook not found"}))),
            Err(e) => Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": e.to_string()}))),
        },
        None => Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "missing url query param"}))),
    }
}

#[route(POST "/api/srv/{board_id}/call")]
pub async fn srv_call(cx: &Cx, body: Body) -> Result<Response> {
    let ctx: &AppContext = app_context(cx);
    let board_id = path_param::<BoardId>(cx);

    let identity = match authorize_identity(cx, ctx, board_id).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return Ok(json_response(StatusCode::FORBIDDEN, serde_json::json!({"error": "authorization required"})))
        }
        Err(resp) => return Ok(resp),
    };
    let is_admin = match &identity {
        SrvIdentity::LegacyAdmin => true,
        SrvIdentity::AppKey { role, .. } => role == "admin",
    };
    if !is_admin {
        return Ok(json_response(StatusCode::FORBIDDEN, serde_json::json!({"error": "admin authorization required"})));
    }

    let app = match ctx.db.srv_app_by_id(board_id).await {
        Ok(Some(app)) => app,
        _ => return Ok(json_response(StatusCode::NOT_FOUND, serde_json::json!({"error": "not found"}))),
    };

    let body_bytes = match to_bytes(body, 1_000_000).await {
        Ok(b) => b,
        Err(_) => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "invalid json"}))),
    };
    let req: serde_json::Value = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(_) => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "invalid json"}))),
    };

    let url = match req.get("url").and_then(|u| u.as_str()) {
        Some(u) if !u.is_empty() => u.to_string(),
        _ => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "missing url"}))),
    };
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("GET").to_string();
    let headers: Vec<(String, String)> = match req.get("headers").and_then(|h| h.as_object()) {
        Some(map) => map.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect(),
        None => Vec::new(),
    };
    let body_str = match req.get("body") {
        Some(serde_json::Value::String(s)) => Some(s.clone()),
        Some(v) => Some(v.to_string()),
        None => None,
    };
    let timeout_ms = req.get("timeout_ms").and_then(|t| t.as_u64()).unwrap_or(0);

    match ctx
        .db
        .srv_http_call(board_id, &app.owner_key, &method, &url, &headers, body_str.as_deref(), timeout_ms)
        .await
    {
        Ok(res) => Ok(json_response(
            StatusCode::OK,
            serde_json::json!({
                "ok": true,
                "status": res.status,
                "duration_ms": res.duration_ms,
                "headers": res.headers,
                "body": res.body,
            }),
        )),
        Err(e) => Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": e.to_string()}))),
    }
}

#[route(POST "/api/srv/{board_id}/rate")]
pub async fn srv_rate_config(cx: &Cx, body: Body) -> Result<Response> {
    let ctx: &AppContext = app_context(cx);
    let board_id = path_param::<BoardId>(cx);

    let identity = match authorize_identity(cx, ctx, board_id).await {
        Ok(Some(id)) => id,
        Ok(None) => {
            return Ok(json_response(StatusCode::FORBIDDEN, serde_json::json!({"error": "admin authorization required"})))
        }
        Err(resp) => return Ok(resp),
    };
    let is_admin = match &identity {
        SrvIdentity::LegacyAdmin => true,
        SrvIdentity::AppKey { role, .. } => role == "admin",
    };
    if !is_admin {
        return Ok(json_response(StatusCode::FORBIDDEN, serde_json::json!({"error": "admin authorization required"})));
    }

    let app = match ctx.db.srv_app_by_id(board_id).await {
        Ok(Some(app)) => app,
        _ => return Ok(json_response(StatusCode::NOT_FOUND, serde_json::json!({"error": "not found"}))),
    };

    let body_bytes = match to_bytes(body, 1_000_000).await {
        Ok(b) => b,
        Err(_) => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "invalid json"}))),
    };
    let req: serde_json::Value = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(_) => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "invalid json"}))),
    };

    if req.get("clear").and_then(|v| v.as_bool()).unwrap_or(false) {
        return match ctx.db.srv_rate_set(board_id, &app.owner_key, None).await {
            Ok(()) => Ok(json_response(StatusCode::OK, serde_json::json!({"ok": true}))),
            Err(e) => Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": e.to_string()}))),
        };
    }

    let mut cfg = serde_json::Map::new();
    for field in ["submit", "upload", "search", "read", "per_day"] {
        if let Some(v) = req.get(field).and_then(|v| v.as_u64()) {
            cfg.insert(field.to_string(), serde_json::json!(v));
        }
    }
    match ctx.db.srv_rate_set(board_id, &app.owner_key, Some(&serde_json::Value::Object(cfg).to_string())).await {
        Ok(()) => Ok(json_response(StatusCode::OK, serde_json::json!({"ok": true}))),
        Err(e) => Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": e.to_string()}))),
    }
}

async fn publish_record_event(ctx: &AppContext, board_id: &str, kind: &str, seq: i64) {
    let record = match ctx.db.srv_record_get(board_id, seq).await {
        Ok(Some(record)) => Some(record),
        _ => None,
    };
    let event = crate::realtime::record_event(kind, board_id, seq, record.as_ref());
    ctx.realtime.publish(&event);
    enqueue_hook_events(ctx, board_id, &event).await;
}

async fn enqueue_hook_events(ctx: &AppContext, board_id: &str, event: &str) {
    match ctx.db.srv_hook_urls(board_id).await {
        Ok(hooks) => {
            for (url, _) in hooks {
                let _ = ctx.db.srv_hook_enqueue(board_id, &url, event).await;
            }
        }
        Err(e) => tracing::warn!("enqueue_hook_events: urls error: {}", e),
    }
}

#[route(DELETE "/api/srv/{board_id}/records/{seq}")]
pub async fn srv_record_delete_route(cx: &Cx) -> Result<Response> {
    let ctx: &AppContext = app_context(cx);
    let board_id = path_param::<BoardId>(cx).to_string();
    let token = bearer_token(cx);
    if let Err(resp) = rate_action(cx, ctx, &board_id, &token, "submit").await {
        return Ok(resp);
    }
    let seq: i64 = match path_param::<Seq>(cx).parse() {
        Ok(s) => s,
        Err(_) => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "invalid seq"}))),
    };
    match ctx.db.srv_app_by_id(&board_id).await {
        Ok(Some(_)) => {}
        _ => return Ok(json_response(StatusCode::NOT_FOUND, serde_json::json!({"error": "not found"}))),
    }
    if let Err(resp) = authorize_write(cx, ctx, &board_id, true).await {
        return Ok(resp);
    }
    match ctx.db.srv_record_delete_one(&board_id, seq).await {
        Ok(true) => {
            let event = crate::realtime::record_event("deleted", &board_id, seq, None);
            ctx.realtime.publish(&event);
            enqueue_hook_events(ctx, &board_id, &event).await;
            Ok(json_response(StatusCode::OK, serde_json::json!({"ok": true, "seq": seq})))
        }
        Ok(false) => Ok(json_response(StatusCode::NOT_FOUND, serde_json::json!({"error": "not found"}))),
        Err(e) => {
            tracing::warn!("srv_record_delete_route: delete error: {}", e);
            Ok(json_response(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({"error": "delete failed"})))
        }
    }
}

#[route(GET "/api/srv/{board_id}/join")]
pub async fn srv_join(cx: &Cx) -> Result<Response> {
    let ctx: &AppContext = app_context(cx);
    let board_id = path_param::<BoardId>(cx).to_string();
    let params = query_params(cx);
    let limit: i64 = params.get("limit").and_then(|l| l.parse().ok()).unwrap_or(50).clamp(1, 500);
    let offset: i64 = params.get("offset").and_then(|v| v.parse().ok()).unwrap_or(0).max(0);

    if let Err(resp) = authorize_read(cx, ctx, &board_id).await {
        return Ok(resp);
    }

    let conds = if let Some(f) = params.get("filter").map(|f| url_decode(f)).filter(|f| !f.is_empty()) {
        let parsed: serde_json::Value = match serde_json::from_str(&f) {
            Ok(v) => v,
            Err(_) => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "invalid filter json"}))),
        };
        match parse_filter(&parsed) {
            Ok(sf) => sf.conds,
            Err(e) => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": e.to_string()}))),
        }
    } else {
        Vec::new()
    };

    match ctx.db.srv_join_list(&board_id, &conds, limit, offset).await {
        Ok(rows) => {
            let mut records = Vec::new();
            for (rec, joined) in rows {
                let mut v = serde_json::json!({"seq": rec.seq, "payload": rec.payload, "created_at": rec.created_at});
                if let Some(j) = joined {
                    v["joined"] = j;
                }
                records.push(v);
            }
            Ok(json_response(StatusCode::OK, serde_json::json!({"ok": true, "records": records})))
        }
        Err(e) => {
            tracing::warn!("srv_join: error: {}", e);
            Ok(json_response(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({"error": "join failed"})))
        }
    }
}

#[route(GET "/api/srv/{board_id}/audit")]
pub async fn srv_audit(cx: &Cx) -> Result<Response> {
    let ctx: &AppContext = app_context(cx);
    let board_id = path_param::<BoardId>(cx).to_string();
    match authorize_identity(cx, ctx, &board_id).await {
        Ok(Some(SrvIdentity::LegacyAdmin)) => {}
        Ok(Some(SrvIdentity::AppKey { role, .. })) if role == "admin" => {}
        Ok(_) => return Ok(json_response(StatusCode::FORBIDDEN, serde_json::json!({"error": "admin authorization required"}))),
        Err(resp) => return Ok(resp),
    }
    let app = match ctx.db.srv_app_by_id(&board_id).await {
        Ok(Some(app)) => app,
        _ => return Ok(json_response(StatusCode::NOT_FOUND, serde_json::json!({"error": "not found"}))),
    };
    let params = query_params(cx);
    let since = params.get("since").map(|s| s.to_string()).filter(|s| !s.is_empty());
    let limit: i64 = params.get("limit").and_then(|l| l.parse().ok()).unwrap_or(100).clamp(1, 1000);
    match ctx.db.srv_audit_list(&board_id, &app.owner_key, since.as_deref(), limit).await {
        Ok(rows) => Ok(json_response(StatusCode::OK, serde_json::json!({"ok": true, "audit": rows}))),
        Err(e) => {
            tracing::warn!("srv_audit: {}", e);
            Ok(json_response(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({"error": "audit failed"})))
        }
    }
}