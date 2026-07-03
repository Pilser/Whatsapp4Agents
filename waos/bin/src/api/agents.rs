use std::sync::Arc;

use hyper::{Body, Request, Response};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::server::ApiState;

#[derive(Deserialize)]
struct CreateAgentBody {
    alias: String,
    description: Option<String>,
}

#[derive(Serialize)]
struct AgentEntry {
    alias: String,
    description: String,
}

pub async fn list(state: &Arc<ApiState>) -> Response<Body> {
    let db = &state.daemon.db_mgr.lock().await.main_conn;
    let mut rows = match db
        .query("SELECT alias, description FROM agents ORDER BY alias", ())
        .await
    {
        Ok(r) => r,
        Err(e) => return json_error(500, &e.to_string()),
    };

    let mut agents = Vec::new();
    loop {
        let row = match rows.next().await {
            Ok(Some(r)) => r,
            Ok(None) => break,
            Err(_) => break,
        };
        agents.push(AgentEntry {
            alias: get_text(&row, 0),
            description: get_text(&row, 1),
        });
    }

    json_ok(&serde_json::json!({ "agents": agents }))
}

pub async fn create(state: &Arc<ApiState>, req: Request<Body>) -> Response<Body> {
    let body = match hyper::body::to_bytes(req.into_body()).await {
        Ok(b) => b,
        Err(e) => return json_error(400, &e.to_string()),
    };

    let create: CreateAgentBody = match serde_json::from_slice(&body) {
        Ok(c) => c,
        Err(e) => return json_error(400, &e.to_string()),
    };

    if create.alias.is_empty() {
        return json_error(400, "alias is required");
    }

    let db = &state.daemon.db_mgr.lock().await.main_conn;
    let now = chrono::Utc::now().timestamp();
    match db
        .execute(
            "INSERT INTO agents (alias, description, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)
             ON CONFLICT(alias) DO UPDATE SET description = ?2, updated_at = ?3",
            turso::params::Params::Positional(vec![
                turso::Value::from(create.alias.as_str()),
                turso::Value::from(create.description.as_deref().unwrap_or("")),
                turso::Value::from(now),
            ]),
        )
        .await
    {
        Ok(_) => {
            info!("Agent '{}' created/updated", create.alias);
            json_ok(&serde_json::json!({ "status": "created", "alias": create.alias }))
        }
        Err(e) => json_error(500, &e.to_string()),
    }
}

pub async fn delete(state: &Arc<ApiState>, alias: &str) -> Response<Body> {
    let db = &state.daemon.db_mgr.lock().await.main_conn;
    match db
        .execute(
            "DELETE FROM agents WHERE alias = ?1",
            turso::params::Params::Positional(vec![turso::Value::from(alias)]),
        )
        .await
    {
        Ok(_) => {
            info!("Agent '{}' deleted", alias);
            json_ok(&serde_json::json!({ "status": "deleted", "alias": alias }))
        }
        Err(e) => json_error(500, &e.to_string()),
    }
}

// ---- helpers (duplicated from accounts.rs to avoid circular deps) ----

fn json_ok(value: &serde_json::Value) -> Response<Body> {
    let body = serde_json::to_string(value).unwrap();
    Response::builder()
        .status(200)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap()
}

fn json_error(status: u16, msg: &str) -> Response<Body> {
    let body = serde_json::to_string(&serde_json::json!({"error": msg})).unwrap();
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap()
}

fn get_text(row: &turso::Row, idx: usize) -> String {
    match row.get_value(idx) {
        Ok(turso::Value::Text(s)) => s,
        _ => String::new(),
    }
}
