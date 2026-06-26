use async_trait::async_trait;
use turso::params::Params;
use turso::Value;
use wacore::store::traits::{MsgSecretEntry, MsgSecretStore};

use super::{wrap_err, TursoStore};

#[async_trait]
impl MsgSecretStore for TursoStore {
    async fn put_msg_secrets(
        &self,
        entries: Vec<MsgSecretEntry>,
    ) -> wacore::store::error::Result<usize> {
        let conn = self.conn.lock();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        for entry in &entries {
            crate::to_store_err!(conn.execute(
                "INSERT OR REPLACE INTO msg_secrets (chat, sender, msg_id, secret, device_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                Params::Positional(vec![
                    Value::from(entry.chat.as_str()),
                    Value::from(entry.sender.as_str()),
                    Value::from(entry.msg_id.as_str()),
                    Value::from(entry.secret.clone()),
                    Value::from(self.device_id),
                    Value::from(now),
                ]),
            ).await.map(|_| ()))?;
        }
        Ok(entries.len())
    }

    async fn get_msg_secret(
        &self,
        chat: &str,
        sender: &str,
        msg_id: &str,
    ) -> wacore::store::error::Result<Option<Vec<u8>>> {
        let conn = self.conn.lock();
        let mut stmt = wrap_err(conn.prepare(
            "SELECT secret FROM msg_secrets WHERE chat = ?1 AND sender = ?2 AND msg_id = ?3 AND device_id = ?4"
        ).await)?;
        let result = stmt.query_row(Params::Positional(vec![
            Value::from(chat),
            Value::from(sender),
            Value::from(msg_id),
            Value::from(self.device_id),
        ])).await;
        match result {
            Ok(row) => {
                let secret = match wrap_err(row.get_value(0))? {
                    Value::Blob(b) => b,
                    _ => return Err(wacore::store::error::StoreError::Validation("expected blob".into())),
                };
                Ok(Some(secret))
            }
            Err(turso::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(wacore::store::error::StoreError::Database(Box::new(e))),
        }
    }

    async fn delete_expired_msg_secrets(
        &self,
        cutoff_timestamp: i64,
    ) -> wacore::store::error::Result<u32> {
        let conn = self.conn.lock();
        let count = crate::to_store_err!(conn.execute(
            "DELETE FROM msg_secrets WHERE created_at < ?1 AND device_id = ?2",
            Params::Positional(vec![
                Value::from(cutoff_timestamp),
                Value::from(self.device_id),
            ]),
        ).await)?;
        Ok(count as u32)
    }
}
