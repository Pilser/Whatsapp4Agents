use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio::sync::Mutex;
use tracing::{error, info};
use turso::params::Params;
use turso::{Connection, Value};

#[derive(Clone)]
pub struct Orchestrator {
    conns: Arc<Mutex<HashMap<String, Connection>>>,
    webhook_url: Option<String>,
    webhook_token: Option<String>,
    check_interval: Duration,
}

impl Orchestrator {
    pub fn new(webhook_url: Option<String>, webhook_token: Option<String>) -> Self {
        Self {
            conns: Arc::new(Mutex::new(HashMap::new())),
            webhook_url,
            webhook_token,
            check_interval: Duration::from_secs(60),
        }
    }

    pub async fn register_account(&self, alias: &str, conn: Connection) {
        self.conns.lock().await.insert(alias.to_string(), conn);
    }

    pub async fn remove_account(&self, alias: &str) {
        self.conns.lock().await.remove(alias);
    }

    pub async fn run(&self) {
        info!("Orchestrator started, interval: {:?}", self.check_interval);
        loop {
            tokio::time::sleep(self.check_interval).await;
            if let Err(e) = self.check_cycle().await {
                error!("Orchestrator cycle failed: {e}");
            }
        }
    }

    async fn check_cycle(&self) -> anyhow::Result<()> {
        let conns = self.conns.lock().await;
        let now = Utc::now().timestamp();
        let stale_threshold = now - 3600;

        for (account, conn) in conns.iter() {
            let mut stale = conn.query(
                "SELECT id, name, unread_count FROM chats
                 WHERE unread_count > 0 AND last_message_at < ?1
                 ORDER BY last_message_at ASC LIMIT 5",
                Params::Positional(vec![Value::from(stale_threshold)]),
            ).await?;

            loop {
                let row = match stale.next().await {
                    Ok(Some(r)) => r,
                    Ok(None) => break,
                    Err(_) => break,
                };
                let chat_id = text_val(&row, 0);
                let chat_name = text_val(&row, 1);
                let unread = int_val(&row, 2);

                if let Some(ref url) = self.webhook_url {
                    let body = serde_json::json!({
                        "event": "stale_unread",
                        "account": account,
                        "chat_id": chat_id,
                        "chat_name": chat_name,
                        "unread_count": unread,
                    });
                    let _ = ureq::Agent::new()
                        .post(url)
                        .set("Content-Type", "application/json")
                        .send_string(&serde_json::to_string(&body)?);
                }
            }

            let mut overdue = conn.query(
                "SELECT context_jid, content FROM memory
                 WHERE key = 'conversation' AND content LIKE '%\"status\":\"pending\"%'",
                (),
            ).await?;

            loop {
                let row = match overdue.next().await {
                    Ok(Some(r)) => r,
                    Ok(None) => break,
                    Err(_) => break,
                };
                let chat_id = text_val(&row, 0);
                let json = text_val(&row, 1);

                if let Ok(memory) = serde_json::from_str::<serde_json::Value>(&json) {
                    let has_overdue = memory["pending"].as_array()
                        .map(|arr| arr.iter().any(|p| {
                            p["status"].as_str() == Some("pending") &&
                            p["due_at"].as_i64().map(|d| d < now).unwrap_or(false)
                        }))
                        .unwrap_or(false);

                    if has_overdue {
                        if let Some(ref url) = self.webhook_url {
                            let body = serde_json::json!({
                                "event": "overdue_follow_up",
                                "account": account,
                                "chat_id": chat_id,
                                "pending": memory["pending"],
                            });
                            let _ = ureq::Agent::new()
                                .post(url)
                                .set("Content-Type", "application/json")
                                .send_string(&serde_json::to_string(&body)?);
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

fn text_val(row: &turso::Row, idx: usize) -> String {
    match row.get_value(idx) { Ok(Value::Text(s)) => s, _ => String::new() }
}

fn int_val(row: &turso::Row, idx: usize) -> i64 {
    match row.get_value(idx) { Ok(Value::Integer(n)) => n, _ => 0 }
}
