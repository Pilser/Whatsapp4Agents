use topcoat::Result;
use topcoat::router::{Body, Response, StatusCode, route, path_param, to_bytes, headers, uri};
use topcoat::context::{Cx, app_context};

use crate::context::AppContext;
use zw_db::serverless::{Agg, gen_file_id, parse_filter, srv_board_file_path, srv_board_files_dir};

#[path_param]
struct BoardId(str);

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

    match ctx.db.srv_record_insert(board_id, &payload).await {
        Ok(seq) => Ok(json_response(StatusCode::CREATED, serde_json::json!({"ok": true, "seq": seq}))),
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

#[route(GET "/api/srv/{board_id}/records")]
pub async fn srv_records(cx: &Cx) -> Result<Response> {
    let ctx: &AppContext = app_context(cx);
    let board_id = path_param::<BoardId>(cx);
    let params = query_params(cx);
    let limit: i64 = params.get("limit").and_then(|l| l.parse().ok()).unwrap_or(50).clamp(1, 500);
    let order = params.get("order").map(|o| url_decode(o)).filter(|o| !o.is_empty()).map(|o| {
        let desc = match params.get("dir").map(|d| *d) {
            Some("asc") => false,
            Some("desc") => true,
            _ => false,
        };
        (o, desc)
    });

    let _ = match require_public_readable(ctx, board_id).await {
        Ok(app) => app,
        Err(resp) => return Ok(resp),
    };

    let records = if let Some(q) = params.get("q").map(|q| url_decode(q)).filter(|q| !q.is_empty()) {
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
        match ctx.db.srv_record_search(board_id, &q, &conds, limit).await {
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
        let filter = match parse_filter(&parsed) {
            Ok(sf) => sf,
            Err(e) => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": e.to_string()}))),
        };
        match ctx
            .db
            .srv_record_filter(board_id, &filter.conds, order.as_ref().map(|(p, d)| (p.as_str(), *d)), limit)
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("srv_records: filter error: {}", e);
                return Ok(json_response(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({"error": "query failed"})));
            }
        }
    } else {
        match ctx.db.srv_record_list(board_id, limit, None).await {
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

    let agg = match params.get("op").and_then(|o| Agg::parse(o).ok()) {
        Some(a) => a,
        None => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "missing or invalid op (count, sum, avg, min, max)"}))),
    };
    let field = params.get("field").map(|s| s.to_string());
    let group_by = params.get("group_by").map(|s| s.to_string());

    let _ = match require_public_readable(ctx, board_id).await {
        Ok(app) => app,
        Err(resp) => return Ok(resp),
    };

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

    let seq: i64 = match params.get("seq").and_then(|s| s.parse().ok()) {
        Some(s) => s,
        None => return Ok(json_response(StatusCode::BAD_REQUEST, serde_json::json!({"error": "missing or invalid seq"}))),
    };

    let _ = match require_public_readable(ctx, board_id).await {
        Ok(app) => app,
        Err(resp) => return Ok(resp),
    };

    match ctx.db.srv_record_get(board_id, seq).await {
        Ok(Some(record)) => Ok(json_response(
            StatusCode::OK,
            serde_json::json!({"ok": true, "record": {"seq": record.seq, "payload": record.payload, "created_at": record.created_at}}),
        )),
        Ok(None) => Ok(json_response(StatusCode::NOT_FOUND, serde_json::json!({"error": "not found"}))),
        Err(e) => {
            tracing::warn!("srv_record: get error: {}", e);
            Ok(json_response(StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({"error": "get failed"})))
        }
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

    match ctx.db.srv_record_insert(board_id, &serde_json::Value::Object(payload)).await {
        Ok(seq) => json_response(
            StatusCode::CREATED,
            serde_json::json!({"ok": true, "seq": seq, "file": format!("files/{}.bin", file_id)}),
        ),
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

    let app = match ctx.db.srv_app_by_id(board_id).await {
        Ok(Some(app)) => app,
        _ => return Ok(json_response(StatusCode::NOT_FOUND, serde_json::json!({"error": "not found"}))),
    };

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
        return Ok(persist_upload(ctx, board_id, &app, &filename, &file_type, &file.data, extra).await);
    }

    let filename = headers(cx)
        .get("X-Filename")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("upload")
        .to_string();

    Ok(persist_upload(ctx, board_id, &app, &filename, &content_type, &body_bytes, serde_json::Map::new()).await)
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
    let path = srv_board_file_path(root, &app.owner_key, board_id, &file_id);
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