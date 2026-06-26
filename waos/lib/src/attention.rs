use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::Mutex;
use turso::Connection;
use waos_core::types::AttentionItem;

#[derive(Clone)]
pub struct AttentionEngine {
    conns: Arc<Mutex<HashMap<String, Connection>>>,
}

impl AttentionEngine {
    pub fn new() -> Self {
        Self {
            conns: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn register_account(&self, alias: &str, conn: Connection) {
        let mut map = self.conns.lock().await;
        map.insert(alias.to_string(), conn);
    }

    pub async fn remove_account(&self, alias: &str) {
        let mut map = self.conns.lock().await;
        map.remove(alias);
    }

    pub async fn rebuild_queue(&self) -> Vec<AttentionItem> {
        let map = self.conns.lock().await;
        let mut all_items = Vec::new();

        for (account, conn) in map.iter() {
            let mut rows = match conn
                .query(
                    "SELECT id, name, type, unread_count, last_message_snippet, last_message_at
                     FROM chats WHERE unread_count > 0
                     ORDER BY last_message_at DESC",
                    (),
                )
                .await
            {
                Ok(r) => r,
                Err(_) => continue,
            };

            loop {
                let row = match rows.next().await {
                    Ok(Some(r)) => r,
                    Ok(None) => break,
                    Err(_) => break,
                };

                let chat_id = match row.get_value(0) {
                    Ok(turso::Value::Text(s)) => s,
                    _ => continue,
                };
                let chat_name = match row.get_value(1) {
                    Ok(turso::Value::Text(s)) => s,
                    _ => chat_id.clone(),
                };
                let chat_type = match row.get_value(2) {
                    Ok(turso::Value::Text(s)) => s,
                    _ => String::new(),
                };
                let unread_count = match row.get_value(3) {
                    Ok(turso::Value::Integer(n)) => n as u32,
                    _ => 0,
                };
                let last_message_snippet = match row.get_value(4) {
                    Ok(turso::Value::Text(s)) => s,
                    _ => String::new(),
                };
                let last_message_at = match row.get_value(5) {
                    Ok(turso::Value::Integer(n)) => n as i64,
                    _ => 0,
                };

                let score = self.calculate_score(conn, &chat_id, unread_count, last_message_at).await;

                all_items.push(AttentionItem {
                    rank: 0,
                    account: account.clone(),
                    chat_id,
                    chat_name,
                    chat_type,
                    reason: format!("{unread_count} unread"),
                    score,
                    unread_count,
                    has_mention: false,
                    last_message_snippet,
                    last_message_at,
                });
            }
        }

        all_items.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        for (i, item) in all_items.iter_mut().enumerate() {
            item.rank = i + 1;
        }
        all_items
    }

    async fn calculate_score(
        &self,
        conn: &Connection,
        chat_id: &str,
        unread: u32,
        last_activity: i64,
    ) -> f64 {
        let now = chrono::Utc::now().timestamp();
        let recency = if now - last_activity < 3600 {
            1.0
        } else if now - last_activity < 86400 {
            0.6
        } else {
            0.2
        };

        let importance = match conn
            .query(
                "SELECT relationship_score FROM contacts WHERE jid = ?1",
                turso::params::Params::Positional(vec![turso::Value::from(chat_id)]),
            )
            .await
        {
            Ok(mut rows) => match rows.next().await {
                Ok(Some(row)) => match row.get_value(0) {
                    Ok(turso::Value::Real(f)) => f,
                    _ => 0.5,
                },
                _ => 0.5,
            },
            Err(_) => 0.5,
        };

        let unread_weight = (unread as f64 / 10.0).min(1.0) * 0.3;
        let recency_weight = recency * 0.25;
        let importance_weight = importance * 0.2;

        unread_weight + recency_weight + importance_weight
    }
}
