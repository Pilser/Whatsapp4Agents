pub mod migrations;
pub mod users;
pub mod sessions;

use std::sync::Arc;
use turso::{Connection, Database};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct DbPool {
    conn: Arc<Mutex<Connection>>,
    _db: Arc<Database>,
}

impl DbPool {
    pub async fn open(db_path: &str) -> anyhow::Result<Self> {
        if let Some(parent) = std::path::Path::new(db_path).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let db = turso::Builder::new_local(db_path)
            .build()
            .await?;
        let conn = db.connect()?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            _db: Arc::new(db),
        })
    }

    pub async fn migrate(&self) -> anyhow::Result<()> {
        let conn = self.conn.lock().await;
        conn.execute_batch(migrations::SCHEMA).await?;
        for migration in migrations::MIGRATIONS {
            let _ = conn.execute_batch(migration).await;
        }
        Ok(())
    }

    pub async fn conn(&self) -> tokio::sync::MutexGuard<'_, Connection> {
        self.conn.lock().await
    }
}
