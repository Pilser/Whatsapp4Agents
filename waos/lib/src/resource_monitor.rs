use std::path::PathBuf;

use tracing::warn;

#[derive(Clone)]
pub struct ResourceMonitor {
    max_memory_mb: u64,
    max_media_cache_mb: u64,
    media_dir: PathBuf,
    main_db_path: PathBuf,
    dash_path: PathBuf,
    delta_path: PathBuf,
}

impl ResourceMonitor {
    pub fn new(
        media_dir: PathBuf,
        main_db_path: PathBuf,
        dash_path: PathBuf,
        delta_path: PathBuf,
    ) -> Self {
        Self {
            max_memory_mb: 500,
            max_media_cache_mb: 1024,
            media_dir,
            main_db_path,
            dash_path,
            delta_path,
        }
    }

    pub async fn check(&self) -> Vec<String> {
        let mut warnings = Vec::new();

        if let Some(mem) = get_current_memory_mb() {
            if mem > self.max_memory_mb {
                warnings.push(format!("Memory usage {mem}MB exceeds limit {}MB", self.max_memory_mb));
            }
        }

        if self.media_dir.exists() {
            let cache_size = dir_size_mb(&self.media_dir).await;
            if cache_size > self.max_media_cache_mb {
                warnings.push(format!("Media cache {cache_size}MB exceeds limit {}MB", self.max_media_cache_mb));
            }
        }

        if self.main_db_path.exists() {
            if let Ok(meta) = tokio::fs::metadata(&self.main_db_path).await {
                let mb = meta.len() / (1024 * 1024);
                if mb > 100 {
                    warnings.push(format!("Main DB size {mb}MB exceeds 100MB"));
                }
            }
        }

        warnings
    }

    pub async fn clean_media_cache(&self) -> anyhow::Result<u64> {
        if !self.media_dir.exists() {
            return Ok(0);
        }
        let mut removed = 0u64;
        let mut entries = tokio::fs::read_dir(&self.media_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() {
                if let Ok(meta) = path.metadata() {
                    if let Ok(modified) = meta.modified() {
                            if let Ok(elapsed) = modified.elapsed() {
                                if elapsed.as_secs() > 7 * 86400 {
                                    let size = meta.len();
                                    let _ = tokio::fs::remove_file(&path).await;
                                    removed += size;
                                }
                            }
                        }
                }
            }
        }
        Ok(removed)
    }

    pub async fn startup_checks(&self) {
        let warnings = self.check().await;
        for w in &warnings {
            warn!("Resource warning: {w}");
        }

        if self.main_db_path.exists() {
            if let Ok(meta) = tokio::fs::metadata(&self.main_db_path).await {
                if meta.len() == 0 {
                    warn!("Main DB is empty, may need re-initialization");
                }
            }
        }

        if self.dash_path.exists() {
            if let Ok(content) = tokio::fs::read_to_string(&self.dash_path).await {
                if serde_json::from_str::<serde_json::Value>(&content).is_err() {
                    warn!("Dashboard file corrupted, will be regenerated");
                    let _ = tokio::fs::remove_file(&self.dash_path).await;
                }
            }
        }

        if self.delta_path.exists() {
            let _ = tokio::fs::remove_file(&self.delta_path).await;
        }
    }

    pub async fn replay_outbound_queues(&self, conns: &std::collections::HashMap<String, turso::Connection>) {
        for (alias, conn) in conns {
            if let Ok(mut rows) = conn.query(
                "SELECT COUNT(*) FROM outbound_queue WHERE status = 'pending'",
                (),
            ).await {
                if let Ok(Some(row)) = rows.next().await {
                    if let Ok(turso::Value::Integer(n)) = row.get_value(0) {
                        if n > 0 {
                            warn!("Account '{alias}': {n} pending outbound messages from before shutdown");
                        }
                    }
                }
            }
        }
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
