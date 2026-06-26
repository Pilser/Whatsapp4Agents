use async_trait::async_trait;
use bytes::Bytes;
use turso::params::Params;
use turso::Value;
use wacore::store::traits::SignalStore;

use super::{val_to_32byte_key, val_to_bytes, wrap_err, TursoStore};

#[async_trait]
impl SignalStore for TursoStore {
    async fn put_identity(&self, address: &str, key: [u8; 32]) -> wacore::store::error::Result<()> {
        let conn = self.conn.lock();
        crate::to_store_err!(conn.execute(
            "INSERT OR REPLACE INTO identities (address, key, device_id) VALUES (?1, ?2, ?3)",
            Params::Positional(vec![
                Value::from(address),
                Value::from(key.to_vec()),
                Value::from(self.device_id),
            ]),
        ).await.map(|_| ()))
    }

    async fn load_identity(&self, address: &str) -> wacore::store::error::Result<Option<[u8; 32]>> {
        let conn = self.conn.lock();
        let mut stmt = wrap_err(conn.prepare(
            "SELECT key FROM identities WHERE address = ?1 AND device_id = ?2"
        ).await)?;
        let result = stmt.query_row(Params::Positional(vec![
            Value::from(address),
            Value::from(self.device_id),
        ])).await;
        match result {
            Ok(row) => {
                let val = wrap_err(row.get_value(0))?;
                val_to_32byte_key(val).map(Some)
            }
            Err(turso::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(wrap_err(Err::<(), _>(e)).unwrap_err()),
        }
    }

    async fn delete_identity(&self, address: &str) -> wacore::store::error::Result<()> {
        let conn = self.conn.lock();
        crate::to_store_err!(conn.execute(
            "DELETE FROM identities WHERE address = ?1 AND device_id = ?2",
            Params::Positional(vec![Value::from(address), Value::from(self.device_id)]),
        ).await.map(|_| ()))
    }

    async fn get_session(&self, address: &str) -> wacore::store::error::Result<Option<Bytes>> {
        let conn = self.conn.lock();
        let mut stmt = wrap_err(conn.prepare(
            "SELECT record FROM sessions WHERE address = ?1 AND device_id = ?2"
        ).await)?;
        let result = stmt.query_row(Params::Positional(vec![
            Value::from(address),
            Value::from(self.device_id),
        ])).await;
        match result {
            Ok(row) => {
                let val = wrap_err(row.get_value(0))?;
                Ok(Some(Bytes::from(val_to_bytes(val))))
            }
            Err(turso::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(wrap_err(Err::<(), _>(e)).unwrap_err()),
        }
    }

    async fn put_session(&self, address: &str, session: &[u8]) -> wacore::store::error::Result<()> {
        let conn = self.conn.lock();
        crate::to_store_err!(conn.execute(
            "INSERT OR REPLACE INTO sessions (address, record, device_id) VALUES (?1, ?2, ?3)",
            Params::Positional(vec![
                Value::from(address),
                Value::from(session),
                Value::from(self.device_id),
            ]),
        ).await.map(|_| ()))
    }

    async fn delete_session(&self, address: &str) -> wacore::store::error::Result<()> {
        let conn = self.conn.lock();
        crate::to_store_err!(conn.execute(
            "DELETE FROM sessions WHERE address = ?1 AND device_id = ?2",
            Params::Positional(vec![Value::from(address), Value::from(self.device_id)]),
        ).await.map(|_| ()))
    }

    async fn store_prekey(
        &self,
        id: u32,
        record: &[u8],
        uploaded: bool,
    ) -> wacore::store::error::Result<()> {
        let conn = self.conn.lock();
        crate::to_store_err!(conn.execute(
            "INSERT OR REPLACE INTO prekeys (id, key, uploaded, device_id) VALUES (?1, ?2, ?3, ?4)",
            Params::Positional(vec![
                Value::from(id as i64),
                Value::from(record),
                Value::from(uploaded),
                Value::from(self.device_id),
            ]),
        ).await.map(|_| ()))
    }

    async fn load_prekey(&self, id: u32) -> wacore::store::error::Result<Option<Bytes>> {
        let conn = self.conn.lock();
        let mut stmt = wrap_err(conn.prepare(
            "SELECT key FROM prekeys WHERE id = ?1 AND device_id = ?2"
        ).await)?;
        let result = stmt.query_row(Params::Positional(vec![
            Value::from(id as i64),
            Value::from(self.device_id),
        ])).await;
        match result {
            Ok(row) => {
                let val = wrap_err(row.get_value(0))?;
                Ok(Some(Bytes::from(val_to_bytes(val))))
            }
            Err(turso::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(wrap_err(Err::<(), _>(e)).unwrap_err()),
        }
    }

    async fn get_max_prekey_id(&self) -> wacore::store::error::Result<u32> {
        let conn = self.conn.lock();
        let mut stmt = wrap_err(conn.prepare(
            "SELECT MAX(id) FROM prekeys WHERE device_id = ?1"
        ).await)?;
        let result = stmt.query_row(Params::Positional(vec![
            Value::from(self.device_id),
        ])).await;
        match result {
            Ok(row) => {
                let val = wrap_err(row.get_value(0))?;
                match val {
                    Value::Integer(n) => Ok(u32::try_from(n).unwrap_or(0)),
                    _ => Ok(0),
                }
            }
            Err(turso::Error::QueryReturnedNoRows) => Ok(0),
            Err(e) => Err(wrap_err(Err::<(), _>(e)).unwrap_err()),
        }
    }

    async fn remove_prekey(&self, id: u32) -> wacore::store::error::Result<()> {
        let conn = self.conn.lock();
        crate::to_store_err!(conn.execute(
            "DELETE FROM prekeys WHERE id = ?1 AND device_id = ?2",
            Params::Positional(vec![Value::from(id as i64), Value::from(self.device_id)]),
        ).await.map(|_| ()))
    }

    async fn store_signed_prekey(
        &self,
        id: u32,
        record: &[u8],
    ) -> wacore::store::error::Result<()> {
        let conn = self.conn.lock();
        crate::to_store_err!(conn.execute(
            "INSERT OR REPLACE INTO signed_prekeys (id, record, device_id) VALUES (?1, ?2, ?3)",
            Params::Positional(vec![
                Value::from(id as i64),
                Value::from(record),
                Value::from(self.device_id),
            ]),
        ).await.map(|_| ()))
    }

    async fn load_signed_prekey(&self, id: u32) -> wacore::store::error::Result<Option<Vec<u8>>> {
        let conn = self.conn.lock();
        let mut stmt = wrap_err(conn.prepare(
            "SELECT record FROM signed_prekeys WHERE id = ?1 AND device_id = ?2"
        ).await)?;
        let result = stmt.query_row(Params::Positional(vec![
            Value::from(id as i64),
            Value::from(self.device_id),
        ])).await;
        match result {
            Ok(row) => {
                let val = wrap_err(row.get_value(0))?;
                Ok(Some(val_to_bytes(val)))
            }
            Err(turso::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(wrap_err(Err::<(), _>(e)).unwrap_err()),
        }
    }

    async fn load_all_signed_prekeys(&self) -> wacore::store::error::Result<Vec<(u32, Vec<u8>)>> {
        let conn = self.conn.lock();
        let mut stmt = wrap_err(conn.prepare(
            "SELECT id, record FROM signed_prekeys WHERE device_id = ?1"
        ).await)?;
        let mut rows = wrap_err(stmt.query(Params::Positional(vec![
            Value::from(self.device_id),
        ])).await)?;
        let mut result = Vec::new();
        while let Some(row) = wrap_err(rows.next().await)? {
            let id_val = wrap_err(row.get_value(0))?;
            let rec_val = wrap_err(row.get_value(1))?;
            let id = match id_val {
                Value::Integer(n) => n as u32,
                _ => continue,
            };
            result.push((id, val_to_bytes(rec_val)));
        }
        Ok(result)
    }

    async fn mark_prekeys_uploaded(&self, ids: &[u32]) -> wacore::store::error::Result<()> {
        let conn = self.conn.lock();
        for &id in ids {
            crate::to_store_err!(conn.execute(
                "UPDATE prekeys SET uploaded = 1 WHERE id = ?1 AND device_id = ?2",
                Params::Positional(vec![
                    Value::from(id as i64),
                    Value::from(self.device_id),
                ]),
            ).await.map(|_| ()))?;
        }
        Ok(())
    }

    async fn remove_signed_prekey(&self, id: u32) -> wacore::store::error::Result<()> {
        let conn = self.conn.lock();
        crate::to_store_err!(conn.execute(
            "DELETE FROM signed_prekeys WHERE id = ?1 AND device_id = ?2",
            Params::Positional(vec![
                Value::from(id as i64),
                Value::from(self.device_id),
            ]),
        ).await.map(|_| ()))
    }

    async fn put_sender_key(
        &self,
        address: &str,
        record: &[u8],
    ) -> wacore::store::error::Result<()> {
        let conn = self.conn.lock();
        crate::to_store_err!(conn.execute(
            "INSERT OR REPLACE INTO sender_keys (address, record, device_id) VALUES (?1, ?2, ?3)",
            Params::Positional(vec![
                Value::from(address),
                Value::from(record),
                Value::from(self.device_id),
            ]),
        ).await.map(|_| ()))
    }

    async fn get_sender_key(&self, address: &str) -> wacore::store::error::Result<Option<Vec<u8>>> {
        let conn = self.conn.lock();
        let mut stmt = wrap_err(conn.prepare(
            "SELECT record FROM sender_keys WHERE address = ?1 AND device_id = ?2"
        ).await)?;
        let result = stmt.query_row(Params::Positional(vec![
            Value::from(address),
            Value::from(self.device_id),
        ])).await;
        match result {
            Ok(row) => {
                let val = wrap_err(row.get_value(0))?;
                Ok(Some(val_to_bytes(val)))
            }
            Err(turso::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(wrap_err(Err::<(), _>(e)).unwrap_err()),
        }
    }

    async fn delete_sender_key(&self, address: &str) -> wacore::store::error::Result<()> {
        let conn = self.conn.lock();
        crate::to_store_err!(conn.execute(
            "DELETE FROM sender_keys WHERE address = ?1 AND device_id = ?2",
            Params::Positional(vec![Value::from(address), Value::from(self.device_id)]),
        ).await.map(|_| ()))
    }
}


