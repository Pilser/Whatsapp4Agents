use std::time::Duration;
use tokio::time;
use zw_db::DbPool;

pub fn spawn_ttl_sweeper(db: DbPool, every: Duration) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            time::sleep(every).await;
            match db.srv_ttl_sweep().await {
                Ok(deleted) => {
                    if deleted > 0 {
                        tracing::info!("ttl sweeper: deleted {deleted} expired records");
                    }
                }
                Err(e) => tracing::warn!("ttl sweeper: {e}"),
            }
        }
    })
}