use async_trait::async_trait;
use turso::params::Params;
use turso::Value;
use wacore::store::traits::*;

use super::{wrap_err, TursoStore};

#[async_trait]
impl ProtocolStore for TursoStore {
    async fn get_sender_key_devices(
        &self,
        group_jid: &str,
    ) -> wacore::store::error::Result<Vec<(String, bool)>> {
        let conn = self.conn.lock();
        let mut stmt = wrap_err(conn.prepare(
            "SELECT device_jid, has_key FROM sender_key_devices WHERE group_jid = ?1 AND device_id = ?2"
        ).await)?;
        let mut rows = wrap_err(stmt.query(Params::Positional(vec![
            Value::from(group_jid),
            Value::from(self.device_id),
        ])).await)?;
        let mut result = Vec::new();
        while let Some(row) = wrap_err(rows.next().await)? {
            let jid = match wrap_err(row.get_value(0))? { Value::Text(s) => s, _ => continue };
            let has_key = match wrap_err(row.get_value(1))? { Value::Integer(n) => n != 0, _ => false };
            result.push((jid, has_key));
        }
        Ok(result)
    }

    async fn set_sender_key_status(
        &self,
        group_jid: &str,
        entries: &[(&str, bool)],
    ) -> wacore::store::error::Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let conn = self.conn.lock();
        let now = chrono::Utc::now().timestamp();
        for (device_jid, has_key) in entries {
            crate::to_store_err!(conn.execute(
                "INSERT INTO sender_key_devices (group_jid, device_jid, has_key, device_id, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(group_jid, device_jid, device_id) DO UPDATE SET
                   has_key = excluded.has_key,
                   updated_at = excluded.updated_at",
                Params::Positional(vec![
                    Value::from(group_jid),
                    Value::from(*device_jid),
                    Value::from(*has_key),
                    Value::from(self.device_id),
                    Value::from(now),
                ]),
            ).await.map(|_| ()))?;
        }
        Ok(())
    }

    async fn clear_sender_key_devices(&self, group_jid: &str) -> wacore::store::error::Result<()> {
        let conn = self.conn.lock();
        crate::to_store_err!(conn.execute(
            "DELETE FROM sender_key_devices WHERE group_jid = ?1 AND device_id = ?2",
            Params::Positional(vec![Value::from(group_jid), Value::from(self.device_id)]),
        ).await.map(|_| ()))
    }

    async fn delete_sender_key_device_rows(
        &self,
        device_jids: &[&str],
    ) -> wacore::store::error::Result<()> {
        if device_jids.is_empty() { return Ok(()); }
        let conn = self.conn.lock();
        for device_jid in device_jids {
            crate::to_store_err!(conn.execute(
                "DELETE FROM sender_key_devices WHERE device_jid = ?1 AND device_id = ?2",
                Params::Positional(vec![Value::from(*device_jid), Value::from(self.device_id)]),
            ).await.map(|_| ()))?;
        }
        Ok(())
    }

    async fn clear_all_sender_key_devices(&self) -> wacore::store::error::Result<()> {
        let conn = self.conn.lock();
        crate::to_store_err!(conn.execute(
            "DELETE FROM sender_key_devices WHERE device_id = ?1",
            Params::Positional(vec![Value::from(self.device_id)]),
        ).await.map(|_| ()))
    }

    async fn get_lid_mapping(
        &self, lid: &str,
    ) -> wacore::store::error::Result<Option<LidPnMappingEntry>> {
        let conn = self.conn.lock();
        let mut stmt = wrap_err(conn.prepare(
            "SELECT lid, phone_number, created_at, learning_source, updated_at
             FROM lid_pn_mapping WHERE lid = ?1 AND device_id = ?2"
        ).await)?;
        let result = stmt.query_row(Params::Positional(vec![
            Value::from(lid), Value::from(self.device_id),
        ])).await;
        map_row_lid(result)
    }

    async fn get_pn_mapping(
        &self, phone: &str,
    ) -> wacore::store::error::Result<Option<LidPnMappingEntry>> {
        let conn = self.conn.lock();
        let mut stmt = wrap_err(conn.prepare(
            "SELECT lid, phone_number, created_at, learning_source, updated_at
             FROM lid_pn_mapping WHERE phone_number = ?1 AND device_id = ?2
             ORDER BY updated_at DESC LIMIT 1"
        ).await)?;
        let result = stmt.query_row(Params::Positional(vec![
            Value::from(phone), Value::from(self.device_id),
        ])).await;
        map_row_lid(result)
    }

    async fn put_lid_mapping(&self, entry: &LidPnMappingEntry) -> wacore::store::error::Result<()> {
        let conn = self.conn.lock();
        crate::to_store_err!(conn.execute(
            "INSERT OR REPLACE INTO lid_pn_mapping (lid, phone_number, created_at, learning_source, updated_at, device_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            Params::Positional(vec![
                Value::from(entry.lid.as_str()), Value::from(entry.phone_number.as_str()),
                Value::from(entry.created_at), Value::from(entry.learning_source.as_str()),
                Value::from(entry.updated_at), Value::from(self.device_id),
            ]),
        ).await.map(|_| ()))
    }

    async fn get_all_lid_mappings(&self) -> wacore::store::error::Result<Vec<LidPnMappingEntry>> {
        let conn = self.conn.lock();
        let mut stmt = wrap_err(conn.prepare(
            "SELECT lid, phone_number, created_at, learning_source, updated_at
             FROM lid_pn_mapping WHERE device_id = ?1"
        ).await)?;
        let mut rows = wrap_err(stmt.query(Params::Positional(vec![
            Value::from(self.device_id),
        ])).await)?;
        let mut result = Vec::new();
        while let Some(row) = wrap_err(rows.next().await)? {
            if let Ok(entry) = row_to_lid(&row) {
                result.push(entry);
            }
        }
        Ok(result)
    }

    async fn save_base_key(
        &self, address: &str, message_id: &str, base_key: &[u8],
    ) -> wacore::store::error::Result<()> {
        let conn = self.conn.lock();
        let now = chrono::Utc::now().timestamp();
        crate::to_store_err!(conn.execute(
            "INSERT OR REPLACE INTO base_keys (address, message_id, base_key, device_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            Params::Positional(vec![
                Value::from(address), Value::from(message_id),
                Value::from(base_key), Value::from(self.device_id), Value::from(now),
            ]),
        ).await.map(|_| ()))
    }

    async fn has_same_base_key(
        &self, address: &str, message_id: &str, current_base_key: &[u8],
    ) -> wacore::store::error::Result<bool> {
        let conn = self.conn.lock();
        let mut stmt = wrap_err(conn.prepare(
            "SELECT base_key FROM base_keys WHERE address = ?1 AND message_id = ?2 AND device_id = ?3"
        ).await)?;
        let result = stmt.query_row(Params::Positional(vec![
            Value::from(address), Value::from(message_id), Value::from(self.device_id),
        ])).await;
        match result {
            Ok(row) => {
                let val = wrap_err(row.get_value(0))?;
                match val { Value::Blob(b) => Ok(b == current_base_key), _ => Ok(false) }
            }
            Err(turso::Error::QueryReturnedNoRows) => Ok(false),
            Err(e) => Err(wrap_err(Err::<(), _>(e)).unwrap_err()),
        }
    }

    async fn delete_base_key(
        &self, address: &str, message_id: &str,
    ) -> wacore::store::error::Result<()> {
        let conn = self.conn.lock();
        crate::to_store_err!(conn.execute(
            "DELETE FROM base_keys WHERE address = ?1 AND message_id = ?2 AND device_id = ?3",
            Params::Positional(vec![
                Value::from(address), Value::from(message_id), Value::from(self.device_id),
            ]),
        ).await.map(|_| ()))
    }

    async fn update_device_list(&self, record: DeviceListRecord) -> wacore::store::error::Result<()> {
        let conn = self.conn.lock();
        let devices_json = wrap_err(serde_json::to_string(&record.devices))?;
        let now = chrono::Utc::now().timestamp();
        crate::to_store_err!(conn.execute(
            "INSERT OR REPLACE INTO device_registry
             (user_id, devices_json, timestamp, phash, raw_id, device_id, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            Params::Positional(vec![
                Value::from(record.user.as_str()), Value::from(devices_json),
                Value::from(record.timestamp),
                record.phash.map(Value::from).unwrap_or(Value::Null),
                record.raw_id.map(|r| Value::from(r as i64)).unwrap_or(Value::Null),
                Value::from(self.device_id), Value::from(now),
            ]),
        ).await.map(|_| ()))
    }

    async fn get_devices(&self, user: &str) -> wacore::store::error::Result<Option<DeviceListRecord>> {
        let conn = self.conn.lock();
        let mut stmt = wrap_err(conn.prepare(
            "SELECT user_id, devices_json, timestamp, phash, raw_id
             FROM device_registry WHERE user_id = ?1 AND device_id = ?2"
        ).await)?;
        let result = stmt.query_row(Params::Positional(vec![
            Value::from(user), Value::from(self.device_id),
        ])).await;
        match result {
            Ok(row) => {
                let user_id = match wrap_err(row.get_value(0))? { Value::Text(s) => s, _ => return Err(wacore::store::error::StoreError::Validation("expected text".into())) };
                let devices_json = match wrap_err(row.get_value(1))? { Value::Text(s) => s, _ => return Err(wacore::store::error::StoreError::Validation("expected text".into())) };
                let timestamp = match wrap_err(row.get_value(2))? { Value::Integer(n) => n as i64, _ => 0 };
                let phash = match wrap_err(row.get_value(3))? { Value::Text(s) => Some(s), _ => None };
                let raw_id = match wrap_err(row.get_value(4))? { Value::Integer(n) => Some(n as u32), _ => None };
                let devices: Vec<DeviceInfo> = wrap_err(serde_json::from_str(&devices_json))?;
                Ok(Some(DeviceListRecord { user: user_id, devices, timestamp, phash, raw_id }))
            }
            Err(turso::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(wrap_err(Err::<(), _>(e)).unwrap_err()),
        }
    }

    async fn delete_devices(&self, user: &str) -> wacore::store::error::Result<()> {
        let conn = self.conn.lock();
        crate::to_store_err!(conn.execute(
            "DELETE FROM device_registry WHERE user_id = ?1 AND device_id = ?2",
            Params::Positional(vec![Value::from(user), Value::from(self.device_id)]),
        ).await.map(|_| ()))
    }

    async fn get_tc_token(&self, jid: &str) -> wacore::store::error::Result<Option<TcTokenEntry>> {
        let conn = self.conn.lock();
        let mut stmt = wrap_err(conn.prepare(
            "SELECT token, token_timestamp, sender_timestamp FROM tc_tokens
             WHERE jid = ?1 AND device_id = ?2"
        ).await)?;
        let result = stmt.query_row(Params::Positional(vec![
            Value::from(jid), Value::from(self.device_id),
        ])).await;
        match result {
            Ok(row) => {
                let token = match wrap_err(row.get_value(0))? { Value::Blob(b) => b, _ => return Err(wacore::store::error::StoreError::Validation("expected blob".into())) };
                let token_ts = match wrap_err(row.get_value(1))? { Value::Integer(n) => n as i64, _ => 0 };
                let sender_ts = match wrap_err(row.get_value(2))? { Value::Integer(n) => Some(n as i64), _ => None };
                Ok(Some(TcTokenEntry { token, token_timestamp: token_ts, sender_timestamp: sender_ts }))
            }
            Err(turso::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(wrap_err(Err::<(), _>(e)).unwrap_err()),
        }
    }

    async fn put_tc_token(&self, jid: &str, entry: &TcTokenEntry) -> wacore::store::error::Result<()> {
        let conn = self.conn.lock();
        let now = chrono::Utc::now().timestamp();
        crate::to_store_err!(conn.execute(
            "INSERT OR REPLACE INTO tc_tokens
             (jid, token, token_timestamp, sender_timestamp, device_id, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            Params::Positional(vec![
                Value::from(jid), Value::from(entry.token.clone()),
                Value::from(entry.token_timestamp),
                entry.sender_timestamp.map(Value::from).unwrap_or(Value::Null),
                Value::from(self.device_id), Value::from(now),
            ]),
        ).await.map(|_| ()))
    }

    async fn delete_tc_token(&self, jid: &str) -> wacore::store::error::Result<()> {
        let conn = self.conn.lock();
        crate::to_store_err!(conn.execute(
            "DELETE FROM tc_tokens WHERE jid = ?1 AND device_id = ?2",
            Params::Positional(vec![Value::from(jid), Value::from(self.device_id)]),
        ).await.map(|_| ()))
    }

    async fn get_all_tc_token_jids(&self) -> wacore::store::error::Result<Vec<String>> {
        let conn = self.conn.lock();
        let mut stmt = wrap_err(conn.prepare(
            "SELECT jid FROM tc_tokens WHERE device_id = ?1"
        ).await)?;
        let mut rows = wrap_err(stmt.query(Params::Positional(vec![
            Value::from(self.device_id),
        ])).await)?;
        let mut result = Vec::new();
        while let Some(row) = wrap_err(rows.next().await)? {
            if let Value::Text(jid) = wrap_err(row.get_value(0))? {
                result.push(jid);
            }
        }
        Ok(result)
    }

    async fn delete_expired_tc_tokens(&self, cutoff_timestamp: i64) -> wacore::store::error::Result<u32> {
        let conn = self.conn.lock();
        let deleted = conn.execute(
            "DELETE FROM tc_tokens WHERE token_timestamp < ?1 AND device_id = ?2",
            Params::Positional(vec![Value::from(cutoff_timestamp), Value::from(self.device_id)]),
        ).await.map_err(|e| wacore::store::error::StoreError::Database(Box::new(e)))?;
        u32::try_from(deleted).map_err(|_| wacore::store::error::StoreError::Validation("count overflow".into()))
    }

    async fn store_sent_message(
        &self, chat_jid: &str, message_id: &str, payload: &[u8],
    ) -> wacore::store::error::Result<()> {
        let conn = self.conn.lock();
        let now = chrono::Utc::now().timestamp();
        crate::to_store_err!(conn.execute(
            "INSERT OR REPLACE INTO sent_messages (chat_jid, message_id, payload, device_id, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            Params::Positional(vec![
                Value::from(chat_jid), Value::from(message_id),
                Value::from(payload), Value::from(self.device_id), Value::from(now),
            ]),
        ).await.map(|_| ()))
    }

    async fn take_sent_message(
        &self, chat_jid: &str, message_id: &str,
    ) -> wacore::store::error::Result<Option<Vec<u8>>> {
        let conn = self.conn.lock();
        conn.execute_batch("BEGIN").await.map_err(|e| wacore::store::error::StoreError::Database(Box::new(e)))?;
        let mut stmt = wrap_err(conn.prepare(
            "SELECT payload FROM sent_messages WHERE chat_jid = ?1 AND message_id = ?2 AND device_id = ?3"
        ).await)?;
        let result = stmt.query_row(Params::Positional(vec![
            Value::from(chat_jid), Value::from(message_id), Value::from(self.device_id),
        ])).await;
        match result {
            Ok(row) => {
                let payload = match wrap_err(row.get_value(0))? { Value::Blob(b) => b, _ => return Err(wacore::store::error::StoreError::Validation("expected blob".into())) };
                conn.execute("DELETE FROM sent_messages WHERE chat_jid = ?1 AND message_id = ?2 AND device_id = ?3",
                    Params::Positional(vec![Value::from(chat_jid), Value::from(message_id), Value::from(self.device_id)]),
                ).await.map_err(|e| wacore::store::error::StoreError::Database(Box::new(e)))?;
                conn.execute_batch("COMMIT").await.map_err(|e| wacore::store::error::StoreError::Database(Box::new(e)))?;
                Ok(Some(payload))
            }
            Err(turso::Error::QueryReturnedNoRows) => {
                conn.execute_batch("COMMIT").await.map_err(|e| wacore::store::error::StoreError::Database(Box::new(e)))?;
                Ok(None)
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK").await;
                Err(wacore::store::error::StoreError::Database(Box::new(e)))
            }
        }
    }

    async fn delete_expired_sent_messages(&self, cutoff_timestamp: i64) -> wacore::store::error::Result<u32> {
        let conn = self.conn.lock();
        let deleted = conn.execute(
            "DELETE FROM sent_messages WHERE created_at < ?1 AND device_id = ?2",
            Params::Positional(vec![Value::from(cutoff_timestamp), Value::from(self.device_id)]),
        ).await.map_err(|e| wacore::store::error::StoreError::Database(Box::new(e)))?;
        u32::try_from(deleted).map_err(|_| wacore::store::error::StoreError::Validation("count overflow".into()))
    }
}

fn map_row_lid(result: std::result::Result<turso::Row, turso::Error>) -> wacore::store::error::Result<Option<LidPnMappingEntry>> {
    match result {
        Ok(row) => row_to_lid(&row).map(Some),
        Err(turso::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(wacore::store::error::StoreError::Database(Box::new(e))),
    }
}

fn row_to_lid(row: &turso::Row) -> wacore::store::error::Result<LidPnMappingEntry> {
    let lid = match row.get_value(0).map_err(|e| wacore::store::error::StoreError::Database(Box::new(e)))? { Value::Text(s) => s, _ => return Err(wacore::store::error::StoreError::Validation("expected text".into())) };
    let phone = match row.get_value(1).map_err(|e| wacore::store::error::StoreError::Database(Box::new(e)))? { Value::Text(s) => s, _ => return Err(wacore::store::error::StoreError::Validation("expected text".into())) };
    let created = match row.get_value(2).map_err(|e| wacore::store::error::StoreError::Database(Box::new(e)))? { Value::Integer(n) => n as i64, _ => 0 };
    let source = match row.get_value(3).map_err(|e| wacore::store::error::StoreError::Database(Box::new(e)))? { Value::Text(s) => s, _ => String::new() };
    let updated = match row.get_value(4).map_err(|e| wacore::store::error::StoreError::Database(Box::new(e)))? { Value::Integer(n) => n as i64, _ => 0 };
    Ok(LidPnMappingEntry { lid, phone_number: phone, created_at: created, learning_source: source, updated_at: updated })
}
