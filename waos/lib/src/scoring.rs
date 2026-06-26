use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::Mutex;
use turso::params::Params;
use turso::{Connection, Value};

#[derive(Clone)]
pub struct ScoringEngine {
    conns: Arc<Mutex<HashMap<String, Connection>>>,
    decay_interval_secs: i64,
}

impl ScoringEngine {
    pub fn new() -> Self {
        Self {
            conns: Arc::new(Mutex::new(HashMap::new())),
            decay_interval_secs: 604800,
        }
    }

    pub async fn register_account(&self, alias: &str, conn: Connection) {
        self.conns.lock().await.insert(alias.to_string(), conn);
    }

    pub async fn remove_account(&self, alias: &str) {
        self.conns.lock().await.remove(alias);
    }

    pub async fn on_message_received(&self, account: &str, sender_jid: &str, is_reply: bool) {
        let conns = self.conns.lock().await;
        let conn = match conns.get(account) {
            Some(c) => c,
            None => return,
        };
        let now = Utc::now().timestamp();
        conn.execute(
            "INSERT INTO contact_scores (jid, frequency, recency, reply_rate, updated_at)
             VALUES (?1, 1, ?2, ?3, ?2)
             ON CONFLICT(jid) DO UPDATE SET
               frequency = frequency + 1,
               recency = ?2,
               reply_rate = CASE WHEN ?3 THEN MIN(1.0, reply_rate + 0.1) ELSE reply_rate END,
               updated_at = ?2",
            Params::Positional(vec![
                Value::from(sender_jid),
                Value::from(now),
                Value::from(is_reply),
            ]),
        ).await.ok();

        conn.execute(
            "UPDATE contacts SET last_interaction_at = ?1, updated_at = ?1 WHERE jid = ?2",
            Params::Positional(vec![Value::from(now), Value::from(sender_jid)]),
        ).await.ok();

        self.upsert_graph_nodes(conn, sender_jid, "contact").await;
    }

    pub async fn on_message_sent(&self, account: &str, to_jid: &str) {
        let conns = self.conns.lock().await;
        let conn = match conns.get(account) {
            Some(c) => c,
            None => return,
        };
        let now = Utc::now().timestamp();
        conn.execute(
            "INSERT INTO contact_scores (jid, frequency, recency, updated_at)
             VALUES (?1, 0, ?2, ?2)
             ON CONFLICT(jid) DO UPDATE SET
               frequency = frequency + 1,
               recency = ?2,
               updated_at = ?2",
            Params::Positional(vec![Value::from(to_jid), Value::from(now)]),
        ).await.ok();
    }

    pub async fn decay_scores(&self) {
        let week_ago = Utc::now().timestamp() - self.decay_interval_secs;
        let conns = self.conns.lock().await;
        for conn in conns.values() {
            conn.execute(
                "UPDATE contact_scores SET
                  frequency = MAX(0, frequency - 1),
                  recency = MAX(0, recency - 1),
                  importance = MAX(0.0, importance - 0.05),
                  updated_at = ?1
                 WHERE updated_at < ?1",
                Params::Positional(vec![Value::from(week_ago)]),
            ).await.ok();
        }
    }

    pub async fn set_importance(&self, account: &str, jid: &str, importance: f64) {
        let conns = self.conns.lock().await;
        if let Some(conn) = conns.get(account) {
            conn.execute(
                "INSERT INTO contact_scores (jid, importance, updated_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(jid) DO UPDATE SET
                   importance = ?2,
                   updated_at = ?3",
                Params::Positional(vec![
                    Value::from(jid),
                    Value::from(importance.clamp(0.0, 1.0)),
                    Value::from(Utc::now().timestamp()),
                ]),
            ).await.ok();
        }
    }

    pub async fn get_score(&self, account: &str, jid: &str) -> Option<ContactScore> {
        let conns = self.conns.lock().await;
        let conn = conns.get(account)?;
        let mut rows = conn.query(
            "SELECT frequency, recency, reply_rate, importance FROM contact_scores WHERE jid = ?1",
            Params::Positional(vec![Value::from(jid)]),
        ).await.ok()?;
        let row = rows.next().await.ok()??;
        Some(ContactScore {
            frequency: match row.get_value(0) { Ok(Value::Integer(n)) => n as u32, _ => 0 },
            recency: match row.get_value(1) { Ok(Value::Integer(n)) => n as i64, _ => 0 },
            reply_rate: match row.get_value(2) { Ok(Value::Real(f)) => f, _ => 0.0 },
            importance: match row.get_value(3) { Ok(Value::Real(f)) => f, _ => 0.0 },
        })
    }

    pub async fn priority_contacts(&self, account: &str, limit: usize) -> Vec<ScoredContact> {
        let conns = self.conns.lock().await;
        let conn = match conns.get(account) {
            Some(c) => c,
            None => return vec![],
        };
        let mut rows = match conn.query(
            "SELECT c.jid, c.push_name, c.phone,
                    cs.frequency, cs.recency, cs.importance, cs.reply_rate,
                    (SELECT COUNT(*) FROM graph_edges e WHERE e.from_jid = c.jid) AS centrality,
                    (SELECT COUNT(*) FROM memory m WHERE m.context_jid = c.jid) AS memory_count
             FROM contacts c
             JOIN contact_scores cs ON cs.jid = c.jid
             ORDER BY (cs.frequency * 0.25 + cs.recency * 0.20 + cs.importance * 0.25 +
                       COALESCE((SELECT COUNT(*) FROM graph_edges e WHERE e.from_jid = c.jid), 0) * 0.15 +
                       cs.reply_rate * 0.10 +
                       LEAST((SELECT COUNT(*) FROM memory m WHERE m.context_jid = c.jid), 10) * 0.05) DESC
             LIMIT ?1",
            Params::Positional(vec![Value::from(limit as i64)]),
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
            results.push(ScoredContact {
                jid: match row.get_value(0) { Ok(Value::Text(s)) => s, _ => String::new() },
                name: match row.get_value(1) { Ok(Value::Text(s)) => s, _ => String::new() },
                phone: match row.get_value(2) { Ok(Value::Text(s)) => s, _ => String::new() },
                frequency: match row.get_value(3) { Ok(Value::Integer(n)) => n as u32, _ => 0 },
                recency: match row.get_value(4) { Ok(Value::Integer(n)) => n as i64, _ => 0 },
                importance: match row.get_value(5) { Ok(Value::Real(f)) => f, _ => 0.0 },
                reply_rate: match row.get_value(6) { Ok(Value::Real(f)) => f, _ => 0.0 },
                centrality: match row.get_value(7) { Ok(Value::Integer(n)) => n as u32, _ => 0 },
                memory_count: match row.get_value(8) { Ok(Value::Integer(n)) => n as u32, _ => 0 },
            });
        }
        results
    }

    async fn upsert_graph_nodes(&self, conn: &Connection, jid: &str, node_type: &str) {
        let now = Utc::now().timestamp();
        conn.execute(
            "INSERT OR IGNORE INTO graph_nodes (jid, node_type, name, created_at) VALUES (?1, ?2, '', ?3)",
            Params::Positional(vec![Value::from(jid), Value::from(node_type), Value::from(now)]),
        ).await.ok();
    }
}

#[derive(Debug, Clone)]
pub struct ContactScore {
    pub frequency: u32,
    pub recency: i64,
    pub reply_rate: f64,
    pub importance: f64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ScoredContact {
    pub jid: String,
    pub name: String,
    pub phone: String,
    pub frequency: u32,
    pub recency: i64,
    pub importance: f64,
    pub reply_rate: f64,
    pub centrality: u32,
    pub memory_count: u32,
}
