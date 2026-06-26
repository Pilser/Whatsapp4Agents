use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::time;
use tracing::{error, info, warn};

use crate::daemon::DaemonState;

#[derive(Serialize, Deserialize)]
pub struct IpcRequest {
    pub cmd: String,
    #[serde(default)]
    pub account: Option<String>,
    #[serde(default)]
    pub chat_id: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub reply_to_id: Option<String>,
    #[serde(default)]
    pub message_id: Option<String>,
    #[serde(default)]
    pub emoji: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub file_path: Option<String>,
    #[serde(default)]
    pub jid: Option<String>,
    #[serde(default)]
    pub jid1: Option<String>,
    #[serde(default)]
    pub jid2: Option<String>,
    #[serde(default)]
    pub community_id: Option<String>,
    #[serde(default)]
    pub score_value: Option<f64>,
    #[serde(default)]
    pub before: Option<i64>,
    #[serde(default)]
    pub after: Option<i64>,
    #[serde(default)]
    pub total: Option<bool>,
    #[serde(default)]
    pub as_document: Option<bool>,
    #[serde(default)]
    pub timeout: Option<u64>,
    #[serde(default)]
    pub wait: Option<bool>,
    #[serde(default)]
    pub metadata: Option<String>,
}

impl IpcRequest {
    pub fn new(cmd: &str) -> Self {
        Self {
            cmd: cmd.to_string(),
            account: None,
            chat_id: None,
            text: None,
            query: None,
            reply_to_id: None,
            message_id: None,
            emoji: None,
            phone: None,
            status: None,
            file_path: None,
            jid: None,
            jid1: None,
            jid2: None,
            community_id: None,
            score_value: None,
            before: None,
            after: None,
            total: None,
            as_document: None,
            timeout: None,
            wait: None,
            metadata: None,
        }
    }
    pub fn with_account(mut self, a: &str) -> Self { self.account = Some(a.to_string()); self }
    pub fn with_chat_id(mut self, c: &str) -> Self { self.chat_id = Some(c.to_string()); self }
    pub fn with_text(mut self, t: &str) -> Self { self.text = Some(t.to_string()); self }
    pub fn with_query(mut self, q: &str) -> Self { self.query = Some(q.to_string()); self }
    pub fn with_reply_to(mut self, r: &str) -> Self { self.reply_to_id = Some(r.to_string()); self }
    pub fn with_message_id(mut self, m: &str) -> Self { self.message_id = Some(m.to_string()); self }
    pub fn with_emoji(mut self, e: &str) -> Self { self.emoji = Some(e.to_string()); self }
    pub fn with_phone(mut self, p: &str) -> Self { self.phone = Some(p.to_string()); self }
    pub fn with_status(mut self, s: &str) -> Self { self.status = Some(s.to_string()); self }
    pub fn with_file_path(mut self, p: &str) -> Self { self.file_path = Some(p.to_string()); self }
    pub fn with_jid(mut self, j: &str) -> Self { self.jid = Some(j.to_string()); self }
    pub fn with_jid1(mut self, j: &str) -> Self { self.jid1 = Some(j.to_string()); self }
    pub fn with_jid2(mut self, j: &str) -> Self { self.jid2 = Some(j.to_string()); self }
    pub fn with_community_id(mut self, c: &str) -> Self { self.community_id = Some(c.to_string()); self }
    pub fn with_score_value(mut self, v: f64) -> Self { self.score_value = Some(v); self }
    pub fn with_before(mut self, ts: i64) -> Self { self.before = Some(ts); self }
    pub fn with_after(mut self, ts: i64) -> Self { self.after = Some(ts); self }
    pub fn with_total(mut self, v: bool) -> Self { self.total = Some(v); self }
    pub fn with_as_document(mut self, v: bool) -> Self { self.as_document = Some(v); self }
    pub fn with_timeout(mut self, v: u64) -> Self { self.timeout = Some(v); self }
    pub fn with_wait(mut self, v: bool) -> Self { self.wait = Some(v); self }
    pub fn with_metadata(mut self, v: &str) -> Self { self.metadata = Some(v.to_string()); self }
}

#[derive(Serialize, Deserialize)]
pub struct IpcResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl IpcResponse {
    fn ok(data: serde_json::Value) -> Self {
        Self { ok: true, data: Some(data), error: None }
    }
    fn err(msg: impl Into<String>) -> Self {
        Self { ok: false, data: None, error: Some(msg.into()) }
    }
}

pub fn socket_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("waos.sock")
}

pub async fn run_server(state: Arc<DaemonState>) {
    let sock_path = socket_path(&state.config.data_dir());
    let _ = tokio::fs::remove_file(&sock_path).await;
    let listener = match UnixListener::bind(&sock_path) {
        Ok(l) => l,
        Err(e) => {
            error!("IPC socket bind failed at {}: {e}", sock_path.display());
            return;
        }
    };
    info!("IPC socket listening at {}", sock_path.display());

    loop {
        let (stream, _) = match listener.accept().await {
            Ok(s) => s,
            Err(e) => {
                warn!("IPC accept error: {e}");
                continue;
            }
        };
        let st = state.clone();
        tokio::spawn(async move { handle_connection(stream, st).await });
    }
}

async fn handle_connection(mut stream: UnixStream, state: Arc<DaemonState>) {
    let (reader, mut writer) = stream.split();
    let mut lines = BufReader::new(reader).lines();

    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        let req: IpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = IpcResponse::err(format!("bad request: {e}"));
                let _ = writer.write_all(serde_json::to_string(&resp).unwrap_or_default().as_bytes()).await;
                let _ = writer.write_all(b"\n").await;
                let _ = writer.flush().await;
                continue;
            }
        };

        // Streaming command — handler takes over the writer
        if req.cmd == "chat_watch" {
            let chat_id = match req.chat_id.clone() {
                Some(id) => id,
                None => {
                    let resp = IpcResponse::err("missing chat_id");
                    let _ = writer.write_all(serde_json::to_string(&resp).unwrap_or_default().as_bytes()).await;
                    let _ = writer.write_all(b"\n").await;
                    let _ = writer.flush().await;
                    break;
                }
            };
            let alias = req.account.clone().unwrap_or("default".to_string());
            let timeout_secs = req.timeout.unwrap_or(120);
            cmd_chat_watch(&mut writer, &state, &alias, &chat_id, timeout_secs).await;
            break;
        }

        let resp = dispatch(&req, &state).await;
        let _ = writer.write_all(serde_json::to_string(&resp).unwrap_or_default().as_bytes()).await;
        let _ = writer.write_all(b"\n").await;
        let _ = writer.flush().await;

        if req.cmd != "dashboard_watch" {
            break;
        }
    }
}

async fn dispatch(req: &IpcRequest, state: &DaemonState) -> IpcResponse {
    let alias = req.account.as_deref().unwrap_or("default");
    match req.cmd.as_str() {
        "status" => cmd_status(state).await,
        "dashboard" => cmd_dashboard(state).await,
        "chat_list" | "chat_unread" => {
            cmd_chat_list(state, alias, req.cmd.as_str() == "chat_unread").await
        }
        "chat_search" => {
            cmd_chat_search(state, alias, req.query.as_deref().unwrap_or("")).await
        }
        "chat_history" => {
            let limit: usize = req.query.as_deref().and_then(|s| s.parse().ok()).unwrap_or(5);
            let offset: usize = req.text.as_deref().and_then(|s| s.parse().ok()).unwrap_or(0);
            let total = req.total.unwrap_or(false);
            cmd_chat_history(state, alias, req.chat_id.as_deref().unwrap_or(""), limit, offset, req.before, req.after, total).await
        }
        "chat_mark_read" => {
            cmd_chat_mark_read(state, alias, req.chat_id.as_deref().unwrap_or("")).await
        }
        "message_send" => {
            let w = req.wait.unwrap_or(false);
            let t = req.timeout.unwrap_or(30);
            cmd_message_send(state, alias, req.chat_id.as_deref().unwrap_or(""), req.text.as_deref().unwrap_or(""), w, t).await
        }
        "message_reply" => {
            let w = req.wait.unwrap_or(false);
            let t = req.timeout.unwrap_or(30);
            cmd_message_reply(state, alias, req.chat_id.as_deref().unwrap_or(""), req.reply_to_id.as_deref().unwrap_or(""), req.text.as_deref().unwrap_or(""), w, t).await
        }
        "message_react" => {
            let w = req.wait.unwrap_or(false);
            let t = req.timeout.unwrap_or(30);
            cmd_message_react(state, alias, req.chat_id.as_deref().unwrap_or(""), req.message_id.as_deref().unwrap_or(""), req.emoji.as_deref().unwrap_or(""), w, t).await
        }
        "message_edit" => {
            let w = req.wait.unwrap_or(false);
            let t = req.timeout.unwrap_or(30);
            cmd_message_edit(state, alias, req.chat_id.as_deref().unwrap_or(""), req.message_id.as_deref().unwrap_or(""), req.text.as_deref().unwrap_or(""), w, t).await
        }
        "message_delete" => {
            let w = req.wait.unwrap_or(false);
            let t = req.timeout.unwrap_or(30);
            cmd_message_delete(state, alias, req.chat_id.as_deref().unwrap_or(""), req.message_id.as_deref().unwrap_or(""), w, t).await
        }
        "media_send" => {
            let as_doc = req.as_document.unwrap_or(false);
            let w = req.wait.unwrap_or(false);
            let t = req.timeout.unwrap_or(30);
            cmd_media_send(state, alias, req.chat_id.as_deref().unwrap_or(""), req.file_path.as_deref().unwrap_or(""), req.text.as_deref(), req.reply_to_id.as_deref(), as_doc, w, t).await
        }
        "media_download" => {
            cmd_media_download(state, alias, req.message_id.as_deref().unwrap_or("")).await
        }
        "contact_search" => {
            cmd_contact_search(state, alias, req.query.as_deref().unwrap_or("")).await
        }
        "contact_info" => {
            cmd_contact_info(state, alias, req.jid.as_deref().unwrap_or("")).await
        }
        "contact_score" => {
            cmd_contact_score(state, alias, req.jid.as_deref().unwrap_or(""), req.score_value.unwrap_or(0.0)).await
        }
        "memory_get" => {
            cmd_memory_get(state, alias, req.chat_id.as_deref().unwrap_or("")).await
        }
        "memory_set" => {
            cmd_memory_set(state, alias, req.chat_id.as_deref().unwrap_or(""), req.text.as_deref().unwrap_or("")).await
        }
        "memory_search" => {
            cmd_memory_search(state, alias, req.query.as_deref().unwrap_or("")).await
        }
        "graph_shared_groups" => {
            cmd_graph_shared_groups(state, alias, req.jid1.as_deref().unwrap_or(""), req.jid2.as_deref().unwrap_or("")).await
        }
        "graph_introductions" => {
            cmd_graph_introductions(state, alias, req.jid.as_deref().unwrap_or("")).await
        }
        "graph_important_contacts" => {
            cmd_graph_important_contacts(state, alias, req.community_id.as_deref().unwrap_or("")).await
        }
        "community_list" => cmd_community_list(state, alias).await,
        "community_info" => {
            cmd_community_info(state, alias, req.community_id.as_deref().unwrap_or("")).await
        }
        "status_send_text" => {
            let w = req.wait.unwrap_or(false);
            let t = req.timeout.unwrap_or(30);
            cmd_status_send_text(state, alias, req.text.as_deref().unwrap_or(""), req.metadata.as_deref().unwrap_or(""), w, t).await
        }
        "status_send_image" => {
            let w = req.wait.unwrap_or(false);
            let t = req.timeout.unwrap_or(30);
            cmd_status_send_media(state, alias, req.file_path.as_deref().unwrap_or(""), req.text.as_deref(), req.metadata.as_deref(), "status_image", w, t).await
        }
        "status_send_video" => {
            let w = req.wait.unwrap_or(false);
            let t = req.timeout.unwrap_or(30);
            cmd_status_send_media(state, alias, req.file_path.as_deref().unwrap_or(""), req.text.as_deref(), req.metadata.as_deref(), "status_video", w, t).await
        }
        "status_revoke" => {
            let w = req.wait.unwrap_or(false);
            let t = req.timeout.unwrap_or(30);
            cmd_status_revoke(state, alias, req.message_id.as_deref().unwrap_or(""), w, t).await
        }
        "status_history" => {
            let limit: usize = req.text.as_deref().and_then(|s| s.parse().ok()).unwrap_or(5);
            let offset: usize = req.reply_to_id.as_deref().and_then(|s| s.parse().ok()).unwrap_or(0);
            cmd_status_history(state, alias, req.query.as_deref().unwrap_or("all"), req.before, req.after, limit, offset).await
        }
        "status_delete" => {
            let confirmed = req.text.as_deref() == Some("yes");
            cmd_status_delete(state, alias, req.before, req.after, req.query.as_deref().unwrap_or("all"), confirmed).await
        }
        "pair" => cmd_pair(state, alias).await,
        "task_list" => cmd_task_list(state, req.status.as_deref()).await,
        "task_complete" => cmd_task_complete(state, req.query.as_deref()).await,
        _ => IpcResponse::err(format!("unknown command: {}", req.cmd)),
    }
}

// ── Chat Watch (Streaming) ────────────────────────────────────────────────

async fn cmd_chat_watch(
    writer: &mut (impl AsyncWriteExt + Unpin),
    state: &DaemonState,
    alias: &str,
    chat_id: &str,
    timeout_secs: u64,
) {
    use waos_core::events::WaosEvent;

    // Send initial state: pending queue items for this chat
    if let Some(conn) = state.db_conns.lock().await.get(alias).cloned() {
        let mut rows = conn.query(
            "SELECT id, status, content, created_at, media_type, error FROM outbound_queue WHERE chat_id = ?1 AND status IN ('pending', 'sending') ORDER BY created_at ASC",
            turso::params::Params::Positional(vec![turso::Value::from(chat_id)]),
        ).await;
        if let Ok(ref mut rows) = rows {
            loop {
                match rows.next().await {
                    Ok(Some(row)) => {
                        let ev = serde_json::json!({
                            "type": "queue_snapshot",
                            "account": alias,
                            "chat_id": chat_id,
                            "data": {
                                "queue_id": text_val(row.get_value(0)),
                                "status": text_val(row.get_value(1)),
                                "content": text_val(row.get_value(2)),
                                "created_at": int_val(row.get_value(3)),
                                "media_type": text_val(row.get_value(4)),
                            }
                        });
                        let _ = writer.write_all(serde_json::to_string(&ev).unwrap_or_default().as_bytes()).await;
                        let _ = writer.write_all(b"\n").await;
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            }
        }
        let _ = writer.flush().await;
    }

    // Subscribe to events and stream matching ones
    let mut rx = state.event_bus.subscribe();
    let deadline = time::Instant::now() + Duration::from_secs(timeout_secs);

    loop {
        let remaining = deadline.checked_duration_since(time::Instant::now());
        let timeout = match remaining {
            Some(d) => d,
            None => break,
        };

        match time::timeout(timeout, rx.recv()).await {
            Ok(Ok(event)) => {
                let matched = match &event {
                    WaosEvent::QueueItemStatusChanged { chat_id: c, .. }
                    | WaosEvent::NewMessage { chat_id: c, .. }
                    | WaosEvent::MessageReceipt { chat_id: c, .. }
                    | WaosEvent::MessageEdited { chat_id: c, .. }
                    | WaosEvent::MessageDeleted { chat_id: c, .. }
                    | WaosEvent::ReactionAdded { chat_id: c, .. }
                    | WaosEvent::ChatPresenceUpdate { chat_id: c, .. } => c == chat_id,
                    _ => false,
                };
                if !matched {
                    continue;
                }
                let mut json = serde_json::to_value(&event).unwrap_or_default();
                if let Some(obj) = json.as_object_mut() {
                    obj.insert("type".into(), serde_json::json!("event"));
                }
                let _ = writer.write_all(serde_json::to_string(&json).unwrap_or_default().as_bytes()).await;
                let _ = writer.write_all(b"\n").await;
                let _ = writer.flush().await;
            }
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(n))) => {
                warn!("Chat watch lagged by {n} events");
            }
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => break,
            Err(_) => break, // timeout
        }
    }
}

// ── Wait for Queue Item (--wait flag) ─────────────────────────────────────

async fn wait_for_queue_item(state: &DaemonState, queue_id: &str, timeout_secs: u64) -> IpcResponse {
    use waos_core::events::WaosEvent;
    let mut rx = state.event_bus.subscribe();
    let deadline = time::Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        let remaining = deadline.checked_duration_since(time::Instant::now());
        let d = match remaining {
            Some(d) => d,
            None => return IpcResponse::err("timeout"),
        };
        match time::timeout(d, rx.recv()).await {
            Ok(Ok(WaosEvent::QueueItemStatusChanged {
                queue_id: ref q, status, ref message_id, ref error, ..
            })) if q == queue_id => {
                match status.as_str() {
                    "sent" => {
                        return IpcResponse::ok(serde_json::json!({
                            "queue_id": queue_id,
                            "status": "sent",
                            "message_id": message_id,
                        }));
                    }
                    "failed" => {
                        return IpcResponse::err(error.clone().unwrap_or("send failed".into()));
                    }
                    _ => continue,
                }
            }
            Ok(Ok(_)) => continue,
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(n))) => {
                warn!("Queue wait lagged by {n} events");
                continue;
            }
            Ok(Err(tokio::sync::broadcast::error::RecvError::Closed)) => {
                return IpcResponse::err("event bus closed");
            }
            Err(_) => return IpcResponse::err("timeout"),
        }
    }
}

fn text_val(v: Result<turso::Value, turso::Error>) -> String {
    match v {
        Ok(turso::Value::Text(s)) => s,
        Ok(turso::Value::Integer(n)) => n.to_string(),
        Ok(turso::Value::Real(f)) => f.to_string(),
        _ => String::new(),
    }
}

fn int_val(v: Result<turso::Value, turso::Error>) -> i64 {
    match v {
        Ok(turso::Value::Integer(n)) => n,
        Ok(turso::Value::Real(f)) => f as i64,
        _ => 0,
    }
}

async fn cmd_status(state: &DaemonState) -> IpcResponse {
    let conn = &state.db_mgr.lock().await.main_conn;
    let mut rows = match conn.query("SELECT alias, phone, status, error, updated_at FROM accounts ORDER BY alias", ()).await {
        Ok(r) => r,
        Err(e) => return IpcResponse::err(format!("db query: {e}")),
    };
    let mut accounts = Vec::new();
    while let Ok(Some(row)) = rows.next().await {
        let error = text_val(row.get_value(3));
        let mut entry = serde_json::json!({
            "alias": text_val(row.get_value(0)),
            "phone": text_val(row.get_value(1)),
            "status": text_val(row.get_value(2)),
            "updated_at": int_val(row.get_value(4)),
        });
        if !error.is_empty() {
            entry["error"] = serde_json::json!(error);
        }
        accounts.push(entry);
    }
    IpcResponse::ok(serde_json::json!({ "accounts": accounts }))
}

async fn cmd_pair(state: &DaemonState, alias: &str) -> IpcResponse {
    IpcResponse::ok(serde_json::json!({
        "account": alias,
        "message": "Use the web UI at /accounts/{alias} or POST /api/accounts/{alias}/pair to start pairing",
        "instructions": [
            "1. Go to the web dashboard and open the account manage overlay",
            "2. Click 'Pair' with your phone number",
            "3. Enter the pairing code on your phone > Linked Devices"
        ]
    }))
}

async fn cmd_dashboard(state: &DaemonState) -> IpcResponse {
    let path = &state.dash_path;
    match tokio::fs::read_to_string(path).await {
        Ok(content) => {
            match serde_json::from_str::<serde_json::Value>(&content) {
                Ok(v) => IpcResponse::ok(v),
                Err(e) => IpcResponse::err(format!("dashboard parse: {e}")),
            }
        }
        Err(e) => IpcResponse::err(format!("dashboard read: {e}")),
    }
}

async fn open_account_conn(state: &DaemonState, alias: &str) -> Result<turso::Connection, String> {
    // First try in-memory connections (daemon already has it open)
    let conns = state.db_conns.lock().await;
    if let Some(conn) = conns.get(alias) {
        return Ok(conn.clone());
    }
    drop(conns);

    // Fall back to opening the DB file directly
    let db_path = state.config.account_db(alias);
    if !db_path.exists() {
        return Err(format!("No database found for account '{alias}'"));
    }
    let db = turso::Builder::new_local(db_path.to_str().unwrap()).build().await.map_err(|e| e.to_string())?;
    db.connect().map_err(|e| e.to_string())
}

async fn cmd_chat_list(state: &DaemonState, alias: &str, unread_only: bool) -> IpcResponse {
    let conn = match open_account_conn(state, alias).await {
        Ok(c) => c,
        Err(e) => return IpcResponse::err(e),
    };
    let sql = if unread_only {
        "SELECT id, name, type, unread_count, last_message_snippet, last_message_at
         FROM chats WHERE unread_count > 0 ORDER BY last_message_at DESC"
    } else {
        "SELECT id, name, type, unread_count, last_message_snippet, last_message_at
         FROM chats ORDER BY last_message_at DESC"
    };
    let mut rows = match conn.query(sql, ()).await {
        Ok(r) => r,
        Err(e) => return IpcResponse::err(format!("query: {e}")),
    };
    let mut chats = Vec::new();
    while let Ok(Some(row)) = rows.next().await {
        chats.push(serde_json::json!({
            "id": text_val(row.get_value(0)),
            "name": text_val(row.get_value(1)),
            "type": text_val(row.get_value(2)),
            "unread": int_val(row.get_value(3)),
            "last_message": text_val(row.get_value(4)),
            "last_activity": int_val(row.get_value(5)),
        }));
    }
    IpcResponse::ok(serde_json::json!({ "chats": chats }))
}

async fn cmd_chat_search(state: &DaemonState, alias: &str, query: &str) -> IpcResponse {
    let conn = match open_account_conn(state, alias).await {
        Ok(c) => c,
        Err(e) => return IpcResponse::err(e),
    };
    let pattern = format!("%{}%", query);
    let mut rows = match conn.query(
        "SELECT m.id, m.chat_id, c.name, m.sender_name, m.content, m.timestamp
         FROM messages m JOIN chats c ON c.id = m.chat_id
         WHERE m.content LIKE ?1 ORDER BY m.timestamp DESC LIMIT 50",
        turso::params::Params::Positional(vec![turso::Value::from(pattern)]),
    ).await {
        Ok(r) => r,
        Err(e) => return IpcResponse::err(format!("query: {e}")),
    };
    let mut results = Vec::new();
    while let Ok(Some(row)) = rows.next().await {
        results.push(serde_json::json!({
            "id": text_val(row.get_value(0)),
            "chat": text_val(row.get_value(2)),
            "sender": text_val(row.get_value(3)),
            "content": text_val(row.get_value(4)),
        }));
    }
    IpcResponse::ok(serde_json::json!({ "results": results }))
}

async fn cmd_message_send(state: &DaemonState, alias: &str, chat_id: &str, text: &str, wait: bool, timeout: u64) -> IpcResponse {
    let conn = match open_account_conn(state, alias).await {
        Ok(c) => c,
        Err(e) => return IpcResponse::err(e),
    };
    let id = uuid::Uuid::new_v4().to_string();
    if let Err(e) = conn.execute(
        "INSERT INTO outbound_queue (id, chat_id, content, status, created_at)
         VALUES (?1, ?2, ?3, 'pending', ?4)",
        turso::params::Params::Positional(vec![
            turso::Value::from(id.as_str()),
            turso::Value::from(chat_id),
            turso::Value::from(text),
            turso::Value::from(chrono::Utc::now().timestamp()),
        ]),
    ).await {
        return IpcResponse::err(format!("db insert: {e}"));
    }
    if wait {
        wait_for_queue_item(state, &id, timeout).await
    } else {
        IpcResponse::ok(serde_json::json!({ "account": alias, "queue_id": id, "action": "send" }))
    }
}

fn normalize_jid(jid: &str) -> String {
    if let Some(at_pos) = jid.find('@') {
        if let Some(colon_pos) = jid[..at_pos].rfind(':') {
            return format!("{}{}", &jid[..colon_pos], &jid[at_pos..]);
        }
    }
    jid.to_string()
}

async fn agent_jid(data_dir: &Path, alias: &str) -> Option<String> {
    let session_db = data_dir.join("sessions").join(alias).join("session.db");
    if !session_db.exists() {
        return None;
    }
    let db = turso::Builder::new_local(session_db.to_str()?).build().await.ok()?;
    let conn = db.connect().ok()?;
    let mut rows = conn.query("SELECT pn FROM device LIMIT 1", ()).await.ok()?;
    let row = rows.next().await.ok()??;
    let raw = text_val(row.get_value(0));
    if raw.is_empty() { None } else { Some(normalize_jid(&raw)) }
}

async fn cmd_chat_history(state: &DaemonState, alias: &str, chat_id: &str, limit: usize, offset: usize, before: Option<i64>, after: Option<i64>, show_total: bool) -> IpcResponse {
    let conn = match open_account_conn(state, alias).await {
        Ok(c) => c,
        Err(e) => return IpcResponse::err(e),
    };
    let my_jid = agent_jid(&state.config.data_dir(), alias).await;

    let total_count = if show_total {
        let mut count_rows = match conn.query(
            "SELECT COUNT(*) FROM messages WHERE chat_id = ?1",
            turso::params::Params::Positional(vec![turso::Value::from(chat_id)]),
        ).await {
            Ok(r) => r,
            Err(e) => return IpcResponse::err(format!("count query: {e}")),
        };
        match count_rows.next().await.ok().flatten().and_then(|r| r.get_value(0).ok()) {
            Some(turso::Value::Integer(n)) => n,
            _ => 0,
        }
    } else {
        0
    };

    let (sql, params): (&str, Vec<turso::Value>) = match (before, after) {
        (Some(ts), _) => (
            "SELECT id, sender_jid, sender_name, content, timestamp, message_type, has_media, status, media_path
             FROM messages WHERE chat_id = ?1 AND timestamp < ?2
             ORDER BY timestamp DESC LIMIT ?3 OFFSET ?4",
            vec![turso::Value::from(chat_id), turso::Value::from(ts), turso::Value::from(limit as i64), turso::Value::from(offset as i64)],
        ),
        (_, Some(ts)) => (
            "SELECT id, sender_jid, sender_name, content, timestamp, message_type, has_media, status, media_path
             FROM messages WHERE chat_id = ?1 AND timestamp > ?2
             ORDER BY timestamp ASC LIMIT ?3 OFFSET ?4",
            vec![turso::Value::from(chat_id), turso::Value::from(ts), turso::Value::from(limit as i64), turso::Value::from(offset as i64)],
        ),
        _ => (
            "SELECT id, sender_jid, sender_name, content, timestamp, message_type, has_media, status, media_path
             FROM messages WHERE chat_id = ?1
             ORDER BY timestamp DESC LIMIT ?2 OFFSET ?3",
            vec![turso::Value::from(chat_id), turso::Value::from(limit as i64), turso::Value::from(offset as i64)],
        ),
    };
    let mut rows = match conn.query(sql, turso::params::Params::Positional(params)).await {
        Ok(r) => r,
        Err(e) => return IpcResponse::err(format!("query: {e}")),
    };
    let mut msgs = Vec::new();
    while let Ok(Some(row)) = rows.next().await {
        let jid = text_val(row.get_value(1));
        let is_me = my_jid.as_ref().is_some_and(|mine| normalize_jid(&jid) == *mine);
        msgs.push(serde_json::json!({
            "id": text_val(row.get_value(0)),
            "sender": text_val(row.get_value(2)),
            "content": text_val(row.get_value(3)),
            "timestamp": int_val(row.get_value(4)),
            "type": text_val(row.get_value(5)),
            "has_media": int_val(row.get_value(6)) != 0,
            "is_me": is_me,
            "status": text_val(row.get_value(7)),
            "media_path": text_val(row.get_value(8)),
        }));
    }
    // Default (no before/after) returns DESC — reverse to chronological order
    if before.is_none() && after.is_none() {
        msgs.reverse();
    }
    // before query was DESC — reverse to chronological
    if before.is_some() {
        msgs.reverse();
    }
    let mut resp = serde_json::json!({ "messages": msgs, "chat_id": chat_id, "limit": limit });
    if show_total {
        resp["total"] = serde_json::json!(total_count);
    }
    IpcResponse::ok(resp)
}

async fn cmd_chat_mark_read(state: &DaemonState, alias: &str, chat_id: &str) -> IpcResponse {
    let conn = match open_account_conn(state, alias).await {
        Ok(c) => c,
        Err(e) => return IpcResponse::err(e),
    };
    if let Err(e) = conn.execute(
        "UPDATE chats SET unread_count = 0, updated_at = ?1 WHERE id = ?2",
        turso::params::Params::Positional(vec![
            turso::Value::from(chrono::Utc::now().timestamp()),
            turso::Value::from(chat_id),
        ]),
    ).await {
        return IpcResponse::err(format!("db update: {e}"));
    }
    IpcResponse::ok(serde_json::json!({ "account": alias, "chat_id": chat_id, "action": "mark_read" }))
}

async fn cmd_message_reply(state: &DaemonState, alias: &str, chat_id: &str, reply_to_id: &str, text: &str, wait: bool, timeout: u64) -> IpcResponse {
    let conn = match open_account_conn(state, alias).await {
        Ok(c) => c,
        Err(e) => return IpcResponse::err(e),
    };
    let id = uuid::Uuid::new_v4().to_string();
    if let Err(e) = conn.execute(
        "INSERT INTO outbound_queue (id, chat_id, content, quoted_message_id, status, created_at)
         VALUES (?1, ?2, ?3, ?4, 'pending', ?5)",
        turso::params::Params::Positional(vec![
            turso::Value::from(id.as_str()),
            turso::Value::from(chat_id),
            turso::Value::from(text),
            turso::Value::from(reply_to_id),
            turso::Value::from(chrono::Utc::now().timestamp()),
        ]),
    ).await {
        return IpcResponse::err(format!("db insert: {e}"));
    }
    if wait {
        wait_for_queue_item(state, &id, timeout).await
    } else {
        IpcResponse::ok(serde_json::json!({ "account": alias, "queue_id": id, "action": "reply" }))
    }
}

async fn cmd_message_react(state: &DaemonState, alias: &str, chat_id: &str, message_id: &str, emoji: &str, wait: bool, timeout: u64) -> IpcResponse {
    let conn = match open_account_conn(state, alias).await {
        Ok(c) => c,
        Err(e) => return IpcResponse::err(e),
    };
    let id = uuid::Uuid::new_v4().to_string();
    let content = format!("REACTION:{message_id}:{emoji}");
    if let Err(e) = conn.execute(
        "INSERT INTO outbound_queue (id, chat_id, content, status, created_at)
         VALUES (?1, ?2, ?3, 'pending', ?4)",
        turso::params::Params::Positional(vec![
            turso::Value::from(id.as_str()),
            turso::Value::from(chat_id),
            turso::Value::from(content.as_str()),
            turso::Value::from(chrono::Utc::now().timestamp()),
        ]),
    ).await {
        return IpcResponse::err(format!("db insert: {e}"));
    }
    if wait {
        wait_for_queue_item(state, &id, timeout).await
    } else {
        IpcResponse::ok(serde_json::json!({ "account": alias, "queue_id": id, "action": "react" }))
    }
}

async fn cmd_message_edit(state: &DaemonState, alias: &str, chat_id: &str, message_id: &str, text: &str, wait: bool, timeout: u64) -> IpcResponse {
    let conn = match open_account_conn(state, alias).await {
        Ok(c) => c,
        Err(e) => return IpcResponse::err(e),
    };
    let id = uuid::Uuid::new_v4().to_string();
    let content = format!("EDIT:{message_id}:{text}");
    if let Err(e) = conn.execute(
        "INSERT INTO outbound_queue (id, chat_id, content, status, created_at)
         VALUES (?1, ?2, ?3, 'pending', ?4)",
        turso::params::Params::Positional(vec![
            turso::Value::from(id.as_str()),
            turso::Value::from(chat_id),
            turso::Value::from(content.as_str()),
            turso::Value::from(chrono::Utc::now().timestamp()),
        ]),
    ).await {
        return IpcResponse::err(format!("db insert: {e}"));
    }
    if wait {
        wait_for_queue_item(state, &id, timeout).await
    } else {
        IpcResponse::ok(serde_json::json!({ "account": alias, "queue_id": id, "action": "edit" }))
    }
}

async fn cmd_message_delete(state: &DaemonState, alias: &str, chat_id: &str, message_id: &str, wait: bool, timeout: u64) -> IpcResponse {
    let conn = match open_account_conn(state, alias).await {
        Ok(c) => c,
        Err(e) => return IpcResponse::err(e),
    };
    let id = uuid::Uuid::new_v4().to_string();
    let content = format!("DELETE:{message_id}");
    if let Err(e) = conn.execute(
        "INSERT INTO outbound_queue (id, chat_id, content, status, created_at)
         VALUES (?1, ?2, ?3, 'pending', ?4)",
        turso::params::Params::Positional(vec![
            turso::Value::from(id.as_str()),
            turso::Value::from(chat_id),
            turso::Value::from(content.as_str()),
            turso::Value::from(chrono::Utc::now().timestamp()),
        ]),
    ).await {
        return IpcResponse::err(format!("db insert: {e}"));
    }
    if wait {
        wait_for_queue_item(state, &id, timeout).await
    } else {
        IpcResponse::ok(serde_json::json!({ "account": alias, "queue_id": id, "action": "delete" }))
    }
}

// ── Media ─────────────────────────────────────────────────────────────────

async fn cmd_media_send(state: &DaemonState, alias: &str, chat_id: &str, file_path: &str, caption: Option<&str>, reply_to: Option<&str>, as_document: bool, wait: bool, timeout: u64) -> IpcResponse {
    let path = std::path::PathBuf::from(file_path);
    if !path.exists() {
        return IpcResponse::err(format!("File not found: {file_path}"));
    }
    let conn = match open_account_conn(state, alias).await {
        Ok(c) => c,
        Err(e) => return IpcResponse::err(e),
    };
    let id = uuid::Uuid::new_v4().to_string();
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("bin").to_lowercase();
    let mime = match ext.as_str() {
        "jpg"|"jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "mp4" => "video/mp4",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "opus" => "audio/ogg; codecs=opus",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    };
    let media_type = if as_document {
        "document"
    } else {
        match ext.as_str() {
            "jpg" | "jpeg" | "png" | "gif" | "webp" => "image",
            "mp4" | "mov" | "avi" | "mkv" => "video",
            "mp3" | "ogg" | "wav" | "opus" => "audio",
            _ => "document",
        }
    };
    let caption_text = caption.unwrap_or("");
    if let Err(e) = conn.execute(
        "INSERT INTO outbound_queue (id, chat_id, content, media_path, media_mime, quoted_message_id, media_type, status, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', ?8)",
        turso::params::Params::Positional(vec![
            turso::Value::from(id.as_str()),
            turso::Value::from(chat_id),
            turso::Value::from(caption_text),
            turso::Value::from(file_path),
            turso::Value::from(mime),
            turso::Value::from(reply_to.unwrap_or("")),
            turso::Value::from(media_type),
            turso::Value::from(chrono::Utc::now().timestamp()),
        ]),
    ).await {
        return IpcResponse::err(format!("db insert: {e}"));
    }
    if wait {
        wait_for_queue_item(state, &id, timeout).await
    } else {
        IpcResponse::ok(serde_json::json!({ "account": alias, "queue_id": id, "action": "media_send", "media_type": media_type, "as_document": as_document, "file": file_path }))
    }
}

async fn cmd_media_download(state: &DaemonState, alias: &str, message_id: &str) -> IpcResponse {
    let conn = match open_account_conn(state, alias).await {
        Ok(c) => c,
        Err(e) => return IpcResponse::err(e),
    };
    let mut rows = match conn.query(
        "SELECT chat_id, media_path, media_mime FROM messages WHERE id = ?1 AND has_media = 1",
        turso::params::Params::Positional(vec![turso::Value::from(message_id)]),
    ).await {
        Ok(r) => r,
        Err(e) => return IpcResponse::err(format!("query: {e}")),
    };
    if let Ok(Some(row)) = rows.next().await {
        let chat_id = text_val(row.get_value(0));
        let media_path = text_val(row.get_value(1));
        IpcResponse::ok(serde_json::json!({
            "account": alias, "message_id": message_id, "chat_id": chat_id,
            "media_path": media_path,
            "note": "Download not yet available via CLI; use daemon media download feature"
        }))
    } else {
        IpcResponse::err(format!("No media message found with id '{message_id}'"))
    }
}

// ── Status ────────────────────────────────────────────────────────────────

async fn cmd_status_send_text(state: &DaemonState, alias: &str, text: &str, metadata: &str, wait: bool, timeout: u64) -> IpcResponse {
    let conn = match open_account_conn(state, alias).await {
        Ok(c) => c,
        Err(e) => return IpcResponse::err(e),
    };
    let id = uuid::Uuid::new_v4().to_string();
    if let Err(e) = conn.execute(
        "INSERT INTO outbound_queue (id, chat_id, content, media_type, metadata, status, created_at)
         VALUES (?1, 'status@broadcast', ?2, 'status_text', ?3, 'pending', ?4)",
        turso::params::Params::Positional(vec![
            turso::Value::from(id.as_str()),
            turso::Value::from(text),
            turso::Value::from(metadata),
            turso::Value::from(chrono::Utc::now().timestamp()),
        ]),
    ).await {
        return IpcResponse::err(format!("db insert: {e}"));
    }
    if wait {
        wait_for_queue_item(state, &id, timeout).await
    } else {
        IpcResponse::ok(serde_json::json!({ "account": alias, "queue_id": id, "action": "status_send_text" }))
    }
}

async fn cmd_status_send_media(state: &DaemonState, alias: &str, file_path: &str, caption: Option<&str>, metadata: Option<&str>, media_type: &str, wait: bool, timeout: u64) -> IpcResponse {
    let path = std::path::PathBuf::from(file_path);
    if !path.exists() {
        return IpcResponse::err(format!("File not found: {file_path}"));
    }
    let meta = metadata.unwrap_or("");
    // Validate thumbnail is present in metadata for media status
    let meta_json: serde_json::Value = serde_json::from_str(meta).unwrap_or_default();
    let has_thumbnail = meta_json.get("thumbnail").and_then(|v| v.as_str()).map(|s| !s.is_empty()).unwrap_or(false);
    if !has_thumbnail {
        return IpcResponse::err(
            "Missing --thumbnail-file. Status media requires a JPEG thumbnail. \
             Generate one using external tools (Python with Pillow, ffmpeg, etc.) \
             and pass via --thumbnail-file".to_string()
        );
    }
    let thumb_path = meta_json["thumbnail"].as_str().unwrap();
    if !std::path::PathBuf::from(thumb_path).exists() {
        return IpcResponse::err(format!("Thumbnail file not found: {thumb_path}"));
    }
    let media_kind = if media_type == "status_video" { "video" } else { "image" };
    if let Err(e) = waos_lib::media::check_status_file_size(file_path, media_kind) {
        return IpcResponse::err(e.to_string());
    }
    let conn = match open_account_conn(state, alias).await {
        Ok(c) => c,
        Err(e) => return IpcResponse::err(e),
    };
    let id = uuid::Uuid::new_v4().to_string();
    let caption_text = caption.unwrap_or("");
    if let Err(e) = conn.execute(
        "INSERT INTO outbound_queue (id, chat_id, content, media_path, media_type, metadata, status, created_at)
         VALUES (?1, 'status@broadcast', ?2, ?3, ?4, ?5, 'pending', ?6)",
        turso::params::Params::Positional(vec![
            turso::Value::from(id.as_str()),
            turso::Value::from(caption_text),
            turso::Value::from(file_path),
            turso::Value::from(media_type),
            turso::Value::from(meta),
            turso::Value::from(chrono::Utc::now().timestamp()),
        ]),
    ).await {
        return IpcResponse::err(format!("db insert: {e}"));
    }
    if wait {
        wait_for_queue_item(state, &id, timeout).await
    } else {
        IpcResponse::ok(serde_json::json!({ "account": alias, "queue_id": id, "action": media_type }))
    }
}

async fn cmd_status_revoke(state: &DaemonState, alias: &str, message_id: &str, wait: bool, timeout: u64) -> IpcResponse {
    let conn = match open_account_conn(state, alias).await {
        Ok(c) => c,
        Err(e) => return IpcResponse::err(e),
    };
    let id = uuid::Uuid::new_v4().to_string();
    if let Err(e) = conn.execute(
        "INSERT INTO outbound_queue (id, chat_id, content, media_type, status, created_at)
         VALUES (?1, 'status@broadcast', ?2, 'status_revoke', 'pending', ?3)",
        turso::params::Params::Positional(vec![
            turso::Value::from(id.as_str()),
            turso::Value::from(message_id),
            turso::Value::from(chrono::Utc::now().timestamp()),
        ]),
    ).await {
        return IpcResponse::err(format!("db insert: {e}"));
    }
    if wait {
        wait_for_queue_item(state, &id, timeout).await
    } else {
        IpcResponse::ok(serde_json::json!({ "account": alias, "queue_id": id, "action": "status_revoke" }))
    }
}

// ── Status History ────────────────────────────────────────────────────────

async fn cmd_status_history(state: &DaemonState, alias: &str, status_filter: &str, before: Option<i64>, after: Option<i64>, limit: usize, offset: usize) -> IpcResponse {
    let conn = match open_account_conn(state, alias).await {
        Ok(c) => c,
        Err(e) => return IpcResponse::err(e),
    };

    let now = chrono::Utc::now().timestamp();
    let mut conditions: Vec<String> = Vec::new();
    let mut params: Vec<turso::Value> = Vec::new();

    if status_filter != "all" && status_filter != "expired" {
        conditions.push(format!("status = ?{}", params.len() + 1));
        params.push(turso::Value::from(status_filter));
    }
    if let Some(ts) = before {
        conditions.push(format!("created_at < ?{}", params.len() + 1));
        params.push(turso::Value::from(ts));
    }
    if let Some(ts) = after {
        conditions.push(format!("created_at > ?{}", params.len() + 1));
        params.push(turso::Value::from(ts));
    }
    let where_clause = if conditions.is_empty() { String::new() } else { format!("WHERE {}", conditions.join(" AND ")) };

    // Total count
    let count_sql = format!("SELECT COUNT(*) FROM status_history {}", where_clause);
    let mut count_rows = match conn.query(&count_sql, turso::params::Params::Positional(params.clone())).await {
        Ok(r) => r,
        Err(e) => return IpcResponse::err(format!("count query: {e}")),
    };
    let total: i64 = match count_rows.next().await {
        Ok(Some(row)) => match row.get_value(0) { Ok(turso::Value::Integer(n)) => n, _ => 0 },
        _ => 0,
    };

    // Fetch with pagination
    let sql = format!(
        "SELECT id, media_type, content, media_path, thumbnail_path, bg_color, font, message_id, status, created_at, updated_at
         FROM status_history {} ORDER BY created_at DESC LIMIT ?{} OFFSET ?{}",
        where_clause,
        params.len() + 1,
        params.len() + 2,
    );
    params.push(turso::Value::from(limit as i64));
    params.push(turso::Value::from(offset as i64));

    let mut rows = match conn.query(&sql, turso::params::Params::Positional(params)).await {
        Ok(r) => r,
        Err(e) => return IpcResponse::err(format!("query: {e}")),
    };

    let mut results = Vec::new();
    while let Ok(Some(row)) = rows.next().await {
        let id = text_val(row.get_value(0));
        let media_type = text_val(row.get_value(1));
        let content = text_val(row.get_value(2));
        let media_path = text_val(row.get_value(3));
        let thumbnail_path = text_val(row.get_value(4));
        let bg_color = text_val(row.get_value(5));
        let font = match row.get_value(6) { Ok(turso::Value::Integer(n)) => n, _ => 0 };
        let message_id = text_val(row.get_value(7));
        let status = text_val(row.get_value(8));
        let created_at = match row.get_value(9) { Ok(turso::Value::Integer(n)) => n, _ => 0 };
        let _updated_at = match row.get_value(10) { Ok(turso::Value::Integer(n)) => n, _ => 0 };

        let is_expired = status == "posted" && created_at + 86400 < now;
        if status_filter == "expired" && !is_expired { continue; }

        let mut entry = serde_json::json!({
            "id": id,
            "media_type": media_type,
            "content": content,
            "status": status,
            "is_expired": is_expired,
            "created_at": created_at,
        });
        if !media_path.is_empty() { entry["media_path"] = serde_json::json!(media_path); }
        if !thumbnail_path.is_empty() { entry["thumbnail_path"] = serde_json::json!(thumbnail_path); }
        if !bg_color.is_empty() { entry["bg_color"] = serde_json::json!(bg_color); }
        if font != 0 { entry["font"] = serde_json::json!(font); }
        if !message_id.is_empty() { entry["message_id"] = serde_json::json!(message_id); }

        results.push(entry);
    }

    IpcResponse::ok(serde_json::json!({
        "account": alias,
        "statuses": results,
        "total": total,
        "limit": limit,
        "offset": offset,
    }))
}

async fn cmd_status_delete(state: &DaemonState, alias: &str, older_than: Option<i64>, newer_than: Option<i64>, status_filter: &str, confirmed: bool) -> IpcResponse {
    let conn = match open_account_conn(state, alias).await {
        Ok(c) => c,
        Err(e) => return IpcResponse::err(e),
    };

    let now = chrono::Utc::now().timestamp();
    let mut conditions: Vec<String> = Vec::new();
    let mut params: Vec<turso::Value> = Vec::new();

    match status_filter {
        "posted" | "revoked" => {
            conditions.push(format!("status = ?{}", params.len() + 1));
            params.push(turso::Value::from(status_filter));
        }
        "expired" => {
            conditions.push(format!("status = 'posted' AND created_at < ?{}", params.len() + 1));
            params.push(turso::Value::from(now - 86400));
        }
        "all" => {}
        _ => return IpcResponse::err(format!("Invalid status filter '{status_filter}'. Use: posted, revoked, expired, all")),
    }
    if let Some(ts) = older_than {
        conditions.push(format!("created_at < ?{}", params.len() + 1));
        params.push(turso::Value::from(ts));
    }
    if let Some(ts) = newer_than {
        conditions.push(format!("created_at > ?{}", params.len() + 1));
        params.push(turso::Value::from(ts));
    }

    let where_clause = if conditions.is_empty() { String::new() } else { format!("WHERE {}", conditions.join(" AND ")) };

    // Count
    let count_sql = format!("SELECT COUNT(*) FROM status_history {}", where_clause);
    let mut count_rows = match conn.query(&count_sql, turso::params::Params::Positional(params.clone())).await {
        Ok(r) => r,
        Err(e) => return IpcResponse::err(format!("count query: {e}")),
    };
    let total: i64 = match count_rows.next().await {
        Ok(Some(row)) => match row.get_value(0) { Ok(turso::Value::Integer(n)) => n, _ => 0 },
        _ => 0,
    };

    if !confirmed {
        return IpcResponse::ok(serde_json::json!({
            "account": alias,
            "dry_run": true,
            "count": total,
            "message": format!("{total} entries would be deleted. Set text=\"yes\" to confirm"),
        }));
    }

    match conn.execute(&format!("DELETE FROM status_history {}", where_clause), turso::params::Params::Positional(params)).await {
        Ok(_) => IpcResponse::ok(serde_json::json!({ "account": alias, "deleted": total })),
        Err(e) => IpcResponse::err(format!("delete failed: {e}")),
    }
}

// ── Contact ───────────────────────────────────────────────────────────────

fn real_val(v: Result<turso::Value, turso::Error>) -> f64 {
    match v { Ok(turso::Value::Real(f)) => f, Ok(turso::Value::Integer(n)) => n as f64, _ => 0.0 }
}

async fn cmd_contact_search(state: &DaemonState, alias: &str, query: &str) -> IpcResponse {
    let conn = match open_account_conn(state, alias).await {
        Ok(c) => c,
        Err(e) => return IpcResponse::err(e),
    };
    let pattern = format!("%{}%", query);
    let mut rows = match conn.query(
        "SELECT c.jid, COALESCE(c.push_name, c.name, ''), c.phone,
                COALESCE(cs.importance, 0.0), COALESCE(cs.frequency, 0)
         FROM contacts c LEFT JOIN contact_scores cs ON cs.jid = c.jid
         WHERE c.push_name LIKE ?1 OR c.name LIKE ?1 OR c.phone LIKE ?1
         ORDER BY cs.importance DESC LIMIT 20",
        turso::params::Params::Positional(vec![turso::Value::from(pattern)]),
    ).await {
        Ok(r) => r,
        Err(e) => return IpcResponse::err(format!("query: {e}")),
    };
    let mut results = Vec::new();
    while let Ok(Some(row)) = rows.next().await {
        results.push(serde_json::json!({
            "jid": text_val(row.get_value(0)),
            "name": text_val(row.get_value(1)),
            "phone": text_val(row.get_value(2)),
            "importance": real_val(row.get_value(3)),
            "interactions": int_val(row.get_value(4)),
        }));
    }
    IpcResponse::ok(serde_json::json!({ "contacts": results }))
}

async fn cmd_contact_info(state: &DaemonState, alias: &str, jid: &str) -> IpcResponse {
    let conn = match open_account_conn(state, alias).await {
        Ok(c) => c,
        Err(e) => return IpcResponse::err(e),
    };
    let mut rows = match conn.query(
        "SELECT c.jid, COALESCE(c.push_name, c.name, ''), c.phone, c.is_group,
                c.last_interaction_at,
                COALESCE(cs.frequency, 0), COALESCE(cs.recency, 0),
                COALESCE(cs.reply_rate, 0.0), COALESCE(cs.importance, 0.0)
         FROM contacts c LEFT JOIN contact_scores cs ON cs.jid = c.jid
         WHERE c.jid = ?1",
        turso::params::Params::Positional(vec![turso::Value::from(jid)]),
    ).await {
        Ok(r) => r,
        Err(e) => return IpcResponse::err(format!("query: {e}")),
    };
    let row = match rows.next().await {
        Ok(Some(r)) => r,
        _ => return IpcResponse::ok(serde_json::json!({})),
    };

    let mut mem_rows = match conn.query(
        "SELECT content FROM memory WHERE context_jid = ?1 AND key = 'conversation' ORDER BY created_at DESC LIMIT 1",
        turso::params::Params::Positional(vec![turso::Value::from(jid)]),
    ).await {
        Ok(r) => r,
        Err(_) => return IpcResponse::err("memory query failed"),
    };
    let memory: Option<serde_json::Value> = if let Ok(Some(mr)) = mem_rows.next().await {
        serde_json::from_str(&text_val(mr.get_value(0))).ok()
    } else { None };

    let mut edge_rows = match conn.query(
        "SELECT to_jid, edge_type, strength FROM graph_edges WHERE from_jid = ?1 LIMIT 20",
        turso::params::Params::Positional(vec![turso::Value::from(jid)]),
    ).await {
        Ok(r) => r,
        Err(_) => return IpcResponse::err("graph query failed"),
    };
    let mut edges = Vec::new();
    while let Ok(Some(er)) = edge_rows.next().await {
        edges.push(serde_json::json!({
            "to": text_val(er.get_value(0)),
            "type": text_val(er.get_value(1)),
            "strength": real_val(er.get_value(2)),
        }));
    }

    IpcResponse::ok(serde_json::json!({
        "jid": text_val(row.get_value(0)),
        "name": text_val(row.get_value(1)),
        "phone": text_val(row.get_value(2)),
        "is_group": int_val(row.get_value(3)) != 0,
        "last_interaction": int_val(row.get_value(4)),
        "scores": {
            "frequency": int_val(row.get_value(5)),
            "recency": int_val(row.get_value(6)),
            "reply_rate": real_val(row.get_value(7)),
            "importance": real_val(row.get_value(8)),
        },
        "memory": memory,
        "graph_edges": edges,
    }))
}

async fn cmd_contact_score(state: &DaemonState, alias: &str, jid: &str, value: f64) -> IpcResponse {
    let conn = match open_account_conn(state, alias).await {
        Ok(c) => c,
        Err(e) => return IpcResponse::err(e),
    };
    let now = chrono::Utc::now().timestamp();
    if let Err(e) = conn.execute(
        "INSERT INTO contact_scores (jid, importance, updated_at)
         VALUES (?1, ?2, ?3) ON CONFLICT(jid) DO UPDATE SET importance = ?2, updated_at = ?3",
        turso::params::Params::Positional(vec![
            turso::Value::from(jid),
            turso::Value::from(value.clamp(0.0, 1.0)),
            turso::Value::from(now),
        ]),
    ).await {
        return IpcResponse::err(format!("db update: {e}"));
    }
    IpcResponse::ok(serde_json::json!({ "account": alias, "jid": jid, "importance": value }))
}

// ── Memory ────────────────────────────────────────────────────────────────

async fn cmd_memory_get(state: &DaemonState, alias: &str, chat_id: &str) -> IpcResponse {
    let conn = match open_account_conn(state, alias).await {
        Ok(c) => c,
        Err(e) => return IpcResponse::err(e),
    };
    let mut rows = match conn.query(
        "SELECT content FROM memory WHERE key = 'conversation' AND context_jid = ?1 ORDER BY created_at DESC LIMIT 1",
        turso::params::Params::Positional(vec![turso::Value::from(chat_id)]),
    ).await {
        Ok(r) => r,
        Err(e) => return IpcResponse::err(format!("query: {e}")),
    };
    let result = match rows.next().await {
        Ok(Some(row)) => {
            serde_json::from_str(&text_val(row.get_value(0))).unwrap_or(serde_json::json!({"error": "parse failed"}))
        }
        _ => serde_json::json!({"notes": "", "topics": [], "pending": []}),
    };
    IpcResponse::ok(result)
}

async fn cmd_memory_set(state: &DaemonState, alias: &str, chat_id: &str, json_str: &str) -> IpcResponse {
    let conn = match open_account_conn(state, alias).await {
        Ok(c) => c,
        Err(e) => return IpcResponse::err(e),
    };
    let parsed: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(e) => return IpcResponse::err(format!("invalid json: {e}")),
    };
    let canonical = serde_json::to_string(&parsed).unwrap_or_default();
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    if let Err(e) = conn.execute(
        "INSERT INTO memory (id, key, content, context_jid, importance, created_at)
         VALUES (?1, 'conversation', ?2, ?3, 1.0, ?4)",
        turso::params::Params::Positional(vec![
            turso::Value::from(id.as_str()),
            turso::Value::from(canonical.as_str()),
            turso::Value::from(chat_id),
            turso::Value::from(now),
        ]),
    ).await {
        return IpcResponse::err(format!("db insert: {e}"));
    }
    IpcResponse::ok(serde_json::json!({ "account": alias, "chat_id": chat_id }))
}

async fn cmd_memory_search(state: &DaemonState, alias: &str, query: &str) -> IpcResponse {
    let conn = match open_account_conn(state, alias).await {
        Ok(c) => c,
        Err(e) => return IpcResponse::err(e),
    };
    // Try vector search if configured, fall back to LIKE
    if let Some(ref agent) = state.config.agent {
        if let (Some(provider), Some(api_key)) = (agent.embedding_provider.as_ref(), agent.embedding_api_key.as_ref()) {
            let model = agent.embedding_model.as_deref().unwrap_or("text-embedding-3-small");
            let dims = agent.embedding_dimensions.unwrap_or(1536);
            let embedder = waos_lib::vector_memory::create_embedding_provider(provider, api_key, model, dims);
            let vm = waos_lib::vector_memory::VectorMemory::new(embedder);
            vm.register_account(alias, conn.clone()).await;
            match vm.search_similar(alias, query, 10).await {
                Ok(results) => return IpcResponse::ok(serde_json::json!({ "results": results })),
                Err(e) => return IpcResponse::err(format!("vector search: {e}")),
            }
        }
    }
    // Fallback LIKE search
    let pattern = format!("%{}%", query);
    let mut rows = match conn.query(
        "SELECT context_jid, content FROM memory
         WHERE key = 'conversation' AND content LIKE ?1
         GROUP BY context_jid LIMIT 10",
        turso::params::Params::Positional(vec![turso::Value::from(pattern.as_str())]),
    ).await {
        Ok(r) => r,
        Err(e) => return IpcResponse::err(format!("query: {e}")),
    };
    let mut results = Vec::new();
    while let Ok(Some(row)) = rows.next().await {
        results.push(serde_json::json!({
            "chat_id": text_val(row.get_value(0)),
            "memory": text_val(row.get_value(1)),
        }));
    }
    IpcResponse::ok(serde_json::json!({ "results": results }))
}

// ── Graph ─────────────────────────────────────────────────────────────────

async fn cmd_graph_shared_groups(state: &DaemonState, alias: &str, jid1: &str, jid2: &str) -> IpcResponse {
    let conn = match open_account_conn(state, alias).await {
        Ok(c) => c,
        Err(e) => return IpcResponse::err(e),
    };
    let graph = waos_lib::graph::RelationshipGraph::new();
    graph.register_account(alias, conn).await;
    let groups = graph.shared_groups(alias, jid1, jid2).await;
    IpcResponse::ok(serde_json::json!({ "shared_groups": groups }))
}

async fn cmd_graph_introductions(state: &DaemonState, alias: &str, jid: &str) -> IpcResponse {
    let conn = match open_account_conn(state, alias).await {
        Ok(c) => c,
        Err(e) => return IpcResponse::err(e),
    };
    let graph = waos_lib::graph::RelationshipGraph::new();
    graph.register_account(alias, conn).await;
    let results = graph.introductions(alias, jid, 3).await;
    IpcResponse::ok(serde_json::json!({ "introductions": results }))
}

async fn cmd_graph_important_contacts(state: &DaemonState, alias: &str, community_id: &str) -> IpcResponse {
    let conn = match open_account_conn(state, alias).await {
        Ok(c) => c,
        Err(e) => return IpcResponse::err(e),
    };
    let graph = waos_lib::graph::RelationshipGraph::new();
    graph.register_account(alias, conn).await;
    let results = graph.important_contacts(alias, community_id).await;
    IpcResponse::ok(serde_json::json!({ "important_contacts": results }))
}

// ── Community ─────────────────────────────────────────────────────────────

async fn cmd_community_list(state: &DaemonState, alias: &str) -> IpcResponse {
    let conn = match open_account_conn(state, alias).await {
        Ok(c) => c,
        Err(e) => return IpcResponse::err(e),
    };
    let mut rows = match conn.query(
        "SELECT id, name FROM chats WHERE type = 'community' ORDER BY name ASC",
        (),
    ).await {
        Ok(r) => r,
        Err(e) => return IpcResponse::err(format!("query: {e}")),
    };
    let mut results = Vec::new();
    while let Ok(Some(row)) = rows.next().await {
        results.push(serde_json::json!({
            "id": text_val(row.get_value(0)),
            "name": text_val(row.get_value(1)),
        }));
    }
    IpcResponse::ok(serde_json::json!({ "communities": results }))
}

async fn cmd_community_info(state: &DaemonState, alias: &str, community_id: &str) -> IpcResponse {
    let conn = match open_account_conn(state, alias).await {
        Ok(c) => c,
        Err(e) => return IpcResponse::err(e),
    };
    let mut rows = match conn.query(
        "SELECT id, name, unread_count, last_message_snippet, last_message_at
         FROM chats WHERE id = ?1",
        turso::params::Params::Positional(vec![turso::Value::from(community_id)]),
    ).await {
        Ok(r) => r,
        Err(e) => return IpcResponse::err(format!("query: {e}")),
    };
    let row = match rows.next().await {
        Ok(Some(r)) => r,
        _ => return IpcResponse::ok(serde_json::json!({})),
    };
    let mut members = match conn.query(
        "SELECT c.jid, COALESCE(c.push_name, c.name, 'Unknown')
         FROM contacts c JOIN graph_edges e ON e.from_jid = c.jid
         WHERE e.to_jid = ?1 AND e.edge_type = 'member_of' LIMIT 50",
        turso::params::Params::Positional(vec![turso::Value::from(community_id)]),
    ).await {
        Ok(r) => r,
        Err(_) => return IpcResponse::err("members query failed"),
    };
    let mut member_list = Vec::new();
    while let Ok(Some(m)) = members.next().await {
        member_list.push(serde_json::json!({
            "jid": text_val(m.get_value(0)),
            "name": text_val(m.get_value(1)),
        }));
    }
    IpcResponse::ok(serde_json::json!({
        "id": text_val(row.get_value(0)),
        "name": text_val(row.get_value(1)),
        "unread_count": text_val(row.get_value(2)),
        "last_message": text_val(row.get_value(3)),
        "last_activity": int_val(row.get_value(4)),
        "members": member_list,
    }))
}

// ── Task ──────────────────────────────────────────────────────────────────

async fn cmd_task_list(state: &DaemonState, status: Option<&str>) -> IpcResponse {
    let conn = &state.db_mgr.lock().await.main_conn;
    let (sql, params) = match status {
        Some(s) => (
            "SELECT id, account, action, params, status, created_at FROM tasks WHERE status = ?1 ORDER BY created_at DESC",
            turso::params::Params::Positional(vec![turso::Value::from(s)]),
        ),
        None => (
            "SELECT id, account, action, params, status, created_at FROM tasks ORDER BY created_at DESC",
            turso::params::Params::Positional(vec![]),
        ),
    };
    let mut rows = match conn.query(sql, params).await {
        Ok(r) => r,
        Err(e) => return IpcResponse::err(format!("query: {e}")),
    };
    let mut results = Vec::new();
    while let Ok(Some(row)) = rows.next().await {
        results.push(serde_json::json!({
            "id": int_val(row.get_value(0)),
            "account": text_val(row.get_value(1)),
            "action": text_val(row.get_value(2)),
            "params": text_val(row.get_value(3)),
            "status": text_val(row.get_value(4)),
            "created_at": int_val(row.get_value(5)),
        }));
    }
    IpcResponse::ok(serde_json::json!({ "tasks": results }))
}

async fn cmd_task_complete(state: &DaemonState, id_str: Option<&str>) -> IpcResponse {
    let id: i64 = match id_str.and_then(|s| s.parse().ok()) {
        Some(n) => n,
        None => return IpcResponse::err("missing task id"),
    };
    let conn = &state.db_mgr.lock().await.main_conn;
    let now = chrono::Utc::now().timestamp();
    if let Err(e) = conn.execute(
        "UPDATE tasks SET status = 'completed', updated_at = ?1 WHERE id = ?2",
        turso::params::Params::Positional(vec![turso::Value::from(now), turso::Value::from(id)]),
    ).await {
        return IpcResponse::err(format!("db update: {e}"));
    }
    IpcResponse::ok(serde_json::json!({ "task_id": id }))
}

/// Try to execute a command via IPC. Returns Ok(()) if the daemon responded
/// and printed the result. Returns Err(()) if daemon is not running (no socket).
pub async fn try_exec(req: &IpcRequest) -> Result<(), ()> {
    let config = waos_lib::config::Config::load();
    let sock_path = socket_path(&config.data_dir());
    if !sock_path.exists() {
        return Err(());
    }
    match send_command(&sock_path, req).await {
        Ok(resp) => {
            if resp.ok {
                if let Some(data) = &resp.data {
                    println!("{}", serde_json::to_string_pretty(data).unwrap_or_default());
                } else {
                    println!(r#"{{"ok":true}}"#);
                }
            } else {
                eprintln!("Error: {}", resp.error.as_deref().unwrap_or("unknown"));
            }
            Ok(())
        }
        Err(e) => {
            eprintln!("IPC error: {e}");
            Err(())
        }
    }
}

/// Send a streaming command (e.g. chat_watch). Reads JSON lines until
/// the connection closes (timeout on server side or Ctrl+C on client side).
/// Prints each line to stdout.
pub async fn send_stream_command(req: &IpcRequest) -> Result<(), String> {
    let config = waos_lib::config::Config::load();
    let sock_path = socket_path(&config.data_dir());
    let mut stream = UnixStream::connect(&sock_path).await.map_err(|e| format!("IPC connect: {e}"))?;
    let (reader, writer) = stream.split();
    let req_json = serde_json::to_string(req).map_err(|e| format!("serialize: {e}"))?;
    let mut writer = writer;
    writer.write_all(req_json.as_bytes()).await.map_err(|e| format!("write: {e}"))?;
    writer.write_all(b"\n").await.map_err(|e| format!("write: {e}"))?;
    writer.flush().await.map_err(|e| format!("flush: {e}"))?;
    drop(writer);
    let mut lines = BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        println!("{line}");
    }
    Ok(())
}

pub async fn send_command(sock_path: &std::path::Path, req: &IpcRequest) -> anyhow::Result<IpcResponse> {
    let mut stream = UnixStream::connect(sock_path).await?;
    let (reader, mut writer) = stream.split();
    let req_json = serde_json::to_string(req)?;
    writer.write_all(req_json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;

    let mut lines = BufReader::new(reader).lines();
    if let Some(line) = lines.next_line().await? {
        let resp: IpcResponse = serde_json::from_str(&line)?;
        return Ok(resp);
    }
    anyhow::bail!("no response from daemon");
}
