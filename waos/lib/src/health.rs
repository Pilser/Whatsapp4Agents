use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use tokio::sync::Mutex;
use turso::params::Params;
use turso::{Connection, Value};

#[derive(Clone)]
pub struct HealthCollector {
    main_db_path: PathBuf,
    media_dir: PathBuf,
    daemon_started_at: i64,
    start_time: Arc<Instant>,
    conns: Arc<Mutex<HashMap<String, Connection>>>,
}

impl HealthCollector {
    pub fn new(main_db_path: PathBuf, media_dir: PathBuf) -> Self {
        Self {
            main_db_path,
            media_dir,
            daemon_started_at: Utc::now().timestamp(),
            start_time: Arc::new(Instant::now()),
            conns: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn register_account(&self, alias: &str, conn: Connection) {
        self.conns.lock().await.insert(alias.to_string(), conn);
    }

    pub async fn remove_account(&self, alias: &str) {
        self.conns.lock().await.remove(alias);
    }

    pub async fn collect(&self) -> serde_json::Value {
        let uptime = (*self.start_time).elapsed().as_secs();
        let now = Utc::now().timestamp();
        let today_start = now - (now % 86400);
        let conns = self.conns.lock().await;

        let mut total_received = 0i64;
        let mut total_sent = 0i64;
        let mut total_pending = 0i64;
        let accounts_total = conns.len() as u32;

        for (_alias, conn) in conns.iter() {
            if let Ok(mut rows) = conn.query(
                "SELECT COUNT(*) FROM messages WHERE timestamp > ?1",
                Params::Positional(vec![Value::from(today_start)]),
            ).await {
                if let Ok(Some(row)) = rows.next().await {
                    if let Ok(Value::Integer(n)) = row.get_value(0) {
                        total_received += n;
                    }
                }
            }

            if let Ok(mut rows) = conn.query(
                "SELECT COUNT(*) FROM outbound_queue WHERE status = 'pending'",
                (),
            ).await {
                if let Ok(Some(row)) = rows.next().await {
                    if let Ok(Value::Integer(n)) = row.get_value(0) {
                        total_pending += n;
                    }
                }
            }

            if let Ok(mut rows) = conn.query(
                "SELECT COUNT(*) FROM outbound_queue WHERE sent_at > ?1",
                Params::Positional(vec![Value::from(today_start)]),
            ).await {
                if let Ok(Some(row)) = rows.next().await {
                    if let Ok(Value::Integer(n)) = row.get_value(0) {
                        total_sent += n;
                    }
                }
            }
        }

        let db_size_mb = tokio::fs::metadata(&self.main_db_path)
            .await
            .map(|m| (m.len() / (1024 * 1024)) as u64)
            .unwrap_or(0);

        let media_size = dir_size_mb(&self.media_dir).await;
        let mem_usage = get_current_memory_mb().unwrap_or(0);

        let mut acct_health = serde_json::Map::new();
        for alias in conns.keys() {
            let acct = serde_json::json!({
                "connection": "unknown",
                "connected_since": null,
                "disconnected_count_24h": 0,
            });
            acct_health.insert(alias.clone(), acct);
        }

        serde_json::json!({
            "daemon_started_at": self.daemon_started_at,
            "uptime_seconds": uptime,
            "messages_received_today": total_received,
            "messages_sent_today": total_sent,
            "outbound_queue_depth": total_pending,
            "accounts_total": accounts_total,
            "accounts_connected": 0,
            "memory_usage_mb": mem_usage,
            "db_size_mb": db_size_mb,
            "media_cache_size_mb": media_size,
            "last_checked_at": now,
            "accounts_health": serde_json::Value::Object(acct_health),
        })
    }
}

fn get_current_memory_mb() -> Option<u64> {
    let content = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in content.lines() {
        if line.starts_with("VmRSS:") {
            let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
            return Some(kb / 1024);
        }
    }
    None
}

async fn dir_size_mb(dir: &PathBuf) -> u64 {
    let mut total = 0u64;
    if let Ok(mut entries) = tokio::fs::read_dir(dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Ok(meta) = entry.metadata().await {
                total += meta.len();
            }
        }
    }
    total / (1024 * 1024)
}
