use std::sync::Arc;

use async_trait::async_trait;
use buffa::Message;
use turso::params::Params;
use turso::Value;
use wacore::libsignal::protocol::{KeyPair, PrivateKey, PublicKey};
use wacore::store::Device as CoreDevice;
use wacore::store::traits::DeviceStore as DeviceStoreTrait;

use super::{wrap_err, TursoStore};

#[async_trait]
impl DeviceStoreTrait for TursoStore {
    async fn save(&self, device: &CoreDevice) -> wacore::store::error::Result<()> {
        let conn = self.conn.lock();

        let noise_key = {
            let mut bytes = Vec::new();
            bytes.extend_from_slice(device.noise_key.private_key.serialize().as_slice());
            bytes.extend_from_slice(device.noise_key.public_key.public_key_bytes());
            bytes
        };
        let identity_key = {
            let mut bytes = Vec::new();
            bytes.extend_from_slice(device.identity_key.private_key.serialize().as_slice());
            bytes.extend_from_slice(device.identity_key.public_key.public_key_bytes());
            bytes
        };
        let signed_pre_key = {
            let mut bytes = Vec::new();
            bytes.extend_from_slice(device.signed_pre_key.private_key.serialize().as_slice());
            bytes.extend_from_slice(device.signed_pre_key.public_key.public_key_bytes());
            bytes
        };

        let account = device.account.as_ref().map(|a| a.encode_to_vec());
        let server_cert_chain = device.server_cert_chain.as_ref().map(|c| wrap_err(serde_json::to_vec(c))).transpose()?;

        crate::to_store_err!(conn.execute(
            "INSERT OR REPLACE INTO device (
                id, lid, pn, registration_id, noise_key, identity_key,
                signed_pre_key, signed_pre_key_id, signed_pre_key_signature,
                adv_secret_key, account, push_name, app_version_primary,
                app_version_secondary, app_version_tertiary, app_version_last_fetched_ms,
                edge_routing_info, props_hash,
                next_pre_key_id, server_has_prekeys, nct_salt,
                server_cert_chain, login_counter
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
                ?19, ?20, ?21, ?22, ?23)",
            Params::Positional(vec![
                Value::from(1i64),
                device.lid.as_ref().map(|j| Value::from(j.to_string())).unwrap_or(Value::Null),
                device.pn.as_ref().map(|j| Value::from(j.to_string())).unwrap_or(Value::Null),
                Value::from(device.registration_id as i64),
                Value::from(noise_key),
                Value::from(identity_key),
                Value::from(signed_pre_key),
                Value::from(device.signed_pre_key_id as i64),
                Value::from(device.signed_pre_key_signature.to_vec()),
                Value::from(device.adv_secret_key.to_vec()),
                account.map(Value::from).unwrap_or(Value::Null),
                Value::from(device.push_name.as_str()),
                Value::from(device.app_version_primary as i64),
                Value::from(device.app_version_secondary as i64),
                Value::from(device.app_version_tertiary as i64),
                Value::from(device.app_version_last_fetched_ms),
                device.edge_routing_info.as_ref().map(|b| Value::from(b.as_slice())).unwrap_or(Value::Null),
                device.props_hash.as_ref().map(|s| Value::from(s.as_str())).unwrap_or(Value::Null),
                Value::from(device.next_pre_key_id as i64),
                Value::from(device.server_has_prekeys),
                device.nct_salt.as_ref().map(|b| Value::from(b.as_slice())).unwrap_or(Value::Null),
                server_cert_chain.map(Value::from).unwrap_or(Value::Null),
                Value::from(device.login_counter as i64),
            ]),
        ).await.map(|_| ()))
    }

    async fn load(&self) -> wacore::store::error::Result<Option<CoreDevice>> {
        let conn = self.conn.lock();
        let mut stmt = wrap_err(conn.prepare(
            "SELECT * FROM device WHERE id = 1"
        ).await)?;
        let result = stmt.query_row(()).await;
        match result {
            Ok(row) => {
                let n = row.column_count();
                let mut vals = Vec::with_capacity(n);
                for i in 0..n {
                    vals.push(wrap_err(row.get_value(i))?);
                }
                if vals.is_empty() {
                    return Ok(None);
                }
                let v = |idx: usize| -> Option<&Value> { vals.get(idx) };

                let noise_key_bytes = match v(4) { Some(Value::Blob(b)) => b, _ => return Err(wacore::store::error::StoreError::Validation("missing noise_key".into())) };
                let identity_key_bytes = match v(5) { Some(Value::Blob(b)) => b, _ => return Err(wacore::store::error::StoreError::Validation("missing identity_key".into())) };
                let signed_pre_key_bytes = match v(6) { Some(Value::Blob(b)) => b, _ => return Err(wacore::store::error::StoreError::Validation("missing signed_pre_key".into())) };

                if noise_key_bytes.len() != 64 || identity_key_bytes.len() != 64 || signed_pre_key_bytes.len() != 64 {
                    return Err(wacore::store::error::StoreError::Validation("key pair length mismatch".into()));
                }

                let noise_key = KeyPair::new(
                    PublicKey::from_djb_public_key_bytes(&noise_key_bytes[32..64])
                        .map_err(|e| wacore::store::error::StoreError::Database(Box::new(e)))?,
                    PrivateKey::deserialize(&noise_key_bytes[0..32])
                        .map_err(|e| wacore::store::error::StoreError::Database(Box::new(e)))?,
                );
                let identity_key = KeyPair::new(
                    PublicKey::from_djb_public_key_bytes(&identity_key_bytes[32..64])
                        .map_err(|e| wacore::store::error::StoreError::Database(Box::new(e)))?,
                    PrivateKey::deserialize(&identity_key_bytes[0..32])
                        .map_err(|e| wacore::store::error::StoreError::Database(Box::new(e)))?,
                );
                let signed_pre_key = KeyPair::new(
                    PublicKey::from_djb_public_key_bytes(&signed_pre_key_bytes[32..64])
                        .map_err(|e| wacore::store::error::StoreError::Database(Box::new(e)))?,
                    PrivateKey::deserialize(&signed_pre_key_bytes[0..32])
                        .map_err(|e| wacore::store::error::StoreError::Database(Box::new(e)))?,
                );

                let lid_str = match v(1) { Some(Value::Text(s)) => Some(s.clone()), _ => None };
                let pn_str = match v(2) { Some(Value::Text(s)) => Some(s.clone()), _ => None };
                let registration_id = match v(3) { Some(Value::Integer(n)) => *n as u32, _ => return Err(wacore::store::error::StoreError::Validation("missing registration_id".into())) };
                let signed_pre_key_id = match v(7) { Some(Value::Integer(n)) => *n as u32, _ => return Err(wacore::store::error::StoreError::Validation("missing signed_pre_key_id".into())) };
                let signature_bytes = match v(8) { Some(Value::Blob(b)) => b.clone(), _ => return Err(wacore::store::error::StoreError::Validation("missing signature".into())) };
                let adv_secret_bytes = match v(9) { Some(Value::Blob(b)) => b.clone(), _ => return Err(wacore::store::error::StoreError::Validation("missing adv_secret".into())) };
                let account_bytes = match v(10) { Some(Value::Blob(b)) => Some(b.clone()), _ => None };
                let push_name = match v(11) { Some(Value::Text(s)) => s.clone(), _ => return Err(wacore::store::error::StoreError::Validation("missing push_name".into())) };
                let app_version_primary = match v(12) { Some(Value::Integer(n)) => *n as u32, _ => return Err(wacore::store::error::StoreError::Validation("missing app_version_primary".into())) };
                let app_version_secondary = match v(13) { Some(Value::Integer(n)) => *n as u32, _ => 0 };
                let app_version_tertiary = match v(14) { Some(Value::Integer(n)) => *n as u32, _ => 0 };
                let app_version_last_fetched_ms = match v(15) { Some(Value::Integer(n)) => *n, _ => 0 };
                let edge_routing_info = match v(16) { Some(Value::Blob(b)) => Some(b.clone()), _ => None };
                let props_hash = match v(17) { Some(Value::Text(s)) => Some(s.clone()), _ => None };
                let next_pre_key_id = match v(18) { Some(Value::Integer(n)) => *n as u32, _ => 0 };
                let server_has_prekeys = match v(19) { Some(Value::Integer(n)) => *n != 0, _ => false };
                let nct_salt = match v(20) { Some(Value::Blob(b)) => Some(b.clone()), _ => None };
                let server_cert_chain = match v(21) { Some(Value::Blob(b)) => Some(wrap_err(serde_json::from_slice(&b))?), _ => None };
                let login_counter = match v(22) { Some(Value::Integer(n)) => *n as i32, _ => 0 };

                let mut signature = [0u8; 64];
                let mut adv_secret = [0u8; 32];
                signature.copy_from_slice(&signature_bytes);
                adv_secret.copy_from_slice(&adv_secret_bytes);

                let account = account_bytes
                    .map(|b| waproto::whatsapp::ADVSignedDeviceIdentity::decode_from_slice(&b))
                    .transpose()
                    .map_err(|e| wacore::store::error::StoreError::Serialization(Box::new(e)))?;

                Ok(Some(CoreDevice {
                    pn: pn_str.and_then(|s| s.parse().ok()),
                    lid: lid_str.and_then(|s| s.parse().ok()),
                    registration_id,
                    noise_key,
                    identity_key,
                    signed_pre_key,
                    signed_pre_key_id,
                    signed_pre_key_signature: signature,
                    adv_secret_key: adv_secret,
                    account: account.map(Arc::new),
                    push_name,
                    app_version_primary,
                    app_version_secondary,
                    app_version_tertiary,
                    app_version_last_fetched_ms,
                    edge_routing_info,
                    props_hash,
                    next_pre_key_id,
                    server_has_prekeys,
                    nct_salt,
                    server_cert_chain,
                    ..Default::default()
                }))
            }
            Err(turso::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(wacore::store::error::StoreError::Database(Box::new(e))),
        }
    }

    async fn exists(&self) -> wacore::store::error::Result<bool> {
        let conn = self.conn.lock();
        let mut stmt = wrap_err(conn.prepare(
            "SELECT COUNT(*) FROM device WHERE id = 1"
        ).await)?;
        let result = stmt.query_row(()).await;
        match result {
            Ok(row) => {
                let count = match wrap_err(row.get_value(0))? { Value::Integer(n) => n, _ => 0 };
                Ok(count > 0)
            }
            Err(turso::Error::QueryReturnedNoRows) => Ok(false),
            Err(e) => Err(wacore::store::error::StoreError::Database(Box::new(e))),
        }
    }

    async fn create(&self) -> wacore::store::error::Result<i32> {
        use wacore::libsignal::protocol::KeyPair;
        use wacore::store::Device as CoreDevice;

        let new_device = CoreDevice::new();
        let serialize_kp = |kp: &KeyPair| -> Vec<u8> {
            let mut bytes = Vec::new();
            bytes.extend_from_slice(kp.private_key.serialize().as_slice());
            bytes.extend_from_slice(kp.public_key.public_key_bytes());
            bytes
        };

        let noise_key = serialize_kp(&new_device.noise_key);
        let identity_key = serialize_kp(&new_device.identity_key);
        let signed_pre_key = serialize_kp(&new_device.signed_pre_key);

        let conn = self.conn.lock();
        crate::to_store_err!(conn.execute(
            "INSERT INTO device (
                id, lid, pn, registration_id, noise_key, identity_key,
                signed_pre_key, signed_pre_key_id, signed_pre_key_signature,
                adv_secret_key, account, push_name, app_version_primary,
                app_version_secondary, app_version_tertiary, app_version_last_fetched_ms,
                edge_routing_info, props_hash,
                next_pre_key_id, server_has_prekeys, nct_salt,
                server_cert_chain, login_counter
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
                ?19, ?20, ?21, ?22, ?23)",
            turso::params::Params::Positional(vec![
                turso::Value::from(self.device_id as i64),
                turso::Value::Null,
                turso::Value::Null,
                turso::Value::from(new_device.registration_id as i64),
                turso::Value::from(noise_key),
                turso::Value::from(identity_key),
                turso::Value::from(signed_pre_key),
                turso::Value::from(new_device.signed_pre_key_id as i64),
                turso::Value::from(new_device.signed_pre_key_signature.to_vec()),
                turso::Value::from(new_device.adv_secret_key.to_vec()),
                turso::Value::Null,
                turso::Value::from(new_device.push_name.as_str()),
                turso::Value::from(new_device.app_version_primary as i64),
                turso::Value::from(new_device.app_version_secondary as i64),
                turso::Value::from(new_device.app_version_tertiary as i64),
                turso::Value::from(new_device.app_version_last_fetched_ms),
                turso::Value::Null,
                turso::Value::Null,
                turso::Value::from(new_device.next_pre_key_id as i64),
                turso::Value::from(new_device.server_has_prekeys),
                turso::Value::Null,
                turso::Value::Null,
                turso::Value::from(0i64),
            ]),
        ).await.map(|_| self.device_id))
    }

    async fn snapshot_db(&self, name: &str, extra_content: Option<&[u8]>) -> wacore::store::error::Result<()> {
        let db_path = format!("/tmp/waos-snapshot-{name}.db");
        // Turso doesn't expose file path directly; we use execute_branch to checkpoint
        let conn = self.conn.lock();
        conn.execute_batch(&format!("VACUUM INTO '{db_path}'")).await
            .map_err(|e| wacore::store::error::StoreError::Database(Box::new(e)))?;
        if let Some(content) = extra_content {
            let content_path = format!("{db_path}.extra");
            std::fs::write(&content_path, content)
                .map_err(|e| wacore::store::error::StoreError::Database(Box::new(e)))?;
        }
        Ok(())
    }
}
