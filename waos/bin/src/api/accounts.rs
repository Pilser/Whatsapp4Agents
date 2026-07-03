use std::sync::Arc;

use hyper::{Body, Request, Response};
use serde::Deserialize;
use tracing::{info, warn};
use waos_core::events::WaosEvent;
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
        .query("SELECT alias, phone, status, error, updated_at, zeroclaw_agents, bridge_dm_enabled, bridge_group_enabled FROM accounts ORDER BY alias", ())
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
        let agents = get_text(&row, 5);
        let bridge_dm = get_int(&row, 6);
        let bridge_group = get_int(&row, 7);
        let mut entry = serde_json::json!({
            "alias": get_text(&row, 0),
            "phone": get_text(&row, 1),
            "status": get_text(&row, 2),
            "zeroclaw_agents": agents,
            "bridge_dm_enabled": bridge_dm == 1,
            "bridge_group_enabled": bridge_group == 1,
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
            "SELECT alias, phone, status, error, updated_at, zeroclaw_agents, bridge_dm_enabled, bridge_group_enabled FROM accounts WHERE alias = ?1",
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
    let agents = get_text(&row, 5);
    let bridge_dm = get_int(&row, 6);
    let bridge_group = get_int(&row, 7);
    let mut entry = serde_json::json!({
        "alias": get_text(&row, 0),
        "phone": get_text(&row, 1),
        "status": get_text(&row, 2),
        "zeroclaw_agents": agents,
        "bridge_dm_enabled": bridge_dm == 1,
        "bridge_group_enabled": bridge_group == 1,
    });
    if !error.is_empty() {
        entry["error"] = serde_json::json!(error);
    }
    json_ok(&entry)
}

pub async fn update_account_agents(state: &Arc<ApiState>, alias: &str, req: Request<Body>) -> Response<Body> {
    let body = match hyper::body::to_bytes(req.into_body()).await {
        Ok(b) => b,
        Err(e) => return json_error(400, &e.to_string()),
    };

    #[derive(serde::Deserialize)]
    struct AgentsBody {
        agents: String,
        #[serde(default)]
        bridge_dm_enabled: Option<bool>,
        #[serde(default)]
        bridge_group_enabled: Option<bool>,
    }

    let update: AgentsBody = match serde_json::from_slice(&body) {
        Ok(b) => b,
        Err(e) => return json_error(400, &e.to_string()),
    };

    let db = &state.daemon.db_mgr.lock().await.main_conn;
    let now = chrono::Utc::now().timestamp();
    let has_dm = update.bridge_dm_enabled.is_some();
    let has_group = update.bridge_group_enabled.is_some();
    let result = if has_dm || has_group {
        let dm_val = update.bridge_dm_enabled.map(|v| if v { 1i64 } else { 0i64 });
        let group_val = update.bridge_group_enabled.map(|v| if v { 1i64 } else { 0i64 });
        let mut sql = String::from("UPDATE accounts SET zeroclaw_agents = ?1");
        let mut params: Vec<turso::Value> = vec![turso::Value::from(update.agents)];
        if has_dm {
            sql.push_str(", bridge_dm_enabled = ?");
            params.push(turso::Value::from(dm_val.unwrap()));
        }
        if has_group {
            sql.push_str(", bridge_group_enabled = ?");
            params.push(turso::Value::from(group_val.unwrap()));
        }
        sql.push_str(", updated_at = ?");
        params.push(turso::Value::from(now));
        sql.push_str(" WHERE alias = ?");
        params.push(turso::Value::from(alias));
        db.execute(&sql, turso::params::Params::Positional(params)).await
    } else {
        db.execute(
            "UPDATE accounts SET zeroclaw_agents = ?1, updated_at = ?2 WHERE alias = ?3",
            turso::params::Params::Positional(vec![
                turso::Value::from(update.agents),
                turso::Value::from(now),
                turso::Value::from(alias),
            ]),
        ).await
    };

    match result {
        Ok(_) => json_ok(&serde_json::json!({ "status": "updated", "alias": alias })),
        Err(e) => json_error(500, &e.to_string()),
    }
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
    info!("[{alias}] Pair request for phone {phone}");

    // Try existing connected client first — directly call pair_with_code
    let clients = state.daemon.account_manager.lock().await.clients.clone();
    let existing = clients.lock().await.get(alias).cloned();
    if let Some(client) = existing {
        info!("[{alias}] Using existing connected client for pair");
        let options = PairCodeOptions::for_phone(&phone);
        match client.pair_with_code(options).await {
            Ok(ref code) => {
                info!("[{alias}] pair_with_code succeeded, code length: {}", code.len());
                state.daemon.event_bus.publish(WaosEvent::PairingCode {
                    account: alias.to_string(),
                    code: code.clone(),
                });
                return json_ok(&serde_json::json!({ "code": code }));
            }
            Err(e) => {
                tracing::error!("[{alias}] pair_with_code failed on existing client: {e:?}");
                return json_error(500, &format!("{:?}", e));
            }
        }
    }

    // No connected client — restart bot with with_pair_code on the builder.
    // The bot's internal task (bot.rs:434) waits for socket and calls
    // pair_with_code() during init, before QR mode takes over.
    info!("[{alias}] No connected client — restarting bot for pair with {phone}");

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

    // Start bot WITH phone — adds with_pair_code on the builder
    info!("[{alias}] Spawning new bot for pair");
    if let Err(e) = state.daemon.account_manager.lock().await
        .add_account_with_phone(alias, &state.daemon.sessions_dir, &state.daemon.event_bus, Some(phone.clone()))
        .await
    {
        return json_error(500, &format!("failed to start bot: {e}"));
    }

    // Poll pair_results until the bot's internal task completes pairing.
    // The internal task (bot.rs:434) calls wait_for_socket → pair_with_code
    // and our event handler stores the result.
    info!("[{alias}] Waiting for pair result...");
    let mut waited = 0u64;
    let max_wait = 90u64;
    loop {
        let mgr = state.daemon.account_manager.lock().await;
        let result = mgr.pair_results.lock().await.remove(alias);
        drop(mgr);
        match result {
            Some(Ok(code)) => {
                info!("[{alias}] Pair code generated via builder, length: {}", code.len());
                return json_ok(&serde_json::json!({ "code": code }));
            }
            Some(Err(err)) => {
                warn!("[{alias}] Pair failed via builder: {err}");
                return json_error(500, &err);
            }
            None => {
                waited += 1;
                if waited >= max_wait {
                    warn!("[{alias}] Pair result not ready after {waited}s — timeout");
                    return json_error(500, "timeout waiting for pair result from bot");
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }
        }
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

/// Strip all non-digit characters so the phone is raw digits (e.g. "256755304241").
/// whatsapp-rust builds the JID as `{phone}@s.whatsapp.net` — a `+` prefix makes it invalid.
fn normalize_phone(phone: &str) -> String {
    let digits: String = phone.chars().filter(|c| c.is_ascii_digit()).collect();
    digits.trim_start_matches('0').to_string()
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

fn get_int(row: &turso::Row, idx: usize) -> i64 {
    match row.get_value(idx) {
        Ok(turso::Value::Integer(n)) => n,
        _ => 0,
    }
}
