use async_trait::async_trait;
use turso::params::Params;
use turso::Value;
use wacore::appstate::hash::HashState;
use wacore::appstate::processor::AppStateMutationMAC;
use wacore::store::traits::{AppStateSyncKey, AppSyncStore};

use super::{val_to_bytes, wrap_err, TursoStore};

#[async_trait]
impl AppSyncStore for TursoStore {
    async fn get_sync_key(
        &self,
        key_id: &[u8],
    ) -> wacore::store::error::Result<Option<AppStateSyncKey>> {
        let conn = self.conn.lock();
        let mut stmt = wrap_err(conn.prepare(
            "SELECT key_data FROM app_state_keys WHERE key_id = ?1 AND device_id = ?2"
        ).await)?;
        let result = stmt.query_row(Params::Positional(vec![
            Value::from(key_id),
            Value::from(self.device_id),
        ])).await;
        match result {
            Ok(row) => {
                let key_data = val_to_bytes(wrap_err(row.get_value(0))?);
                wrap_err(serde_json::from_slice(&key_data))
            }
            Err(turso::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(wrap_err(Err::<(), _>(e)).unwrap_err()),
        }
    }

    async fn set_sync_key(
        &self,
        key_id: &[u8],
        key: AppStateSyncKey,
    ) -> wacore::store::error::Result<()> {
        let conn = self.conn.lock();
        let key_data = wrap_err(serde_json::to_vec(&key))?;
        crate::to_store_err!(conn.execute(
            "INSERT OR REPLACE INTO app_state_keys (key_id, key_data, device_id) VALUES (?1, ?2, ?3)",
            Params::Positional(vec![
                Value::from(key_id),
                Value::from(key_data),
                Value::from(self.device_id),
            ]),
        ).await.map(|_| ()))
    }

    async fn get_version(&self, name: &str) -> wacore::store::error::Result<HashState> {
        let conn = self.conn.lock();
        let mut stmt = wrap_err(conn.prepare(
            "SELECT state_data FROM app_state_versions WHERE name = ?1 AND device_id = ?2"
        ).await)?;
        let result = stmt.query_row(Params::Positional(vec![
            Value::from(name),
            Value::from(self.device_id),
        ])).await;
        match result {
            Ok(row) => {
                let state_data = val_to_bytes(wrap_err(row.get_value(0))?);
                wrap_err(serde_json::from_slice(&state_data))
            }
            Err(turso::Error::QueryReturnedNoRows) => Ok(HashState::default()),
            Err(e) => Err(wrap_err(Err::<(), _>(e)).unwrap_err()),
        }
    }

    async fn set_version(&self, name: &str, state: HashState) -> wacore::store::error::Result<()> {
        let conn = self.conn.lock();
        let state_data = wrap_err(serde_json::to_vec(&state))?;
        crate::to_store_err!(conn.execute(
            "INSERT OR REPLACE INTO app_state_versions (name, state_data, device_id) VALUES (?1, ?2, ?3)",
            Params::Positional(vec![
                Value::from(name),
                Value::from(state_data),
                Value::from(self.device_id),
            ]),
        ).await.map(|_| ()))
    }

    async fn put_mutation_macs(
        &self,
        name: &str,
        version: u64,
        mutations: &[AppStateMutationMAC],
    ) -> wacore::store::error::Result<()> {
        let conn = self.conn.lock();
        for mutation in mutations {
            crate::to_store_err!(conn.execute(
                "INSERT OR REPLACE INTO app_state_mutation_macs (name, version, index_mac, value_mac, device_id) VALUES (?1, ?2, ?3, ?4, ?5)",
                Params::Positional(vec![
                    Value::from(name),
                    Value::from(version as i64),
                    Value::from(mutation.index_mac.clone()),
                    Value::from(mutation.value_mac.clone()),
                    Value::from(self.device_id),
                ]),
            ).await.map(|_| ()))?;
        }
        Ok(())
    }

    async fn get_mutation_mac(
        &self,
        name: &str,
        index_mac: &[u8],
    ) -> wacore::store::error::Result<Option<Vec<u8>>> {
        let conn = self.conn.lock();
        let mut stmt = wrap_err(conn.prepare(
            "SELECT value_mac FROM app_state_mutation_macs WHERE name = ?1 AND index_mac = ?2 AND device_id = ?3"
        ).await)?;
        let result = stmt.query_row(Params::Positional(vec![
            Value::from(name),
            Value::from(index_mac),
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

    async fn clear_mutation_macs(&self, name: &str) -> wacore::store::error::Result<()> {
        let conn = self.conn.lock();
        crate::to_store_err!(conn.execute(
            "DELETE FROM app_state_mutation_macs WHERE name = ?1 AND device_id = ?2",
            Params::Positional(vec![
                Value::from(name),
                Value::from(self.device_id),
            ]),
        ).await.map(|_| ()))
    }

    async fn delete_mutation_macs(
        &self,
        name: &str,
        index_macs: &[Vec<u8>],
    ) -> wacore::store::error::Result<()> {
        let conn = self.conn.lock();
        for index_mac in index_macs {
            crate::to_store_err!(conn.execute(
                "DELETE FROM app_state_mutation_macs WHERE name = ?1 AND index_mac = ?2 AND device_id = ?3",
                Params::Positional(vec![
                    Value::from(name),
                    Value::from(index_mac.clone()),
                    Value::from(self.device_id),
                ]),
            ).await.map(|_| ()))?;
        }
        Ok(())
    }

    async fn get_latest_sync_key_id(&self) -> wacore::store::error::Result<Option<Vec<u8>>> {
        let conn = self.conn.lock();
        let mut stmt = wrap_err(conn.prepare(
            "SELECT key_id FROM app_state_keys WHERE device_id = ?1 ORDER BY key_id DESC LIMIT 1"
        ).await)?;
        let result = stmt.query_row(Params::Positional(vec![
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
}
