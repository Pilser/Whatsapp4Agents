use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::{mpsc, Mutex};
use tracing::{debug, info, warn};
use turso::{Connection, Value};
use turso::params::Params;
use whatsapp_rust::client::Client;
use whatsapp_rust::Jid;

use waos_core::events::WaosEvent;

use super::client::ZeroClawClient;
use super::client::ZeroClawEvent;

#[derive(Clone, Debug)]
pub struct ZeroClawConfig {
    pub socket_path: PathBuf,
    pub data_dir: PathBuf,
}

impl ZeroClawConfig {
    pub fn from_config(cfg: &crate::config::Config) -> Option<Self> {
        let path = cfg.bridge.zeroclaw_socket.as_ref().map(PathBuf::from).or_else(|| {
            let home = dirs::home_dir()?;
            let p = home.join(".zeroclaw").join("daemon.sock");
            if p.exists() { Some(p) } else { None }
        })?;

        if !path.exists() {
            return None;
        }

        Some(Self {
            socket_path: path,
            data_dir: cfg.data_dir(),
        })
    }
}

pub struct ZeroClawBridge {
    socket_path: PathBuf,
    data_dir: PathBuf,
    conns: Arc<Mutex<HashMap<String, Connection>>>,
    main_conn: Option<Connection>,
    clients: Arc<Mutex<HashMap<String, Arc<Client>>>>,
}

impl ZeroClawBridge {
    pub fn new(config: ZeroClawConfig) -> Self {
        Self {
            socket_path: config.socket_path,
            data_dir: config.data_dir,
            conns: Arc::new(Mutex::new(HashMap::new())),
            main_conn: None,
            clients: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn set_main_conn(&mut self, conn: Connection) {
        self.main_conn = Some(conn);
    }

    pub fn set_clients(&mut self, clients: Arc<Mutex<HashMap<String, Arc<Client>>>>) {
        self.clients = clients;
    }

    pub async fn register_account(&self, alias: &str, conn: Connection) {
        self.conns.lock().await.insert(alias.to_string(), conn);
    }

    pub async fn get_contact_info(&self, account: &str, jid: &str) -> Option<String> {
        let conns = self.conns.lock().await;
        let conn = conns.get(account)?;
        let mut rows = conn
            .query(
                "SELECT COALESCE(push_name, name, phone, 'Unknown') FROM contacts WHERE jid = ?1",
                Params::Positional(vec![Value::from(jid)]),
            )
            .await
            .ok()?;
        let row = rows.next().await.ok()??;
        match row.get_value(0) {
            Ok(Value::Text(s)) if !s.is_empty() => Some(s),
            _ => None,
        }
    }

    async fn agent_jid(&self, account: &str) -> Option<String> {
        let session_db = self.data_dir.join("sessions").join(account).join("session.db");
        if !session_db.exists() {
            return None;
        }
        let db = turso::Builder::new_local(session_db.to_str()?).build().await.ok()?;
        let conn = db.connect().ok()?;
        let mut rows = conn.query("SELECT pn FROM device LIMIT 1", ()).await.ok()?;
        let row = rows.next().await.ok()??;
        let raw: String = match row.get_value(0) {
            Ok(turso::Value::Text(s)) => s,
            _ => return None,
        };
        if raw.is_empty() {
            return None;
        }
        let jid = if let Some(at) = raw.find('@') {
            if let Some(colon) = raw[..at].rfind(':') {
                format!("{}{}", &raw[..colon], &raw[at..])
            } else {
                raw
            }
        } else {
            raw
        };
        Some(jid)
    }

    async fn get_group_name(&self, account: &str, group_jid: &str) -> Option<String> {
        let conns = self.conns.lock().await;
        let conn = conns.get(account)?;
        let mut rows = conn
            .query(
                "SELECT name FROM wa_groups WHERE id = ?1",
                Params::Positional(vec![Value::from(group_jid)]),
            )
            .await
            .ok()?;
        let row = rows.next().await.ok()??;
        match row.get_value(0) {
            Ok(Value::Text(s)) if !s.is_empty() => Some(s),
            _ => None,
        }
    }

    async fn is_bridge_dm_enabled(&self, account: &str) -> bool {
        let conn = match self.main_conn.as_ref() {
            Some(c) => c.clone(),
            None => return false,
        };
        let mut rows = match conn
            .query(
                "SELECT bridge_dm_enabled FROM accounts WHERE alias = ?1",
                Params::Positional(vec![Value::from(account)]),
            )
            .await
        {
            Ok(r) => r,
            Err(_) => return false,
        };
        let row = match rows.next().await {
            Ok(Some(r)) => r,
            _ => return false,
        };
        match row.get_value(0) {
            Ok(Value::Integer(n)) => n == 1,
            _ => false,
        }
    }

    async fn is_bridge_group_enabled(&self, account: &str) -> bool {
        let conn = match self.main_conn.as_ref() {
            Some(c) => c.clone(),
            None => return false,
        };
        let mut rows = match conn
            .query(
                "SELECT bridge_group_enabled FROM accounts WHERE alias = ?1",
                Params::Positional(vec![Value::from(account)]),
            )
            .await
        {
            Ok(r) => r,
            Err(_) => return false,
        };
        let row = match rows.next().await {
            Ok(Some(r)) => r,
            _ => return false,
        };
        match row.get_value(0) {
            Ok(Value::Integer(n)) => n == 1,
            _ => false,
        }
    }

    async fn get_agents_for_account(&self, account: &str) -> Vec<String> {
        let conn = match self.main_conn.as_ref() {
            Some(c) => c.clone(),
            None => return Vec::new(),
        };
        let mut rows = match conn
            .query(
                "SELECT zeroclaw_agents FROM accounts WHERE alias = ?1",
                Params::Positional(vec![Value::from(account)]),
            )
            .await
        {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        let row = match rows.next().await {
            Ok(Some(r)) => r,
            _ => return Vec::new(),
        };
        let raw: String = match row.get_value(0) {
            Ok(Value::Text(s)) => s,
            _ => String::new(),
        };
        raw.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
    }

    async fn is_group_enabled(&self, account: &str, group_jid: &str) -> Option<bool> {
        let conns = self.conns.lock().await;
        let conn = conns.get(account)?;
        let mut rows = conn
            .query(
                "SELECT enabled FROM wa_groups WHERE id = ?1",
                Params::Positional(vec![Value::from(group_jid)]),
            )
            .await
            .ok()?;
        let row = rows.next().await.ok()??;
        match row.get_value(0) {
            Ok(Value::Integer(n)) => Some(n != 0),
            _ => Some(false),
        }
    }

    async fn is_sender_admin(&self, account: &str, group_jid: &str, sender_jid: &str) -> Option<bool> {
        let conns = self.conns.lock().await;
        let conn = conns.get(account)?;
        let mut rows = conn
            .query(
                "SELECT is_admin FROM group_participants WHERE group_jid = ?1 AND participant_jid = ?2",
                Params::Positional(vec![Value::from(group_jid), Value::from(sender_jid)]),
            )
            .await
            .ok()?;
        let row = rows.next().await.ok()??;
        match row.get_value(0) {
            Ok(Value::Integer(n)) => Some(n != 0),
            _ => Some(false),
        }
    }

    pub async fn run(self: Arc<Self>, mut rx: tokio::sync::broadcast::Receiver<WaosEvent>) {
        if !self.socket_path.exists() {
            info!(
                "ZeroClaw bridge disabled — socket not found at {}",
                self.socket_path.display()
            );
            return;
        }

        info!(
            "ZeroClaw bridge started — socket: {}",
            self.socket_path.display()
        );

        while let Ok(event) = rx.recv().await {
            match event {
                WaosEvent::NewMessage {
                    account,
                    chat_id,
                    message_id,
                    sender_jid,
                    sender_name,
                    content,
                    mentions,
                    is_from_me,
                    is_group,
                    timestamp,
                    quoted_message_id,
                    quoted_content,
                    quoted_sender,
                    message_type,
                    ..
                } => {
                    if is_from_me {
                        continue;
                    }

                    if chat_id.ends_with("@broadcast") {
                        debug!("[{account}] Skipping broadcast/status message from {chat_id}");
                        continue;
                    }

                    let agents = self.get_agents_for_account(&account).await;
                    if agents.is_empty() {
                        warn!("[{account}] No ZeroClaw agents configured — skipping message from {sender_jid}");
                        continue;
                    }

                    let contact_name = self.get_contact_info(&account, &sender_jid).await;
                    let socket_str = self.socket_path.to_string_lossy().to_string();

                    let context = if is_group {
                        if !self.is_bridge_group_enabled(&account).await {
                            warn!("[{account}] Group bridge disabled — skipping message from {chat_id}");
                            continue;
                        }
                        match self.is_group_enabled(&account, &chat_id).await {
                            Some(true) => {}
                            _ => {
                                debug!("[{account}] Group {chat_id} is disabled — not forwarding to agents");
                                continue;
                            }
                        }

                        let my_jid = match self.agent_jid(&account).await {
                            Some(j) => j,
                            None => {
                                debug!("[{account}] Cannot determine agent JID — skipping group message from {chat_id}");
                                continue;
                            }
                        };

                        match self.is_sender_admin(&account, &chat_id, &sender_jid).await {
                            Some(true) => {}
                            _ => {
                                debug!("[{account}] Group {chat_id}: sender {sender_jid} is not admin — skipping");
                                continue;
                            }
                        }

                        let is_reply = quoted_sender == my_jid;
                        let is_mentioned = mentions.iter().any(|j| j == &my_jid);

                        if !is_reply && !is_mentioned {
                            debug!("[{account}] Group {chat_id}: agent not mentioned or replied — skipping");
                            continue;
                        }

                        let group_name = self.get_group_name(&account, &chat_id).await
                            .unwrap_or_default();

                        if is_reply {
                            build_group_reply_context(
                                &account, &chat_id, &group_name, &message_id,
                                &sender_jid, &sender_name, &content, timestamp,
                                contact_name.as_deref(), &quoted_message_id,
                                &quoted_content,
                            )
                        } else {
                            build_group_mention_context(
                                &account, &chat_id, &group_name, &message_id,
                                &sender_jid, &sender_name, &content, timestamp,
                                contact_name.as_deref(), &mentions,
                            )
                        }
                    } else {
                        if !self.is_bridge_dm_enabled(&account).await {
                            warn!("[{account}] DM bridge disabled — skipping message from {sender_jid}");
                            continue;
                        }
                        build_message_context(
                            &account, &chat_id, &message_id, &sender_jid,
                            &sender_name, &content, false, timestamp,
                            contact_name.as_deref(), &quoted_message_id,
                            &quoted_content,
                        )
                    };

                    for agent_alias in agents {
                        let self_clone = self.clone();
                        let alias = agent_alias;
                        let acc = account.clone();
                        let cid = chat_id.clone();
                        let ctx = context.clone();
                        let sock = socket_str.clone();

                        tokio::spawn(async move {
                            if let Err(e) = self_clone
                                .process_with_typing(&sock, &acc, &cid, &alias, &ctx)
                                .await
                            {
                                warn!("[{acc}] ZeroClaw process_message failed for {alias}: {e:#}");
                            }
                        });
                    }
                }
                _ => {}
            }
        }
    }
}

fn build_message_context(
    account: &str, chat_id: &str, message_id: &str, sender_jid: &str,
    sender_name: &str, content: &str, is_group: bool, timestamp: i64,
    contact_name: Option<&str>, quoted_message_id: &str,
    quoted_content: &str,
) -> String {
    let customer = contact_name
        .filter(|n| n != &sender_name)
        .unwrap_or(sender_name);
    let mut msg = serde_json::json!({
        "channel": "waos.whatsapp-web",
        "account": account,
        "chat_id": chat_id,
        "customer": customer,
        "customer_jid": sender_jid,
        "message": content,
        "message_id": message_id,
        "message_type": if is_group { "group" } else { "direct" },
        "timestamp": timestamp,
        "reply_to": null,
    });
    if !quoted_content.is_empty() {
        msg["reply_to"] = serde_json::json!({
            "message_id": quoted_message_id,
            "message": quoted_content,
            "sender": "agent",
        });
    }
    serde_json::to_string(&msg).unwrap_or_default()
}

fn build_group_mention_context(
    account: &str, chat_id: &str, group_name: &str, message_id: &str,
    sender_jid: &str, sender_name: &str, content: &str, timestamp: i64,
    contact_name: Option<&str>, mentions: &[String],
) -> String {
    let customer = contact_name
        .filter(|n| n != &sender_name)
        .unwrap_or(sender_name);
    let group = if group_name.is_empty() {
        chat_id.to_string()
    } else {
        format!("{} ({})", group_name, chat_id)
    };
    let msg = serde_json::json!({
        "channel": "waos.whatsapp-web",
        "account": account,
        "group": group,
        "group_jid": chat_id,
        "customer": customer,
        "customer_jid": sender_jid,
        "message": content,
        "message_id": message_id,
        "message_type": "group_mention",
        "timestamp": timestamp,
        "mentions": mentions,
        "reply_to": null,
    });
    serde_json::to_string(&msg).unwrap_or_default()
}

fn build_group_reply_context(
    account: &str, chat_id: &str, group_name: &str, message_id: &str,
    sender_jid: &str, sender_name: &str, content: &str, timestamp: i64,
    contact_name: Option<&str>, quoted_message_id: &str,
    quoted_content: &str,
) -> String {
    let customer = contact_name
        .filter(|n| n != &sender_name)
        .unwrap_or(sender_name);
    let group = if group_name.is_empty() {
        chat_id.to_string()
    } else {
        format!("{} ({})", group_name, chat_id)
    };
    let mut msg = serde_json::json!({
        "channel": "waos.whatsapp-web",
        "account": account,
        "group": group,
        "group_jid": chat_id,
        "customer": customer,
        "customer_jid": sender_jid,
        "message": content,
        "message_id": message_id,
        "message_type": "group_reply",
        "timestamp": timestamp,
        "mentions": [],
        "reply_to": null,
    });
    if !quoted_content.is_empty() {
        msg["reply_to"] = serde_json::json!({
            "message_id": quoted_message_id,
            "message": quoted_content,
            "sender": "agent",
        });
    }
    serde_json::to_string(&msg).unwrap_or_default()
}

fn build_session_id(agent_alias: &str, account: &str, chat_id: &str, sender_jid: &str) -> String {
    let key = format!("whatsapp.{agent_alias}.{account}_{chat_id}_{sender_jid}");
    sanitize_key(&key)
}

fn sanitize_key(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

impl ZeroClawBridge {
    async fn process_with_typing(
        self: Arc<Self>,
        socket_path: &str,
        account: &str,
        chat_id: &str,
        agent_alias: &str,
        context: &str,
    ) -> Result<()> {
        let session_id = build_session_id(agent_alias, account, chat_id, "reply");
        let (event_tx, mut event_rx) = mpsc::unbounded_channel::<ZeroClawEvent>();

        // Spawn typing handler
        let clients = self.clients.clone();
        let acc = account.to_string();
        let cid = chat_id.to_string();
        let typing_handle = tokio::spawn(async move {
            let jid = match Jid::from_str(&cid) {
                Ok(j) => j,
                Err(_) => return,
            };
            let client = {
                let map = clients.lock().await;
                map.get(&acc).cloned()
            };
            let client = match client {
                Some(c) => c,
                None => return,
            };

            while let Some(event) = event_rx.recv().await {
                match event {
                    ZeroClawEvent::Thinking
                    | ZeroClawEvent::TextChunk
                    | ZeroClawEvent::ToolResult => {
                        let _ = client.chatstate().send_composing(&jid).await;
                    }
                    ZeroClawEvent::ToolCall => {
                        let _ = client.chatstate().send_paused(&jid).await;
                    }
                    ZeroClawEvent::Complete => {
                        let _ = client.chatstate().send_paused(&jid).await;
                        break;
                    }
                }
            }
        });

        // Connect to ZeroClaw
        let mut zc = ZeroClawClient::connect(socket_path).await?;
        zc.initialize().await?;
        let session = zc.session_new(agent_alias, &session_id).await?;

        if session.message_count > 0 {
            info!(
                "Resumed session {session_id} ({} prior messages)",
                session.message_count
            );
        } else {
            info!("Created new session {session_id}");
        }

        let reply_text = zc
            .session_prompt(&session.session_id, context, event_tx)
            .await?;

        let _ = typing_handle.await;

        // Send reply
        if !reply_text.is_empty() {
            let jid = Jid::from_str(chat_id)?;
            let client = {
                let map = self.clients.lock().await;
                map.get(account).cloned()
            };
            match client {
                Some(client) => {
                    info!("[{account}] Sending ZeroClaw reply to {chat_id} ({agent_alias})");
                    client.send_text(jid, reply_text.as_str()).await?;
                }
                None => {
                    warn!("[{account}] No client available — cannot send reply to {chat_id}");
                }
            }
        } else {
            warn!("[{account}] ZeroClaw returned empty reply for {agent_alias}");
        }

        Ok(())
    }
}


