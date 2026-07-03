use std::collections::HashMap;
use std::path::PathBuf;

use turso::{Builder, Connection, Database};

use crate::account_schema;
use crate::main_schema;

pub struct DbManager {
    pub main: Database,
    pub main_conn: Connection,
    accounts: HashMap<String, (Database, Connection)>,
    accounts_dir: PathBuf,
}

impl DbManager {
    pub async fn open(
        main_db_path: &PathBuf,
        accounts_dir: &PathBuf,
        sessions_dir: &PathBuf,
    ) -> anyhow::Result<Self> {
        tokio::fs::create_dir_all(accounts_dir).await?;
        tokio::fs::create_dir_all(sessions_dir).await?;

        let main_db =
            Builder::new_local(main_db_path.to_str().unwrap()).build().await?;
        let main_conn = main_db.connect()?;
        main_conn.execute_batch(main_schema::SCHEMA).await?;
        main_schema::migrate_accounts(&main_conn).await;

        Ok(Self {
            main: main_db,
            main_conn,
            accounts: HashMap::new(),
            accounts_dir: accounts_dir.clone(),
        })
    }

    pub async fn open_account_db(&mut self, alias: &str) -> anyhow::Result<Connection> {
        if let Some((_, conn)) = self.accounts.get(alias) {
            return Ok(conn.clone());
        }

        let db_path = self.accounts_dir.join(format!("{alias}.db"));
        let db = Builder::new_local(db_path.to_str().unwrap()).build().await?;
        let conn = db.connect()?;
        conn.execute_batch(account_schema::SCHEMA).await?;
        account_schema::migrate_outbound_queue(&conn).await;
        account_schema::migrate_outbound_queue_v2(&conn).await;
        account_schema::migrate_wa_groups(&conn).await;
        account_schema::migrate_group_messages(&conn).await;
        account_schema::migrate_group_messages_sender_is_admin(&conn).await;

        let conn_clone = conn.clone();
        self.accounts.insert(alias.to_string(), (db, conn));
        Ok(conn_clone)
    }

    /// Open a fresh connection to an account DB (not cached, independent ConcurrentGuard).
    /// Use for background subsystems that need concurrent access (sync engine, etc.).
    pub async fn open_account_db_fresh(&mut self, alias: &str) -> anyhow::Result<Connection> {
        let db_path = self.accounts_dir.join(format!("{alias}.db"));
        let db = Builder::new_local(db_path.to_str().unwrap()).build().await?;
        let conn = db.connect()?;
        conn.execute_batch(account_schema::SCHEMA).await?;
        account_schema::migrate_outbound_queue(&conn).await;
        account_schema::migrate_outbound_queue_v2(&conn).await;
        account_schema::migrate_wa_groups(&conn).await;
        account_schema::migrate_group_messages(&conn).await;
        account_schema::migrate_group_messages_sender_is_admin(&conn).await;
        Ok(conn)
    }

    pub async fn close_account_db(&mut self, alias: &str) {
        self.accounts.remove(alias);
    }

    pub async fn delete_account_db(&mut self, alias: &str) -> anyhow::Result<()> {
        self.accounts.remove(alias);
        let db_path = self.accounts_dir.join(format!("{alias}.db"));
        if db_path.exists() {
            tokio::fs::remove_file(&db_path).await?;
        }
        Ok(())
    }

    pub fn get_account_conn(&self, alias: &str) -> Option<Connection> {
        self.accounts.get(alias).map(|(_, conn)| conn.clone())
    }

    /// Open a fresh connection to the main DB with its own ConcurrentGuard.
    /// Use this for background subsystems that run concurrently (dashboard, anti-ban, etc.).
    pub fn open_main_conn(&self) -> anyhow::Result<Connection> {
        Ok(self.main.connect()?)
    }

    pub fn list_accounts(&self) -> Vec<String> {
        self.accounts.keys().cloned().collect()
    }
}
