use std::path::Path;
use std::sync::Arc;

use parking_lot::Mutex;
use turso::{Builder, Database};

pub(crate) mod schema;
mod signal;
mod app_sync;
mod protocol;
mod device;

pub use turso::Connection;

pub(crate) fn wrap_err<T, E: std::error::Error + Send + Sync + 'static>(
    result: Result<T, E>,
) -> wacore::store::error::Result<T> {
    result.map_err(|e| {
        wacore::store::error::StoreError::Database(
            Box::new(e) as Box<dyn std::error::Error + Send + Sync>
        )
    })
}

pub(crate) fn val_to_bytes(v: turso::Value) -> Vec<u8> {
    match v {
        turso::Value::Blob(b) => b,
        _ => vec![],
    }
}

pub(crate) fn val_to_32byte_key(v: turso::Value) -> wacore::store::error::Result<[u8; 32]> {
    match v {
        turso::Value::Blob(b) if b.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&b);
            Ok(arr)
        }
        turso::Value::Blob(b) => Err(wacore::store::error::StoreError::Validation(format!(
            "identity key has invalid length {}, expected 32", b.len()
        ))),
        _ => Err(wacore::store::error::StoreError::Validation(
            "expected blob for identity key".to_string(),
        )),
    }
}

pub(crate) fn text_val(v: turso::Value) -> String {
    match v {
        turso::Value::Text(s) => s,
        _ => String::new(),
    }
}

pub(crate) fn int_val(v: turso::Value) -> i64 {
    match v {
        turso::Value::Integer(n) => n,
        _ => 0,
    }
}

#[derive(Clone)]
pub struct TursoStore {
    pub(crate) db: Database,
    pub(crate) conn: Arc<Mutex<Connection>>,
    pub(crate) device_id: i32,
}

impl TursoStore {
    pub async fn new<P: AsRef<Path>>(db_path: P) -> anyhow::Result<Self> {
        let db_path_str = db_path.as_ref().to_string_lossy().to_string();
        if let Some(parent) = Path::new(&db_path_str).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let db = Builder::new_local(&db_path_str).build().await?;
        let conn = db.connect()?;
        conn.execute_batch(schema::SCHEMA_SQL).await?;

        Ok(Self {
            db,
            conn: Arc::new(Mutex::new(conn)),
            device_id: 1,
        })
    }
}
