use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tracing::{error, info, warn};
use turso::params::Params;
use turso::Value;
use whatsapp_rust::client::Client;
use whatsapp_rust::Jid;
use whatsapp_rust::SendResult;
use waos_core::events::generate_id;

use crate::adapter::sender;
use crate::bridge::anti_ban::AntiBan;
use crate::bridge::typing_sim::TypingSimulator;
use crate::event_bus::EventBus;
use crate::media;
use waos_core::events::WaosEvent;

#[derive(Clone, Default)]
pub struct TimelockState {
    pub is_active: bool,
    pub enforcement_type: Option<String>,
    pub expires_at: Option<String>,
}

#[derive(Clone, Debug)]
pub struct QueueItem {
    pub id: String,
    pub account: String,
    pub chat_id: String,
    pub content: String,
    pub media_path: Option<String>,
    pub media_mime: Option<String>,
    pub media_type: String,
    pub quoted_message_id: Option<String>,
    pub status: QueueStatus,
    pub priority: i32,
    pub created_at: i64,
    pub metadata: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum QueueStatus {
    Pending,
    Sending,
    Sent,
    Failed(String),
}

pub struct OutboundQueue {
    db_conns: Arc<Mutex<HashMap<String, turso::Connection>>>,
    clients: Arc<Mutex<HashMap<String, Arc<Client>>>>,
    anti_ban: Arc<AntiBan>,
    event_bus: EventBus,
    typing: TypingSimulator,
    accounts_dir: PathBuf,
    timelock_states: Arc<Mutex<HashMap<String, TimelockState>>>,
}

impl OutboundQueue {
    pub fn new(
        db_conns: Arc<Mutex<HashMap<String, turso::Connection>>>,
        clients: Arc<Mutex<HashMap<String, Arc<Client>>>>,
        anti_ban: Arc<AntiBan>,
        event_bus: EventBus,
        accounts_dir: PathBuf,
    ) -> Self {
        Self {
            db_conns,
            clients,
            anti_ban,
            event_bus,
            typing: TypingSimulator::new(1500),
            accounts_dir,
            timelock_states: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn set_timelock_state(
        &self,
        account: &str,
        is_active: bool,
        enforcement_type: Option<String>,
        expires_at: Option<String>,
    ) {
        self.timelock_states.lock().await.insert(
            account.to_string(),
            TimelockState { is_active, enforcement_type, expires_at },
        );
        if is_active {
            warn!("[{account}] Reach-out timelock recorded — outbound sends to unknown contacts will be blocked");
        } else {
            info!("[{account}] Reach-out timelock cleared");
        }
    }

    pub async fn enqueue(
        &self,
        account: &str,
        chat_id: &str,
        content: &str,
    ) -> String {
        let id = generate_id();
        let db_path = self.accounts_dir.join(format!("{account}.db"));
        if let Ok(db) = turso::Builder::new_local(db_path.to_str().unwrap()).build().await {
            if let Ok(conn) = db.connect() {
                conn.execute(
                    "INSERT INTO outbound_queue (id, chat_id, content, status, created_at)
                     VALUES (?1, ?2, ?3, 'pending', ?4)",
                    Params::Positional(vec![
                        Value::from(id.as_str()),
                        Value::from(chat_id),
                        Value::from(content),
                        Value::from(chrono::Utc::now().timestamp()),
                    ]),
                ).await.ok();
            }
        }
        id
    }

    pub async fn run(&self) {
        loop {
            let items = self.dequeue_pending().await;
            for item in items {
                if let Err(e) = self.process_item(item).await {
                    error!("Failed to process queue item: {e}");
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    async fn dequeue_pending(&self) -> Vec<QueueItem> {
        let mut all_items = Vec::new();
        let conns = self.db_conns.lock().await;
        let clients = self.clients.lock().await;

        for (alias, conn) in conns.iter() {
            if !clients.contains_key(alias) {
                continue;
            }
            let mut rows = match conn.query(
                "SELECT id, chat_id, content, media_path, media_mime, quoted_message_id, priority, created_at, media_type, metadata
                 FROM outbound_queue WHERE status = 'pending'
                 ORDER BY priority DESC, created_at ASC LIMIT 5",
                (),
            ).await {
                Ok(r) => r,
                Err(_) => continue,
            };

            loop {
                let row = match rows.next().await {
                    Ok(Some(r)) => r,
                    Ok(None) => break,
                    Err(_) => break,
                };

                let id = match row.get_value(0) { Ok(Value::Text(s)) => s, _ => continue };
                let chat_id = match row.get_value(1) { Ok(Value::Text(s)) => s, _ => continue };
                let content = match row.get_value(2) { Ok(Value::Text(s)) => s, _ => String::new() };
                let media_path = match row.get_value(3) { Ok(Value::Text(s)) => Some(s), _ => None };
                let media_mime = match row.get_value(4) { Ok(Value::Text(s)) => Some(s), _ => None };
                let quoted = match row.get_value(5) { Ok(Value::Text(s)) => Some(s), _ => None };
                let priority = match row.get_value(6) { Ok(Value::Integer(n)) => n as i32, _ => 0 };
                let created_at = match row.get_value(7) { Ok(Value::Integer(n)) => n, _ => 0 };
                let media_type = match row.get_value(8) { Ok(Value::Text(s)) => s, _ => String::new() };
                let metadata = match row.get_value(9) { Ok(Value::Text(s)) => s, _ => String::new() };

                all_items.push(QueueItem {
                    id,
                    account: alias.clone(),
                    chat_id,
                    content,
                    media_path,
                    media_mime,
                    media_type,
                    quoted_message_id: quoted,
                    status: QueueStatus::Pending,
                    priority,
                    created_at,
                    metadata,
                });
            }
        }
        all_items
    }

    async fn process_item(&self, item: QueueItem) -> anyhow::Result<()> {
        self.anti_ban.check_can_send(&item.account).await
            .map_err(|e| anyhow::anyhow!("Anti-ban: {e}"))?;

        let clients = self.clients.lock().await;
        let client = clients.get(&item.account)
            .ok_or_else(|| anyhow::anyhow!("No client for account '{}'", item.account))?;

        let chat_jid = Jid::from_str(&item.chat_id)
            .map_err(|e| anyhow::anyhow!("Invalid JID '{}': {e}", item.chat_id))?;

        let agent_jid = client.get_pn().map(|j| j.to_string()).unwrap_or_default();
        let client = client.clone();
        drop(clients);

        // Timelock guard: block sends to unknown contacts when reach-out timelock is active.
        {
            let states = self.timelock_states.lock().await;
            if let Some(state) = states.get(&item.account) {
                if state.is_active {
                    let user = chat_jid.user.as_str();
                    match client.tc_token().get(user).await {
                        Ok(Some(_)) => {
                            info!(
                                "[{}] Send to known contact {} (tctoken present) — allowed through timelock",
                                item.account, user
                            );
                        }
                        _ => {
                            let reason = format!(
                                "Reach-out timelock active. Contact '{}' has no privacy token (tctoken). \
                                 Only contacts with prior conversation history can receive messages. \
                                 Enforcement: {}, Expires: {}",
                                user,
                                state.enforcement_type.as_deref().unwrap_or("unknown"),
                                state.expires_at.as_deref().unwrap_or("unknown"),
                            );
                            warn!("[{}] Blocked send to unknown contact {}: {}", item.account, user, reason);
                            self.mark_status(&item.id, &item.account, "failed").await;
                            self.event_bus.publish(WaosEvent::QueueItemStatusChanged {
                                account: item.account.clone(),
                                chat_id: item.chat_id.clone(),
                                queue_id: item.id.clone(),
                                status: "failed".into(),
                                message_id: None,
                                error: Some(reason.clone()),
                            });
                            return Err(anyhow::anyhow!("{reason}"));
                        }
                    }
                }
            }
        }

        self.mark_status(&item.id, &item.account, "sending").await;
        self.event_bus.publish(WaosEvent::QueueItemStatusChanged {
            account: item.account.clone(),
            chat_id: item.chat_id.clone(),
            queue_id: item.id.clone(),
            status: "sending".into(),
            message_id: None,
            error: None,
        });

        let result = self.send_message(&client, &item, &chat_jid).await;

        self.anti_ban.record_send(&item.account).await;

        match result {
            Ok(send_result) => {
                self.mark_status(&item.id, &item.account, "sent").await;
                self.event_bus.publish(WaosEvent::QueueItemStatusChanged {
                    account: item.account.clone(),
                    chat_id: item.chat_id.clone(),
                    queue_id: item.id.clone(),
                    status: "sent".into(),
                    message_id: Some(send_result.message_id.clone()),
                    error: None,
                });
                if item.media_type.starts_with("status_") {
                    if item.media_type == "status_revoke" {
                        self.update_status_by_message_id(&item.account, &item.content, "revoked").await;
                    } else {
                        self.store_status(&item, &send_result.message_id).await;
                    }
                    self.event_bus.publish(WaosEvent::StatusUpdate {
                        account: item.account.clone(),
                        status_id: item.id.clone(),
                        action: if item.media_type == "status_revoke" { "revoked".into() } else { "posted".into() },
                        message_id: Some(send_result.message_id.clone()),
                        error: None,
                        timestamp: chrono::Utc::now().timestamp(),
                    });
                } else {
                    self.store_message(&item, &send_result.message_id, &agent_jid, "sent").await;
                }
                info!("Sent message {} to {}", item.id, item.chat_id);
            }
            Err(e) => {
                let err_msg = e.to_string();
                self.mark_failed(&item.id, &item.account, &err_msg).await;
                self.event_bus.publish(WaosEvent::QueueItemStatusChanged {
                    account: item.account.clone(),
                    chat_id: item.chat_id.clone(),
                    queue_id: item.id.clone(),
                    status: "failed".into(),
                    message_id: None,
                    error: Some(err_msg.clone()),
                });
                if !item.media_type.starts_with("status_") {
                    self.store_message(&item, &item.id, &agent_jid, "failed").await;
                }
                if err_msg.contains("429") {
                    self.anti_ban.trigger_cooldown(300).await;
                }
                error!("Failed to send {}: {err_msg}", item.id);
            }
        }

        Ok(())
    }

    async fn send_message(
        &self,
        client: &Client,
        item: &QueueItem,
        chat_jid: &Jid,
    ) -> anyhow::Result<SendResult> {
        match item.media_type.as_str() {
            "status_text" => return self.send_status_text(client, item).await,
            "status_image" => return self.send_status_image(client, item).await,
            "status_video" => return self.send_status_video(client, item).await,
            "status_revoke" => return self.send_status_revoke(client, item).await,
            _ => {}
        }

        self.anti_ban.pacer().wait().await;
        let msg = if item.content.starts_with("REACTION:") {
            let parts: Vec<&str> = item.content.splitn(3, ':').collect();
            if parts.len() < 3 {
                anyhow::bail!("Invalid REACTION format");
            }
            sender::make_reaction_message(parts[1], parts[2])
        } else if item.content.starts_with("EDIT:") {
            let parts: Vec<&str> = item.content.splitn(3, ':').collect();
            if parts.len() < 3 {
                anyhow::bail!("Invalid EDIT format");
            }
            sender::make_edit_message(parts[1], parts[2])
        } else if item.content.starts_with("DELETE:") {
            let parts: Vec<&str> = item.content.splitn(2, ':').collect();
            if parts.len() < 2 {
                anyhow::bail!("Invalid DELETE format");
            }
            sender::make_delete_message(parts[1])
        } else if let Some(ref media_path) = item.media_path {
            let media_category = if !item.media_type.is_empty() {
                item.media_type.as_str()
            } else {
                item.media_mime.as_deref()
                    .and_then(|m| m.split('/').next())
                    .filter(|c| matches!(*c, "image" | "video" | "audio"))
                    .unwrap_or("document")
            };
            let quote_id = item.quoted_message_id.as_deref();
            let caption = if item.content.is_empty() { None } else { Some(item.content.as_str()) };
            return crate::media::upload_and_send(client, chat_jid, media_path, media_category, quote_id, caption).await
                .map_err(|e| anyhow::anyhow!("Media send failed: {e}"));
        } else if let Some(ref quote_id) = item.quoted_message_id {
            self.typing.simulate(client, chat_jid).await;
            sender::make_reply_message(&item.content, quote_id)
        } else {
            self.typing.simulate(client, chat_jid).await;
            sender::make_text_message(&item.content)
        };

        client.send_message(chat_jid.clone(), msg).await
            .map_err(|e| anyhow::anyhow!("Send failed: {e}"))
    }

    async fn send_status_text(&self, client: &Client, item: &QueueItem) -> anyhow::Result<SendResult> {
        use buffa::Enumeration;
        use waproto::whatsapp::message::extended_text_message::FontType;
        use whatsapp_rust::features::StatusSendOptions;
        let recipients = self.get_all_contacts(&item.account).await;
        let meta: serde_json::Value = serde_json::from_str(&item.metadata).unwrap_or_default();
        let bg = meta.get("bg").and_then(|v| u32::from_str_radix(&v.as_str().unwrap_or("FF1E6E4F").trim_start_matches("0x"), 16).ok()).unwrap_or(0xFF1E6E4F);
        let font = FontType::from_i32(meta.get("font").and_then(|v| v.as_i64()).unwrap_or(0) as i32).unwrap_or(FontType::System);
        self.anti_ban.pacer().wait().await;
        client.status().send_text(&item.content, bg, font, &recipients, StatusSendOptions::default())
            .await.map_err(|e| anyhow::anyhow!("Status text send failed: {e}"))
    }

    async fn send_status_image(&self, client: &Client, item: &QueueItem) -> anyhow::Result<SendResult> {
        use whatsapp_rust::features::StatusSendOptions;
        use whatsapp_rust::upload::UploadOptions;
        use wacore::download::MediaType;
        let file_path = item.media_path.as_deref().ok_or_else(|| anyhow::anyhow!("Missing media_path for status image"))?;
        media::check_status_file_size(file_path, "image")?;
        let meta: serde_json::Value = serde_json::from_str(&item.metadata).unwrap_or_default();
        let thumb_path = meta.get("thumbnail").and_then(|v| v.as_str()).ok_or_else(|| {
            anyhow::anyhow!("Missing thumbnail in metadata for status image. Generate a JPEG thumbnail using external tools (Python with Pillow, ffmpeg, etc.) and ensure --thumbnail-file is provided.")
        })?;
        let data = tokio::fs::read(file_path).await?;
        let thumbnail = tokio::fs::read(thumb_path).await?;
        let upload = client.upload(data, MediaType::Image, UploadOptions::default()).await?;
        let recipients = self.get_all_contacts(&item.account).await;
        let caption = if item.content.is_empty() { None } else { Some(item.content.as_str()) };
        self.anti_ban.pacer().wait().await;
        client.status().send_image(upload, thumbnail, caption, &recipients, StatusSendOptions::default())
            .await.map_err(|e| anyhow::anyhow!("Status image send failed: {e}"))
    }

    async fn send_status_video(&self, client: &Client, item: &QueueItem) -> anyhow::Result<SendResult> {
        use whatsapp_rust::features::StatusSendOptions;
        use whatsapp_rust::upload::UploadOptions;
        use wacore::download::MediaType;
        let file_path = item.media_path.as_deref().ok_or_else(|| anyhow::anyhow!("Missing media_path for status video"))?;
        media::check_status_file_size(file_path, "video")?;
        let meta: serde_json::Value = serde_json::from_str(&item.metadata).unwrap_or_default();
        let thumb_path = meta.get("thumbnail").and_then(|v| v.as_str()).ok_or_else(|| {
            anyhow::anyhow!("Missing thumbnail in metadata for status video. Generate a JPEG thumbnail using external tools (Python with Pillow, ffmpeg, etc.) and ensure --thumbnail-file is provided.")
        })?;
        let data = tokio::fs::read(file_path).await?;
        let thumbnail = tokio::fs::read(thumb_path).await?;
        let upload = client.upload(data, MediaType::Video, UploadOptions::default()).await?;
        let recipients = self.get_all_contacts(&item.account).await;
        let caption = if item.content.is_empty() { None } else { Some(item.content.as_str()) };
        self.anti_ban.pacer().wait().await;
        client.status().send_video(upload, thumbnail, 30, caption, &recipients, StatusSendOptions::default())
            .await.map_err(|e| anyhow::anyhow!("Status video send failed: {e}"))
    }

    async fn send_status_revoke(&self, client: &Client, item: &QueueItem) -> anyhow::Result<SendResult> {
        use whatsapp_rust::features::StatusSendOptions;
        let recipients = self.get_all_contacts(&item.account).await;
        self.anti_ban.pacer().wait().await;
        client.status().revoke(&item.content, &recipients, StatusSendOptions::default())
            .await.map_err(|e| anyhow::anyhow!("Status revoke failed: {e}"))
    }

    async fn get_all_contacts(&self, account: &str) -> Vec<Jid> {
        let conns = self.db_conns.lock().await;
        let mut jids = Vec::new();
        if let Some(conn) = conns.get(account) {
            if let Ok(mut rows) = conn.query(
                "SELECT jid FROM contacts WHERE is_group = 0",
                (),
            ).await {
                loop {
                    match rows.next().await {
                        Ok(Some(row)) => {
                            if let Ok(Value::Text(jid_str)) = row.get_value(0) {
                                if let Ok(jid) = Jid::from_str(&jid_str) {
                                    if jid.is_pn() || jid.is_lid() {
                                        jids.push(jid);
                                    }
                                }
                            }
                        }
                        Ok(None) => break,
                        Err(_) => break,
                    }
                }
            }
        }
        jids
    }

    async fn store_message(&self, item: &QueueItem, msg_id: &str, sender_jid: &str, status: &str) {
        if item.media_type.starts_with("status_") {
            return;
        }
        let content = if item.content.starts_with("REACTION:") {
            let parts: Vec<&str> = item.content.splitn(3, ':').collect();
            if parts.len() >= 3 { format!("Reacted: {}", parts[2]) } else { item.content.clone() }
        } else if item.content.starts_with("EDIT:") {
            let parts: Vec<&str> = item.content.splitn(3, ':').collect();
            if parts.len() >= 3 { format!("{} (edited)", parts[2]) } else { item.content.clone() }
        } else if item.content.starts_with("DELETE:") {
            "Message deleted".to_string()
        } else {
            item.content.clone()
        };

        let msg_type = if item.content.starts_with("REACTION:") { "reaction" }
        else if item.content.starts_with("EDIT:") { "edit" }
        else if item.content.starts_with("DELETE:") { "delete" }
        else if item.media_path.is_some() {
            match item.media_mime.as_deref() {
                Some(m) if m.starts_with("image") => "image",
                Some(m) if m.starts_with("video") => "video",
                Some(m) if m.starts_with("audio") => "audio",
                _ => "document",
            }
        } else { "text" };

        let conns = self.db_conns.lock().await;
        if let Some(conn) = conns.get(&item.account) {
            conn.execute(
                "INSERT OR IGNORE INTO messages (id, chat_id, sender_jid, sender_name, content, has_media, message_type, media_path, media_mime, quoted_message_id, timestamp, status)
                 VALUES (?1, ?2, ?3, '', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                Params::Positional(vec![
                    Value::from(msg_id),
                    Value::from(item.chat_id.as_str()),
                    Value::from(sender_jid),
                    Value::from(content.as_str()),
                    Value::from(if item.media_path.is_some() { 1 } else { 0 }),
                    Value::from(msg_type),
                    Value::from(item.media_path.as_deref().unwrap_or("")),
                    Value::from(item.media_mime.as_deref().unwrap_or("")),
                    Value::from(item.quoted_message_id.as_deref().unwrap_or("")),
                    Value::from(chrono::Utc::now().timestamp()),
                    Value::from(status),
                ]),
            ).await.ok();
        }
    }

    async fn store_status(&self, item: &QueueItem, message_id: &str) {
        let now = chrono::Utc::now().timestamp();
        let meta: serde_json::Value = serde_json::from_str(&item.metadata).unwrap_or_default();
        let thumbnail_path = meta.get("thumbnail").and_then(|v| v.as_str()).unwrap_or("");
        let bg_color = meta.get("bg").and_then(|v| v.as_str()).unwrap_or("");
        let font = meta.get("font").and_then(|v| v.as_i64()).unwrap_or(0);
        let conns = self.db_conns.lock().await;
        if let Some(conn) = conns.get(&item.account) {
            conn.execute(
                "INSERT INTO status_history (id, media_type, content, media_path, thumbnail_path, bg_color, font, message_id, status, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'posted', ?9, ?10)",
                Params::Positional(vec![
                    Value::from(item.id.as_str()),
                    Value::from(item.media_type.as_str()),
                    Value::from(item.content.as_str()),
                    Value::from(item.media_path.as_deref().unwrap_or("")),
                    Value::from(thumbnail_path),
                    Value::from(bg_color),
                    Value::from(font),
                    Value::from(message_id),
                    Value::from(now),
                    Value::from(now),
                ]),
            ).await.ok();
        }
    }

    async fn update_status_by_message_id(&self, account: &str, message_id: &str, status: &str) {
        let now = chrono::Utc::now().timestamp();
        let conns = self.db_conns.lock().await;
        if let Some(conn) = conns.get(account) {
            conn.execute(
                "UPDATE status_history SET status = ?1, updated_at = ?2 WHERE message_id = ?3",
                Params::Positional(vec![
                    Value::from(status),
                    Value::from(now),
                    Value::from(message_id),
                ]),
            ).await.ok();
        }
    }

    async fn mark_status(&self, id: &str, account: &str, status: &str) {
        let conns = self.db_conns.lock().await;
        if let Some(conn) = conns.get(account) {
            conn.execute(
                "UPDATE outbound_queue SET status = ?1 WHERE id = ?2",
                Params::Positional(vec![Value::from(status), Value::from(id)]),
            ).await.ok();
        }
    }

    async fn mark_failed(&self, id: &str, account: &str, error: &str) {
        let conns = self.db_conns.lock().await;
        if let Some(conn) = conns.get(account) {
            conn.execute(
                "UPDATE outbound_queue SET status = 'failed', media_mime = ?1 WHERE id = ?2",
                Params::Positional(vec![Value::from(error), Value::from(id)]),
            ).await.ok();
        }
    }
}


