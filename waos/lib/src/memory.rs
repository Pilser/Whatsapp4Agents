use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use turso::params::Params;
use turso::{Connection, Value};

use waos_core::events::generate_id;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMemory {
    pub notes: String,
    pub topics: Vec<String>,
    pub pending: Vec<PendingItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingItem {
    pub description: String,
    pub due_at: Option<i64>,
    pub status: String,
}

#[derive(Clone)]
pub struct MemoryStore {
    conns: Arc<Mutex<HashMap<String, Connection>>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self {
            conns: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn register_account(&self, alias: &str, conn: Connection) {
        self.conns.lock().await.insert(alias.to_string(), conn);
    }

    pub async fn remove_account(&self, alias: &str) {
        self.conns.lock().await.remove(alias);
    }

    pub async fn get(&self, account: &str, conversation_id: &str) -> Option<ConversationMemory> {
        let conns = self.conns.lock().await;
        let conn = conns.get(account)?;
        let mut rows = conn.query(
            "SELECT content FROM memory WHERE key = 'conversation' AND context_jid = ?1 ORDER BY created_at DESC LIMIT 1",
            Params::Positional(vec![Value::from(conversation_id)]),
        ).await.ok()?;
        let row = rows.next().await.ok()??;
        let json: String = match row.get_value(0) {
            Ok(Value::Text(s)) => s,
            _ => return None,
        };
        serde_json::from_str(&json).ok()
    }

    pub async fn set(&self, account: &str, conversation_id: &str, memory: &ConversationMemory) {
        let conns = self.conns.lock().await;
        let conn = match conns.get(account) {
            Some(c) => c,
            None => return,
        };
        let json = serde_json::to_string(memory).unwrap_or_default();
        let now = Utc::now().timestamp();
        let id = generate_id();
        conn.execute(
            "INSERT INTO memory (id, key, content, context_jid, importance, created_at)
             VALUES (?1, 'conversation', ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
               content = ?2, importance = ?4",
            Params::Positional(vec![
                Value::from(id.as_str()),
                Value::from(json.as_str()),
                Value::from(conversation_id),
                Value::from(1.0),
                Value::from(now),
            ]),
        ).await.ok();
    }

    pub async fn add_topic(&self, account: &str, conversation_id: &str, topic: &str) {
        let mut memory = self.get(account, conversation_id).await.unwrap_or(ConversationMemory {
            notes: String::new(),
            topics: Vec::new(),
            pending: Vec::new(),
        });
        let topic_lower = topic.to_lowercase();
        if !memory.topics.iter().any(|t| t.to_lowercase() == topic_lower) {
            memory.topics.push(topic.to_string());
            self.set(account, conversation_id, &memory).await;
        }
    }

    pub async fn search_by_topic(&self, account: &str, topic: &str) -> Vec<String> {
        let conns = self.conns.lock().await;
        let conn = match conns.get(account) {
            Some(c) => c,
            None => return vec![],
        };
        let pattern = format!("%{}%", topic);
        let mut rows = match conn.query(
            "SELECT context_jid FROM memory
             WHERE key = 'conversation' AND content LIKE ?1
             GROUP BY context_jid",
            Params::Positional(vec![Value::from(pattern.as_str())]),
        ).await {
            Ok(r) => r,
            Err(_) => return vec![],
        };
        let mut results = Vec::new();
        loop {
            let row = match rows.next().await {
                Ok(Some(r)) => r,
                Ok(None) => break,
                Err(_) => break,
            };
            if let Ok(Value::Text(jid)) = row.get_value(0) {
                results.push(jid);
            }
        }
        results
    }

    pub async fn add_pending(&self, account: &str, conversation_id: &str, description: &str, due_at: Option<i64>) {
        let mut memory = self.get(account, conversation_id).await.unwrap_or(ConversationMemory {
            notes: String::new(),
            topics: Vec::new(),
            pending: Vec::new(),
        });
        memory.pending.push(PendingItem {
            description: description.to_string(),
            due_at,
            status: "pending".to_string(),
        });
        self.set(account, conversation_id, &memory).await;
    }

    pub async fn get_overdue(&self, account: &str) -> Vec<(String, String)> {
        let conns = self.conns.lock().await;
        let conn = match conns.get(account) {
            Some(c) => c,
            None => return vec![],
        };
        let mut rows = match conn.query(
            "SELECT context_jid, content FROM memory
             WHERE key = 'conversation' AND content LIKE '%\"status\":\"pending\"%'",
            (),
        ).await {
            Ok(r) => r,
            Err(_) => return vec![],
        };
        let mut results = Vec::new();
        loop {
            let row = match rows.next().await {
                Ok(Some(r)) => r,
                Ok(None) => break,
                Err(_) => break,
            };
            let chat_id = match row.get_value(0) { Ok(Value::Text(s)) => s, _ => continue };
            let json = match row.get_value(1) { Ok(Value::Text(s)) => s, _ => continue };
            if let Ok(memory) = serde_json::from_str::<ConversationMemory>(&json) {
                if memory.pending.iter().any(|p| p.status == "pending") {
                    results.push((chat_id, json));
                }
            }
        }
        results
    }
}
