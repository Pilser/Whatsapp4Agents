use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::{debug, error};
use turso::Connection;
use waproto::whatsapp as wa;
use waos_core::events::WaosEvent;

use crate::event_bus::EventBus;
use crate::graph::RelationshipGraph;
use crate::scoring::ScoringEngine;
use crate::vector_memory::VectorMemory;

#[derive(Clone)]
pub struct SyncEngine {
    conns: Arc<Mutex<std::collections::HashMap<String, Connection>>>,
    scoring: Option<ScoringEngine>,
    graph: Option<RelationshipGraph>,
    vector_memory: Option<VectorMemory>,
}

impl SyncEngine {
    pub fn new() -> Self {
        Self {
            conns: Arc::new(Mutex::new(std::collections::HashMap::new())),
            scoring: None,
            graph: None,
            vector_memory: None,
        }
    }

    pub fn with_scoring(mut self, scoring: ScoringEngine) -> Self {
        self.scoring = Some(scoring);
        self
    }

    pub fn with_graph(mut self, graph: RelationshipGraph) -> Self {
        self.graph = Some(graph);
        self
    }

    pub fn with_vector_memory(mut self, vm: VectorMemory) -> Self {
        self.vector_memory = Some(vm);
        self
    }

    pub async fn get_connection(&self, alias: &str) -> Option<Connection> {
        let map = self.conns.lock().await;
        map.get(alias).cloned()
    }

    pub async fn register_account(&self, alias: &str, conn: Connection) {
        let mut map = self.conns.lock().await;
        map.insert(alias.to_string(), conn);
    }

    pub async fn remove_account(&self, alias: &str) {
        let mut map = self.conns.lock().await;
        map.remove(alias);
    }

    pub async fn handle_event(&self, event: &WaosEvent, bus: &EventBus) {
        match event {
            WaosEvent::NewMessage {
                account,
                chat_id,
                message_id,
                sender_jid,
                sender_name,
                content,
                mentions,
                has_media,
                is_group,
                is_from_me: _,
                message_type,
                media_caption,
                quoted_message_id,
                quoted_content,
                quoted_sender,
                timestamp,
            } => {
                if chat_id.ends_with("@broadcast") {
                    debug!("[{account}] Skipping broadcast/status message from {chat_id}");
                    return;
                }
                let mut process = !*is_group;
                let conn = {
                    let map = self.conns.lock().await;
                    map.get(account).cloned()
                };
                if let Some(conn) = conn {
                    let chat_type = if *is_group { "group" } else { "individual" };

                    if *is_group {
                        if let Err(e) = self.ensure_group(&conn, chat_id, sender_name).await {
                            error!("[{account}] Failed to ensure group {chat_id}: {e}");
                        }
                        process = self.is_group_enabled(&conn, chat_id).await.unwrap_or(false);
                        if !process {
                            debug!("[{account}] Group {chat_id} is disabled — skipping message storage");
                        }
                    }

                    let sender_is_admin = if *is_group {
                        self.is_sender_admin(&conn, chat_id, sender_jid).await.unwrap_or(false)
                    } else {
                        false
                    };

                    if process {
                        if *is_group {
                            if let Err(e) = self
                                .store_group_message(
                                    &conn, chat_id, message_id, sender_jid,
                                    sender_name, content, *has_media, message_type,
                                    media_caption, mentions, quoted_message_id,
                                    quoted_content, quoted_sender, *timestamp,
                                    sender_is_admin,
                                )
                                .await
                            {
                                error!("Failed to store group message for {account}: {e}");
                            }
                        } else if let Err(e) = self
                            .store_message(
                                &conn, chat_id, message_id, sender_jid,
                                sender_name, content, *has_media, message_type,
                                media_caption, mentions, *timestamp,
                            )
                            .await
                        {
                            error!("Failed to store message for {account}: {e}");
                        }
                        if let Err(e) = self.update_chat_last(&conn, chat_id, content, chat_type, *timestamp).await {
                            error!("Failed to update chat for {account}: {e}");
                        }
                        if let Err(e) = self.upsert_contact(&conn, sender_jid, sender_name, *is_group).await {
                            error!("Failed to upsert contact for {account}: {e}");
                        }

                        if let Some(ref scoring) = self.scoring {
                            scoring.on_message_received(account, sender_jid, false).await;
                        }
                        if let Some(ref graph) = self.graph {
                            graph.record_interaction(account, sender_jid, chat_id, "interacted_with").await;
                        }
                        if let Some(ref vm) = self.vector_memory {
                            if let Err(e) = vm.index_message(account, message_id, content).await {
                                error!("Failed to index message vector for {account}: {e}");
                            }
                        }
                    }
                }

                if process && !mentions.is_empty() {
                    let snippet = if content.len() > 100 {
                        format!("{}...", &content[..100])
                    } else {
                        content.clone()
                    };
                    bus.publish(WaosEvent::MentionReceived {
                        account: account.clone(),
                        chat_id: chat_id.clone(),
                        chat_name: String::new(),
                        sender_name: sender_name.clone(),
                        message_snippet: snippet,
                        timestamp: *timestamp,
                    });
                }
            }
            WaosEvent::ContactPushNameUpdated { account, jid, push_name } => {
                let conn = {
                    let map = self.conns.lock().await;
                    map.get(account).cloned()
                };
                if let Some(conn) = conn {
                    if let Err(e) = self.update_push_name(&conn, jid, push_name).await {
                        error!("Failed to update push_name for {account}: {e}");
                    }
                }
            }
            WaosEvent::GroupMetadataChanged { account, group_jid, name, description } => {
                let conn = {
                    let map = self.conns.lock().await;
                    map.get(account).cloned()
                };
                if let Some(conn) = conn {
                    if let Err(e) = self.ensure_group(&conn, group_jid, name).await {
                        error!("Failed to ensure group in wa_groups for {account}: {e}");
                    }
                    if let Err(e) = self.update_group_metadata(&conn, group_jid, name, description).await {
                        error!("Failed to update group metadata for {account}: {e}");
                    }
                }
            }
            WaosEvent::GroupParticipantsChanged { account, group_jid, action, participant_jids } => {
                let conn = {
                    let map = self.conns.lock().await;
                    map.get(account).cloned()
                };
                if let Some(conn) = conn {
                    match action.as_str() {
                        "add" | "promote" => {
                            if let Err(e) = self.upsert_participants(&conn, group_jid, participant_jids, action == "promote").await {
                                error!("Failed to add participants for {account}: {e}");
                            }
                        }
                        "remove" => {
                            if let Err(e) = self.remove_participants(&conn, group_jid, participant_jids).await {
                                error!("Failed to remove participants for {account}: {e}");
                            }
                        }
                        "demote" => {
                            if let Err(e) = self.demote_participants(&conn, group_jid, participant_jids).await {
                                error!("Failed to demote participants for {account}: {e}");
                            }
                        }
                        _ => {}
                    }
                }
            }
            WaosEvent::ChatArchived { account, chat_id, archived } => {
                let conn = {
                    let map = self.conns.lock().await;
                    map.get(account).cloned()
                };
                if let Some(conn) = conn {
                    if let Err(e) = self.set_chat_archived(&conn, chat_id, *archived).await {
                        error!("Failed to set chat archived for {account}: {e}");
                    }
                }
            }
            WaosEvent::ChatMarkedRead { account, chat_id } => {
                let conn = {
                    let map = self.conns.lock().await;
                    map.get(account).cloned()
                };
                if let Some(conn) = conn {
                    if let Err(e) = self.mark_chat_read(&conn, chat_id).await {
                        error!("Failed to mark chat read for {account}: {e}");
                    }
                }
            }
            WaosEvent::ChatDeleted { account, chat_id } => {
                let conn = {
                    let map = self.conns.lock().await;
                    map.get(account).cloned()
                };
                if let Some(conn) = conn {
                    if let Err(e) = self.delete_chat(&conn, chat_id).await {
                        error!("Failed to delete chat for {account}: {e}");
                    }
                }
            }
            WaosEvent::ChatMuted { account, chat_id, muted_until } => {
                let conn = {
                    let map = self.conns.lock().await;
                    map.get(account).cloned()
                };
                if let Some(conn) = conn {
                    if let Err(e) = self.set_chat_muted(&conn, chat_id, *muted_until).await {
                        error!("Failed to set chat muted for {account}: {e}");
                    }
                }
            }
            WaosEvent::ChatPinned { account, chat_id } => {
                let conn = {
                    let map = self.conns.lock().await;
                    map.get(account).cloned()
                };
                if let Some(conn) = conn {
                    if let Err(e) = self.pin_chat(&conn, chat_id).await {
                        error!("Failed to pin chat for {account}: {e}");
                    }
                }
            }
            WaosEvent::PictureChanged { account, jid, removed } => {
                let conn = {
                    let map = self.conns.lock().await;
                    map.get(account).cloned()
                };
                if let Some(conn) = conn {
                    if let Err(e) = self.update_picture(&conn, jid, *removed).await {
                        error!("Failed to update picture for {account}: {e}");
                    }
                }
            }
            WaosEvent::MessageReceipt { account, chat_id, message_ids, receipt_type, timestamp } => {
                let conn = {
                    let map = self.conns.lock().await;
                    map.get(account).cloned()
                };
                if let Some(conn) = conn {
                    if let Err(e) = self.update_receipts(&conn, chat_id, message_ids, receipt_type, *timestamp).await {
                        error!("Failed to update receipts for {account}: {e}");
                    }
                }
            }
            WaosEvent::UndecryptableMessage { account, chat_id, message_id, sender_jid, timestamp } => {
                let conn = {
                    let map = self.conns.lock().await;
                    map.get(account).cloned()
                };
                if let Some(conn) = conn {
                    if let Err(e) = self.store_undecryptable(&conn, chat_id, message_id, sender_jid, *timestamp).await {
                        error!("Failed to store undecryptable message for {account}: {e}");
                    }
                }
            }
            WaosEvent::ContactPhoneChanged { account, old_jid, new_jid } => {
                let conn = {
                    let map = self.conns.lock().await;
                    map.get(account).cloned()
                };
                if let Some(conn) = conn {
                    if let Err(e) = self.merge_contact_jid(&conn, old_jid, new_jid).await {
                        error!("Failed to merge contact JID for {account}: {e}");
                    }
                }
            }
            WaosEvent::PresenceUpdate { account, jid, available, last_seen } => {
                let conn = {
                    let map = self.conns.lock().await;
                    map.get(account).cloned()
                };
                if let Some(conn) = conn {
                    if let Err(e) = self.update_presence(&conn, jid, *available, *last_seen).await {
                        error!("Failed to update presence for {account}: {e}");
                    }
                }
            }
            _ => {}
        }
    }

    async fn store_message(
        &self,
        conn: &Connection,
        chat_id: &str,
        message_id: &str,
        sender_jid: &str,
        sender_name: &str,
        content: &str,
        has_media: bool,
        message_type: &str,
        media_caption: &str,
        mentions: &[String],
        timestamp: i64,
    ) -> anyhow::Result<()> {
        let mentions_json = if mentions.is_empty() {
            None
        } else {
            Some(serde_json::to_string(mentions)?)
        };
        conn.execute(
            "INSERT OR IGNORE INTO messages
             (id, chat_id, sender_jid, sender_name, content, has_media,
              message_type, media_caption, mentions, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            turso::params::Params::Positional(vec![
                turso::Value::from(message_id),
                turso::Value::from(chat_id),
                turso::Value::from(sender_jid),
                turso::Value::from(sender_name),
                turso::Value::from(content),
                turso::Value::from(has_media),
                turso::Value::from(message_type),
                turso::Value::from(media_caption),
                mentions_json.map(|j| turso::Value::from(j)).unwrap_or(turso::Value::Null),
                turso::Value::from(timestamp),
            ]),
        )
        .await?;
        Ok(())
    }

    async fn store_group_message(
        &self,
        conn: &Connection,
        group_jid: &str,
        message_id: &str,
        sender_jid: &str,
        sender_name: &str,
        content: &str,
        has_media: bool,
        message_type: &str,
        media_caption: &str,
        mentions: &[String],
        quoted_message_id: &str,
        quoted_content: &str,
        quoted_sender: &str,
        timestamp: i64,
        sender_is_admin: bool,
    ) -> anyhow::Result<()> {
        let mentions_json = if mentions.is_empty() {
            None
        } else {
            Some(serde_json::to_string(mentions)?)
        };
        conn.execute(
            "INSERT OR IGNORE INTO group_messages
             (id, group_jid, sender_jid, sender_name, content, has_media,
              message_type, media_caption, mentions, quoted_message_id,
              quoted_content, quoted_sender, timestamp, sender_is_admin)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            turso::params::Params::Positional(vec![
                turso::Value::from(message_id),
                turso::Value::from(group_jid),
                turso::Value::from(sender_jid),
                turso::Value::from(sender_name),
                turso::Value::from(content),
                turso::Value::from(has_media),
                turso::Value::from(message_type),
                turso::Value::from(media_caption),
                mentions_json.map(|j| turso::Value::from(j)).unwrap_or(turso::Value::Null),
                turso::Value::from(quoted_message_id),
                turso::Value::from(quoted_content),
                turso::Value::from(quoted_sender),
                turso::Value::from(timestamp),
                turso::Value::from(sender_is_admin),
            ]),
        )
        .await?;
        Ok(())
    }

    async fn ensure_group(&self, conn: &Connection, group_jid: &str, name: &str) -> anyhow::Result<()> {
        conn.execute(
            "INSERT OR IGNORE INTO wa_groups (id, name, type, enabled) VALUES (?1, '', 'group', 0)",
            turso::params::Params::Positional(vec![turso::Value::from(group_jid)]),
        ).await?;
        if !name.is_empty() {
            conn.execute(
                "UPDATE wa_groups SET name = ?1 WHERE id = ?2 AND name = ''",
                turso::params::Params::Positional(vec![
                    turso::Value::from(name),
                    turso::Value::from(group_jid),
                ]),
            ).await?;
        }
        Ok(())
    }

    async fn is_group_enabled(&self, conn: &Connection, group_jid: &str) -> anyhow::Result<bool> {
        let mut rows = conn.query(
            "SELECT enabled FROM wa_groups WHERE id = ?1",
            turso::params::Params::Positional(vec![turso::Value::from(group_jid)]),
        ).await?;
        if let Some(row) = rows.next().await? {
            match row.get_value(0) {
                Ok(turso::Value::Integer(n)) => Ok(n != 0),
                _ => Ok(false),
            }
        } else {
            Ok(false)
        }
    }

    async fn is_sender_admin(&self, conn: &Connection, group_jid: &str, sender_jid: &str) -> anyhow::Result<bool> {
        let mut rows = conn.query(
            "SELECT is_admin FROM group_participants WHERE group_jid = ?1 AND participant_jid = ?2",
            turso::params::Params::Positional(vec![
                turso::Value::from(group_jid),
                turso::Value::from(sender_jid),
            ]),
        ).await?;
        if let Some(row) = rows.next().await? {
            match row.get_value(0) {
                Ok(turso::Value::Integer(n)) => Ok(n != 0),
                _ => Ok(false),
            }
        } else {
            Ok(false)
        }
    }

    async fn update_chat_last(
        &self,
        conn: &Connection,
        chat_id: &str,
        snippet: &str,
        chat_type: &str,
        timestamp: i64,
    ) -> anyhow::Result<()> {
        conn.execute(
            "INSERT INTO chats (id, name, type, unread_count, last_message_snippet, last_message_at, created_at, updated_at)
             VALUES (?1, '', ?2, 1, ?3, ?4, ?4, ?4)
             ON CONFLICT(id) DO UPDATE SET
               type = CASE WHEN type = 'individual' THEN ?2 ELSE type END,
               unread_count = unread_count + 1,
               last_message_snippet = ?3,
               last_message_at = ?4,
               updated_at = ?4",
            turso::params::Params::Positional(vec![
                turso::Value::from(chat_id),
                turso::Value::from(chat_type),
                turso::Value::from(snippet),
                turso::Value::from(timestamp),
            ]),
        )
        .await?;
        Ok(())
    }

    async fn upsert_contact(
        &self,
        conn: &Connection,
        jid: &str,
        push_name: &str,
        is_group: bool,
    ) -> anyhow::Result<()> {
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT INTO contacts (jid, push_name, is_group, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?4)
             ON CONFLICT(jid) DO UPDATE SET
               push_name = CASE WHEN ?2 != '' THEN ?2 ELSE push_name END,
               is_group = ?3,
               last_interaction_at = ?4,
               updated_at = ?4",
            turso::params::Params::Positional(vec![
                turso::Value::from(jid),
                turso::Value::from(push_name),
                turso::Value::from(is_group),
                turso::Value::from(now),
            ]),
        )
        .await?;
        Ok(())
    }

    async fn update_push_name(
        &self,
        conn: &Connection,
        jid: &str,
        push_name: &str,
    ) -> anyhow::Result<()> {
        let now = chrono::Utc::now().timestamp();
        if push_name.is_empty() {
            return Ok(());
        }
        conn.execute(
            "UPDATE contacts SET push_name = ?1, updated_at = ?2 WHERE jid = ?3",
            turso::params::Params::Positional(vec![
                turso::Value::from(push_name),
                turso::Value::from(now),
                turso::Value::from(jid),
            ]),
        )
        .await?;
        Ok(())
    }

    async fn update_group_metadata(
        &self,
        conn: &Connection,
        group_jid: &str,
        name: &str,
        description: &str,
    ) -> anyhow::Result<()> {
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT INTO chats (id, name, type, description, created_at, updated_at)
             VALUES (?1, ?2, 'group', ?3, ?4, ?4)
             ON CONFLICT(id) DO UPDATE SET
               name = CASE WHEN ?2 != '' THEN ?2 ELSE name END,
               description = CASE WHEN ?3 != '' THEN ?3 ELSE description END,
               updated_at = ?4",
            turso::params::Params::Positional(vec![
                turso::Value::from(group_jid),
                turso::Value::from(name),
                turso::Value::from(description),
                turso::Value::from(now),
            ]),
        )
        .await?;
        Ok(())
    }

    async fn upsert_participants(
        &self,
        conn: &Connection,
        group_jid: &str,
        participant_jids: &[String],
        is_admin: bool,
    ) -> anyhow::Result<()> {
        let now = chrono::Utc::now().timestamp();
        for p_jid in participant_jids {
            conn.execute(
                "INSERT OR REPLACE INTO group_participants (group_jid, participant_jid, is_admin, added_at)
                 VALUES (?1, ?2, ?3, ?4)",
                turso::params::Params::Positional(vec![
                    turso::Value::from(group_jid),
                    turso::Value::from(p_jid.as_str()),
                    turso::Value::from(is_admin),
                    turso::Value::from(now),
                ]),
            )
            .await?;
        }
        Ok(())
    }

    async fn remove_participants(
        &self,
        conn: &Connection,
        group_jid: &str,
        participant_jids: &[String],
    ) -> anyhow::Result<()> {
        for p_jid in participant_jids {
            conn.execute(
                "DELETE FROM group_participants WHERE group_jid = ?1 AND participant_jid = ?2",
                turso::params::Params::Positional(vec![
                    turso::Value::from(group_jid),
                    turso::Value::from(p_jid.as_str()),
                ]),
            )
            .await?;
        }
        Ok(())
    }

    async fn demote_participants(
        &self,
        conn: &Connection,
        group_jid: &str,
        participant_jids: &[String],
    ) -> anyhow::Result<()> {
        for p_jid in participant_jids {
            conn.execute(
                "UPDATE group_participants SET is_admin = 0 WHERE group_jid = ?1 AND participant_jid = ?2",
                turso::params::Params::Positional(vec![
                    turso::Value::from(group_jid),
                    turso::Value::from(p_jid.as_str()),
                ]),
            )
            .await?;
        }
        Ok(())
    }

    async fn set_chat_archived(
        &self,
        conn: &Connection,
        chat_id: &str,
        archived: bool,
    ) -> anyhow::Result<()> {
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "UPDATE chats SET archived = ?1, updated_at = ?2 WHERE id = ?3",
            turso::params::Params::Positional(vec![
                turso::Value::from(archived),
                turso::Value::from(now),
                turso::Value::from(chat_id),
            ]),
        )
        .await?;
        Ok(())
    }

    async fn mark_chat_read(
        &self,
        conn: &Connection,
        chat_id: &str,
    ) -> anyhow::Result<()> {
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "UPDATE chats SET unread_count = 0, updated_at = ?1 WHERE id = ?2",
            turso::params::Params::Positional(vec![
                turso::Value::from(now),
                turso::Value::from(chat_id),
            ]),
        )
        .await?;
        Ok(())
    }

    async fn delete_chat(
        &self,
        conn: &Connection,
        chat_id: &str,
    ) -> anyhow::Result<()> {
        conn.execute(
            "DELETE FROM messages WHERE chat_id = ?1",
            turso::params::Params::Positional(vec![turso::Value::from(chat_id)]),
        )
        .await?;
        conn.execute(
            "DELETE FROM chats WHERE id = ?1",
            turso::params::Params::Positional(vec![turso::Value::from(chat_id)]),
        )
        .await?;
        Ok(())
    }

    async fn set_chat_muted(
        &self,
        conn: &Connection,
        chat_id: &str,
        muted_until: i64,
    ) -> anyhow::Result<()> {
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "UPDATE chats SET muted_until = ?1, updated_at = ?2 WHERE id = ?3",
            turso::params::Params::Positional(vec![
                turso::Value::from(muted_until),
                turso::Value::from(now),
                turso::Value::from(chat_id),
            ]),
        )
        .await?;
        Ok(())
    }

    async fn pin_chat(
        &self,
        conn: &Connection,
        chat_id: &str,
    ) -> anyhow::Result<()> {
        let now = chrono::Utc::now().timestamp();
        conn.execute(
            "UPDATE chats SET updated_at = ?1 WHERE id = ?2",
            turso::params::Params::Positional(vec![
                turso::Value::from(now),
                turso::Value::from(chat_id),
            ]),
        )
        .await?;
        Ok(())
    }

    async fn update_picture(
        &self,
        conn: &Connection,
        jid: &str,
        removed: bool,
    ) -> anyhow::Result<()> {
        let now = chrono::Utc::now().timestamp();
        if removed {
            conn.execute(
                "UPDATE chats SET photo_url = NULL, updated_at = ?1 WHERE id = ?2",
                turso::params::Params::Positional(vec![
                    turso::Value::from(now),
                    turso::Value::from(jid),
                ]),
            )
            .await?;
        }
        Ok(())
    }

    async fn update_receipts(
        &self,
        conn: &Connection,
        chat_id: &str,
        message_ids: &[String],
        receipt_type: &str,
        timestamp: i64,
    ) -> anyhow::Result<()> {
        for msg_id in message_ids {
            conn.execute(
                "UPDATE messages SET status = ?1 WHERE id = ?2 AND chat_id = ?3",
                turso::params::Params::Positional(vec![
                    turso::Value::from(receipt_type),
                    turso::Value::from(msg_id.as_str()),
                    turso::Value::from(chat_id),
                ]),
            )
            .await?;
        }
        Ok(())
    }

    async fn store_undecryptable(
        &self,
        conn: &Connection,
        chat_id: &str,
        message_id: &str,
        sender_jid: &str,
        timestamp: i64,
    ) -> anyhow::Result<()> {
        conn.execute(
            "INSERT OR IGNORE INTO messages
             (id, chat_id, sender_jid, sender_name, content, has_media,
              message_type, timestamp)
             VALUES (?1, ?2, ?3, '', '[undecryptable]', 0, 'undecryptable', ?4)",
            turso::params::Params::Positional(vec![
                turso::Value::from(message_id),
                turso::Value::from(chat_id),
                turso::Value::from(sender_jid),
                turso::Value::from(timestamp),
            ]),
        )
        .await?;
        Ok(())
    }

    async fn merge_contact_jid(
        &self,
        conn: &Connection,
        old_jid: &str,
        new_jid: &str,
    ) -> anyhow::Result<()> {
        let now = chrono::Utc::now().timestamp();
        // Delete the new JID if it exists as a separate contact
        conn.execute(
            "DELETE FROM contacts WHERE jid = ?1",
            turso::params::Params::Positional(vec![turso::Value::from(new_jid)]),
        )
        .await?;
        // Update old JID to new JID in contacts
        conn.execute(
            "UPDATE contacts SET jid = ?1, updated_at = ?2 WHERE jid = ?3",
            turso::params::Params::Positional(vec![
                turso::Value::from(new_jid),
                turso::Value::from(now),
                turso::Value::from(old_jid),
            ]),
        )
        .await?;
        // Update messages
        conn.execute(
            "UPDATE messages SET sender_jid = ?1 WHERE sender_jid = ?2",
            turso::params::Params::Positional(vec![
                turso::Value::from(new_jid),
                turso::Value::from(old_jid),
            ]),
        )
        .await?;
        // Update group_participants
        conn.execute(
            "UPDATE group_participants SET participant_jid = ?1 WHERE participant_jid = ?2",
            turso::params::Params::Positional(vec![
                turso::Value::from(new_jid),
                turso::Value::from(old_jid),
            ]),
        )
        .await?;
        // Update graph edges
        conn.execute(
            "UPDATE graph_edges SET from_jid = ?1 WHERE from_jid = ?2",
            turso::params::Params::Positional(vec![
                turso::Value::from(new_jid),
                turso::Value::from(old_jid),
            ]),
        )
        .await?;
        conn.execute(
            "UPDATE graph_edges SET to_jid = ?1 WHERE to_jid = ?2",
            turso::params::Params::Positional(vec![
                turso::Value::from(new_jid),
                turso::Value::from(old_jid),
            ]),
        )
        .await?;
        Ok(())
    }

    async fn update_presence(
        &self,
        conn: &Connection,
        jid: &str,
        available: bool,
        last_seen: Option<i64>,
    ) -> anyhow::Result<()> {
        let now = chrono::Utc::now().timestamp();
        if !available {
            if let Some(ls) = last_seen {
                conn.execute(
                    "UPDATE contacts SET last_interaction_at = ?1, updated_at = ?2 WHERE jid = ?3",
                    turso::params::Params::Positional(vec![
                        turso::Value::from(ls),
                        turso::Value::from(now),
                        turso::Value::from(jid),
                    ]),
                )
                .await?;
            }
        }
        Ok(())
    }

    pub async fn process_history_sync(
        &self,
        conn: &Connection,
        history_sync: &waproto::whatsapp::HistorySync,
    ) -> anyhow::Result<()> {
        let now = chrono::Utc::now().timestamp();

        // Process pushnames → upsert contacts
        for pushname in &history_sync.pushnames {
            if let (Some(jid), Some(pn)) = (&pushname.id, &pushname.pushname) {
                self.upsert_contact(conn, jid, pn, false).await?;
            }
        }

        // Process conversations → upsert chats, messages, participants
        for conv in &history_sync.conversations {
            let chat_id: &str = &conv.id;

            let chat_name = conv.name.as_deref().unwrap_or("");
            let is_group = chat_id.ends_with("@g.us");
            let chat_type = if is_group { "group" } else { "individual" };
            let chat_desc = conv.description.as_deref().unwrap_or("");
            let archived = if conv.archived.unwrap_or(false) { 1 } else { 0 };
            let last_msg_ts = conv.last_msg_timestamp.unwrap_or(0) as i64;
            let unread = conv.unread_count.unwrap_or(0) as i64;

            // Upsert chat
            conn.execute(
                "INSERT INTO chats (id, name, type, description, unread_count,
                 last_message_at, archived, muted_until, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
                 ON CONFLICT(id) DO UPDATE SET
                   name = CASE WHEN ?2 != '' THEN ?2 ELSE name END,
                   type = CASE WHEN type = 'individual' THEN ?3 ELSE type END,
                   description = CASE WHEN ?4 != '' THEN ?4 ELSE description END,
                   unread_count = ?5,
                   last_message_at = CASE WHEN ?6 > last_message_at THEN ?6 ELSE last_message_at END,
                   archived = ?7,
                   muted_until = ?8,
                   updated_at = ?9",
                turso::params::Params::Positional(vec![
                    turso::Value::from(chat_id),
                    turso::Value::from(chat_name),
                    turso::Value::from(chat_type),
                    turso::Value::from(chat_desc),
                    turso::Value::from(unread),
                    turso::Value::from(last_msg_ts),
                    turso::Value::from(archived),
                    turso::Value::from(conv.mute_end_time.unwrap_or(0) as i64),
                    turso::Value::from(now),
                ]),
            )
            .await?;

            // Update contact info for individual chats / group participants
            if !is_group {
                let push_name = conv.display_name.as_deref().unwrap_or("");
                self.upsert_contact(conn, chat_id, push_name, false).await?;

                if let Some(saved_name) = &conv.name {
                    if !saved_name.is_empty() {
                        conn.execute(
                            "UPDATE contacts SET name = ?1, updated_at = ?2 WHERE jid = ?3",
                            turso::params::Params::Positional(vec![
                                turso::Value::from(saved_name.as_str()),
                                turso::Value::from(now),
                                turso::Value::from(chat_id),
                            ]),
                        ).await?;
                    }
                }

                let phone = chat_id.split('@').next().unwrap_or("");
                if !phone.is_empty() {
                    conn.execute(
                        "UPDATE contacts SET phone = ?1, updated_at = ?2 WHERE jid = ?3 AND phone IS NULL",
                        turso::params::Params::Positional(vec![
                            turso::Value::from(phone),
                            turso::Value::from(now),
                            turso::Value::from(chat_id),
                        ]),
                    ).await?;
                }
            } else {
                for p in &conv.participant {
                    self.upsert_contact(conn, &p.user_jid, "", true).await?;
                    let is_admin = matches!(p.rank, Some(wa::group_participant::Rank::Admin) | Some(wa::group_participant::Rank::SUPERADMIN));
                    conn.execute(
                        "INSERT OR IGNORE INTO group_participants (group_jid, participant_jid, is_admin, added_at) VALUES (?1, ?2, ?3, ?4)",
                        turso::params::Params::Positional(vec![
                            turso::Value::from(chat_id),
                            turso::Value::from(p.user_jid.as_str()),
                            turso::Value::from(is_admin),
                            turso::Value::from(now),
                        ]),
                    ).await?;
                }
            }

            // Ensure group is tracked in wa_groups
            if is_group {
                self.ensure_group(conn, chat_id, chat_name).await?;
            }

            // Store messages from this conversation
            for history_msg in &conv.messages {
                let Some(wmi) = history_msg.message.as_option() else { continue };
                let msg_id = match wmi.key.id.as_deref() {
                    Some(id) => id,
                    None => continue,
                };
                let msg_sender = wmi.key.participant.as_deref()
                    .or(wmi.key.remote_jid.as_deref())
                    .unwrap_or("");
                let msg_ts = wmi.message_timestamp.unwrap_or(0) as i64;
                let msg_content = wmi.message.as_option()
                    .and_then(|m| m.conversation.as_deref())
                    .unwrap_or("");

                let sender_name = "";
                let msg_status = match wmi.status {
                    Some(wa::web_message_info::Status::Error) => "error",
                    Some(wa::web_message_info::Status::Pending) => "pending",
                    Some(wa::web_message_info::Status::ServerAck) => "sent",
                    Some(wa::web_message_info::Status::DeliveryAck) => "delivered",
                    Some(wa::web_message_info::Status::Read) => "read",
                    Some(wa::web_message_info::Status::Played) => "played",
                    _ => "pending",
                };
                conn.execute(
                    "INSERT OR IGNORE INTO messages
                     (id, chat_id, sender_jid, sender_name, content, has_media, status, timestamp)
                     VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7)",
                    turso::params::Params::Positional(vec![
                        turso::Value::from(msg_id.to_string()),
                        turso::Value::from(chat_id),
                        turso::Value::from(msg_sender),
                        turso::Value::from(sender_name),
                        turso::Value::from(msg_content),
                        turso::Value::from(msg_status),
                        turso::Value::from(msg_ts),
                    ]),
                )
                .await?;
            }
        }

        Ok(())
    }
}
