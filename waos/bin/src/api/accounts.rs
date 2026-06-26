use std::sync::Arc;

use hyper::{Body, Request, Response};
use serde::Deserialize;
use tracing::info;
use whatsapp_rust::pair_code::PairCodeOptions;

use crate::server::ApiState;

#[derive(Deserialize)]
struct CreateAccountBody {
    alias: String,
    phone: Option<String>,
}

#[derive(Deserialize)]
struct PairAccountBody {
    phone: String,
}

pub async fn list(state: &Arc<ApiState>) -> Response<Body> {
    let db = &state.daemon.db_mgr.lock().await.main_conn;
    let mut rows = match db
        .query("SELECT alias, phone, status, error, updated_at FROM accounts ORDER BY alias", ())
        .await
    {
        Ok(r) => r,
        Err(e) => return json_error(500, &e.to_string()),
    };

    let mut accounts = Vec::new();
    loop {
        let row = match rows.next().await {
            Ok(Some(r)) => r,
            Ok(None) => break,
            Err(_) => break,
        };
        let error = get_text(&row, 3);
        let mut entry = serde_json::json!({
            "alias": get_text(&row, 0),
            "phone": get_text(&row, 1),
            "status": get_text(&row, 2),
        });
        if !error.is_empty() {
            entry["error"] = serde_json::json!(error);
        }
        accounts.push(entry);
    }

    json_ok(&serde_json::json!({ "accounts": accounts }))
}

pub async fn create(state: &Arc<ApiState>, req: Request<Body>) -> Response<Body> {
    let body = match hyper::body::to_bytes(req.into_body()).await {
        Ok(b) => b,
        Err(e) => return json_error(400, &e.to_string()),
    };

    let create: CreateAccountBody = match serde_json::from_slice(&body) {
        Ok(c) => c,
        Err(e) => return json_error(400, &e.to_string()),
    };

    if create.alias.is_empty() {
        return json_error(400, "alias is required");
    }

    match state
        .daemon
        .init_account(&create.alias, create.phone.as_deref())
        .await
    {
        Ok(()) => json_ok(&serde_json::json!({
            "status": "created",
            "alias": create.alias,
        })),
        Err(e) => json_error(500, &e.to_string()),
    }
}

pub async fn get(state: &Arc<ApiState>, alias: &str) -> Response<Body> {
    let db = &state.daemon.db_mgr.lock().await.main_conn;
    let mut rows = match db
        .query(
            "SELECT alias, phone, status, error, updated_at FROM accounts WHERE alias = ?1",
            turso::params::Params::Positional(vec![turso::Value::from(alias)]),
        )
        .await
    {
        Ok(r) => r,
        Err(e) => return json_error(500, &e.to_string()),
    };

    let row = match rows.next().await {
        Ok(Some(r)) => r,
        _ => return json_error(404, "account not found"),
    };

    let error = get_text(&row, 3);
    let mut entry = serde_json::json!({
        "alias": get_text(&row, 0),
        "phone": get_text(&row, 1),
        "status": get_text(&row, 2),
    });
    if !error.is_empty() {
        entry["error"] = serde_json::json!(error);
    }
    json_ok(&entry)
}

pub async fn delete(state: &Arc<ApiState>, alias: &str) -> Response<Body> {
    match state.daemon.destroy_account(alias).await {
        Ok(()) => json_ok(&serde_json::json!({ "status": "deleted", "alias": alias })),
        Err(e) => json_error(500, &e.to_string()),
    }
}

pub async fn qr(state: &Arc<ApiState>, alias: &str) -> Response<Body> {
    let qr_codes = state.daemon.account_manager.lock().await.qr_codes.clone();
    let code = qr_codes.lock().await.get(alias).cloned();
    match code {
        Some(qr) => json_ok(&serde_json::json!({ "alias": alias, "qr": qr })),
        None => json_ok(&serde_json::json!({ "alias": alias, "qr": null })),
    }
}

pub async fn pair(state: &Arc<ApiState>, alias: &str, req: Request<Body>) -> Response<Body> {
    let body = match hyper::body::to_bytes(req.into_body()).await {
        Ok(b) => b,
        Err(e) => return json_error(400, &e.to_string()),
    };

    let pair_body: PairAccountBody = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(e) => return json_error(400, &e.to_string()),
    };

    if pair_body.phone.is_empty() {
        return json_error(400, "phone is required");
    }

    let phone = normalize_phone(&pair_body.phone);

    // Try to use existing connected client first
    let clients = state.daemon.account_manager.lock().await.clients.clone();
    let existing = clients.lock().await.get(alias).cloned();
    if let Some(client) = existing {
        let options = PairCodeOptions::for_phone(&phone);
        match client.pair_with_code(options).await {
            Ok(code) => return json_ok(&serde_json::json!({ "code": code })),
            Err(e) => return json_error(500, &format!("{:?}", e)),
        }
    }

    // No connected client — restart bot WITHOUT internal pair_code flow and
    // call pair_with_code() directly once the bot is running.
    info!("Restarting bot '{}' for direct phone pairing with {}", alias, phone);

    // Remove old bot and wipe session
    state.daemon.account_manager.lock().await.remove_account(alias).await;
    let session_dir = state.daemon.sessions_dir.join(alias);
    if session_dir.exists() {
        let mut entries = match tokio::fs::read_dir(&session_dir).await {
            Ok(e) => e,
            Err(_) => return json_error(500, "failed to read session dir"),
        };
        while let Some(entry) = entries.next_entry().await.unwrap_or(None) {
            let path = entry.path();
            if path.is_dir() {
                let _ = tokio::fs::remove_dir_all(&path).await;
            } else {
                let _ = tokio::fs::remove_file(&path).await;
            }
        }
        let _ = tokio::fs::remove_dir(&session_dir).await;
    }
    // Clear any stale pending client
    state.daemon.account_manager.lock().await.pending_clients.lock().await.remove(alias);

    // Start bot WITHOUT phone — no with_pair_code on builder
    if let Err(e) = state.daemon.account_manager.lock().await
        .add_account(alias, &state.daemon.sessions_dir, &state.daemon.event_bus)
        .await
    {
        return json_error(500, &format!("failed to start bot: {e}"));
    }

    // Poll pending_clients for up to 30s until the client is available
    let client = {
        let mut waited = 0u64;
        let max_wait = 30u64;
        let client = loop {
            let mgr = state.daemon.account_manager.lock().await;
            let pc = mgr.pending_clients.lock().await;
            if let Some(c) = pc.get(alias).cloned() {
                break c;
            }
            drop(pc);
            drop(mgr);
            waited += 1;
            if waited >= max_wait {
                return json_error(500, "timeout waiting for bot client to initialize");
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        };
        client
    };

    // Wait for the WebSocket to be ready
    if let Err(e) = client.wait_for_socket(std::time::Duration::from_secs(30)).await {
        return json_error(500, &format!("socket not ready: {e}"));
    }

    // Call pair_with_code() directly — errors propagate to the frontend
    let options = PairCodeOptions::for_phone(&phone);
    match client.pair_with_code(options).await {
        Ok(code) => json_ok(&serde_json::json!({ "code": code })),
        Err(e) => json_error(500, &format!("{:?}", e)),
    }
}

pub async fn regenerate_qr(state: &Arc<ApiState>, alias: &str) -> Response<Body> {
    // Clear stored QR code
    {
        let qr_codes = state.daemon.account_manager.lock().await.qr_codes.clone();
        qr_codes.lock().await.remove(alias);
    }

    // Remove bot
    state.daemon.account_manager.lock().await.remove_account(alias).await;

    // Wipe session credentials
    let session_dir = state.daemon.sessions_dir.join(alias);
    if session_dir.exists() {
        let mut entries = match tokio::fs::read_dir(&session_dir).await {
            Ok(e) => e,
            Err(_) => return json_error(500, "failed to read session dir"),
        };
        while let Some(entry) = entries.next_entry().await.unwrap_or(None) {
            let path = entry.path();
            if path.is_dir() {
                let _ = tokio::fs::remove_dir_all(&path).await;
            } else {
                let _ = tokio::fs::remove_file(&path).await;
            }
        }
        let _ = tokio::fs::remove_dir(&session_dir).await;
    }

    // Restart bot without phone — triggers fresh QR handshake
    match state.daemon.account_manager.lock().await
        .add_account(alias, &state.daemon.sessions_dir, &state.daemon.event_bus)
        .await
    {
        Ok(()) => json_ok(&serde_json::json!({
            "status": "restarting",
            "message": "Bot restarting for QR pairing. QR code will appear below when ready."
        })),
        Err(e) => json_error(500, &e.to_string()),
    }
}

/// Strip leading zeros from a phone number and ensure it starts with '+'.
/// wa-rs rejects numbers starting with '0'.
fn normalize_phone(phone: &str) -> String {
    let trimmed = phone.trim();
    if trimmed.starts_with('+') {
        return trimmed.to_string();
    }
    let stripped = trimmed.trim_start_matches('0');
    format!("+{}", stripped)
}

// ---- helpers ----

fn json_ok(value: &serde_json::Value) -> Response<Body> {
    let body = serde_json::to_string(value).unwrap();
    Response::builder()
        .status(200)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap()
}

pub fn json_error(status: u16, msg: &str) -> Response<Body> {
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
