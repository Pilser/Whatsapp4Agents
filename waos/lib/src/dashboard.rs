use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::Mutex;
use turso::Connection;
use waos_core::events::WaosEvent;
use waos_core::types::DashboardData;

#[derive(Clone)]
pub struct DashboardWriter {
    path: Arc<Mutex<String>>,
    delta_path: Arc<Mutex<String>>,
    db_conns: Arc<Mutex<HashMap<String, Connection>>>,
    main_conn: Option<Connection>,
}

#[derive(Serialize)]
pub struct DashboardDelta {
    pub event_type: String,
    pub chat_id: String,
    pub summary: serde_json::Value,
    pub attention_queue: Vec<serde_json::Value>,
}

impl DashboardWriter {
    pub fn new(path: &Path, delta_path: &Path) -> Self {
        Self {
            path: Arc::new(Mutex::new(path.to_string_lossy().to_string())),
            delta_path: Arc::new(Mutex::new(delta_path.to_string_lossy().to_string())),
            db_conns: Arc::new(Mutex::new(HashMap::new())),
            main_conn: None,
        }
    }

    pub fn set_main_conn(&mut self, conn: Connection) {
        self.main_conn = Some(conn);
    }

    pub async fn register_account(&self, alias: &str, conn: Connection) {
        let mut map = self.db_conns.lock().await;
        map.insert(alias.to_string(), conn);
    }

    pub async fn remove_account(&self, alias: &str) {
        let mut map = self.db_conns.lock().await;
        map.remove(alias);
    }

    pub async fn write(&self) -> anyhow::Result<()> {
        let map = self.db_conns.lock().await;
        let mut dashboard = DashboardData {
            accounts: serde_json::Value::Object(Default::default()),
            summary: serde_json::Value::Object(Default::default()),
            attention_queue: vec![],
            unread_chats: vec![],
            health: serde_json::json!({"status": "ok"}),
        };

        let mut total_unread = 0u64;
        let mut account_list = serde_json::Map::new();

        let statuses: HashMap<String, serde_json::Value> = if let Some(ref mconn) = self.main_conn {
            let mut smap = HashMap::new();
            if let Ok(mut rows) = mconn.query("SELECT alias, status, error FROM accounts", ()).await {
                while let Ok(Some(row)) = rows.next().await {
                    let alias = match row.get_value(0) { Ok(turso::Value::Text(s)) => s, _ => continue };
                    let status = match row.get_value(1) { Ok(turso::Value::Text(s)) => s, _ => String::new() };
                    let error = match row.get_value(2) { Ok(turso::Value::Text(s)) => s, _ => String::new() };
                    let mut entry = serde_json::json!({"status": status});
                    if !error.is_empty() {
                        entry["error"] = serde_json::json!(error);
                    }
                    smap.insert(alias, entry);
                }
            }
            smap
        } else {
            HashMap::new()
        };

        for (alias, conn) in map.iter() {
            let unread = conn
                .query(
                    "SELECT COUNT(*) FROM chats WHERE unread_count > 0",
                    (),
                )
                .await
                .ok();
            let total = conn
                .query("SELECT COUNT(*) FROM chats", ())
                .await
                .ok();

            let unread_count: u64 = match unread {
                Some(mut rows) => match rows.next().await {
                    Ok(Some(row)) => match row.get_value(0) {
                        Ok(turso::Value::Integer(n)) => n as u64,
                        _ => 0,
                    },
                    _ => 0,
                },
                None => 0,
            };

            total_unread += unread_count;

            let mut account_entry = statuses.get(alias.as_str())
                .cloned()
                .unwrap_or_else(|| serde_json::json!({"status": "unknown"}));
            account_entry["unread_chats"] = serde_json::json!(unread_count);
            account_entry["total_chats"] = serde_json::json!(match total {
                Some(mut rows) => match rows.next().await {
                    Ok(Some(row)) => match row.get_value(0) {
                        Ok(turso::Value::Integer(n)) => n as u64,
                        _ => 0,
                    },
                    _ => 0,
                },
                None => 0,
            });
            account_list.insert(alias.clone(), account_entry);
        }

        dashboard.accounts = serde_json::Value::Object(account_list);
        dashboard.summary = serde_json::json!({
            "total_accounts": map.len(),
            "total_unread": total_unread,
        });

        let path = self.path.lock().await.clone();
        let json = serde_json::to_string_pretty(&dashboard)?;
        tokio::fs::write(&path, json).await?;

        Ok(())
    }

    pub async fn write_delta(&self, event: &WaosEvent) -> anyhow::Result<()> {
        let map = self.db_conns.lock().await;
        let mut total_unread = 0u64;

        for (_alias, conn) in map.iter() {
            if let Ok(mut rows) = conn
                .query("SELECT COUNT(*) FROM chats WHERE unread_count > 0", ())
                .await
            {
                if let Ok(Some(row)) = rows.next().await {
                    if let Ok(turso::Value::Integer(n)) = row.get_value(0) {
                        total_unread += n as u64;
                    }
                }
            }
        }

        let chat_id = match event {
            WaosEvent::NewMessage { chat_id, .. }
            | WaosEvent::MessageEdited { chat_id, .. }
            | WaosEvent::MessageDeleted { chat_id, .. }
            | WaosEvent::ReactionAdded { chat_id, .. }
            | WaosEvent::MentionReceived { chat_id, .. }
            | WaosEvent::ChatArchived { chat_id, .. }
            | WaosEvent::ChatMarkedRead { chat_id, .. }
            | WaosEvent::ChatDeleted { chat_id, .. }
            | WaosEvent::ChatMuted { chat_id, .. }
            | WaosEvent::ChatPinned { chat_id, .. }
            | WaosEvent::MessageReceipt { chat_id, .. }
            | WaosEvent::UndecryptableMessage { chat_id, .. }
            | WaosEvent::ChatPresenceUpdate { chat_id, .. } => Some(chat_id.as_str()),
            _ => None,
        };

        let event_name = match event {
            WaosEvent::Connected { .. } => "Connected",
            WaosEvent::Disconnected { .. } => "Disconnected",
            WaosEvent::Reconnecting { .. } => "Reconnecting",
            WaosEvent::QrCodeReady { .. } => "QrCodeReady",
            WaosEvent::PairingCode { .. } => "PairingCode",
            WaosEvent::NewMessage { .. } => "NewMessage",
            WaosEvent::MessageEdited { .. } => "MessageEdited",
            WaosEvent::MessageDeleted { .. } => "MessageDeleted",
            WaosEvent::ReactionAdded { .. } => "ReactionAdded",
            WaosEvent::MentionReceived { .. } => "MentionReceived",
            WaosEvent::ContactPushNameUpdated { .. } => "ContactPushNameUpdated",
            WaosEvent::GroupMetadataChanged { .. } => "GroupMetadataChanged",
            WaosEvent::GroupParticipantsChanged { .. } => "GroupParticipantsChanged",
            WaosEvent::ChatArchived { .. } => "ChatArchived",
            WaosEvent::ChatMarkedRead { .. } => "ChatMarkedRead",
            WaosEvent::ChatDeleted { .. } => "ChatDeleted",
            WaosEvent::ChatMuted { .. } => "ChatMuted",
            WaosEvent::ChatPinned { .. } => "ChatPinned",
            WaosEvent::PictureChanged { .. } => "PictureChanged",
            WaosEvent::MessageReceipt { .. } => "MessageReceipt",
            WaosEvent::UndecryptableMessage { .. } => "UndecryptableMessage",
            WaosEvent::AccountTempBan { .. } => "AccountTempBan",
            WaosEvent::ContactPhoneChanged { .. } => "ContactPhoneChanged",
            WaosEvent::ChatPresenceUpdate { .. } => "ChatPresenceUpdate",
            WaosEvent::PresenceUpdate { .. } => "PresenceUpdate",
            WaosEvent::AccountStatusUpdate { .. } => "AccountStatusUpdate",
            WaosEvent::HistorySyncProgress { .. } => "HistorySyncProgress",
            WaosEvent::QueueItemStatusChanged { .. } => "QueueItemStatusChanged",
            WaosEvent::StatusUpdate { .. } => "StatusUpdate",
        };

        let delta = DashboardDelta {
            event_type: event_name.to_string(),
            chat_id: chat_id.unwrap_or("").to_string(),
            summary: serde_json::json!({
                "total_unread": total_unread,
                "total_accounts": map.len(),
            }),
            attention_queue: vec![],
        };

        let delta_path = self.delta_path.lock().await.clone();
        let json = serde_json::to_string(&delta)?;
        tokio::fs::write(&delta_path, json).await?;

        Ok(())
    }
}
