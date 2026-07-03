use crate::DbPool;
use turso::params::Params;
use turso::Value;

#[derive(Debug, Clone)]
pub struct UserRow {
    pub id: String,
    pub external_id: Option<String>,
    pub name: String,
    pub created_at: String,
}

impl DbPool {
    pub async fn upsert_user(&self, id: &str, name: Option<&str>, external_id: Option<&str>) -> anyhow::Result<()> {
        let conn = self.conn().await;
        let name_value = match name {
            Some(s) if !s.is_empty() => Value::from(s),
            _ => Value::Null,
        };
        conn.execute(
            "INSERT INTO users (id, name, external_id) VALUES (?1, COALESCE(?2, 'unknown'), ?3) ON CONFLICT(id) DO UPDATE SET name = CASE WHEN ?2 IS NOT NULL THEN ?2 ELSE name END, external_id = COALESCE(?3, external_id)",
            Params::Positional(vec![
                Value::from(id),
                name_value,
                external_id.map(Value::from).unwrap_or(Value::Null),
            ]),
        ).await?;
        Ok(())
    }

    pub async fn get_user(&self, id: &str) -> anyhow::Result<Option<UserRow>> {
        let conn = self.conn().await;
        let mut rows = conn.query(
            "SELECT id, external_id, name, created_at FROM users WHERE id = ?1",
            Params::Positional(vec![Value::from(id)]),
        ).await?;
        if let Some(row) = rows.next().await? {
            Ok(Some(UserRow {
                id: match row.get_value(0)? { Value::Text(s) => s, _ => String::new() },
                external_id: match row.get_value(1)? { Value::Text(s) if !s.is_empty() => Some(s), _ => None },
                name: match row.get_value(2)? { Value::Text(s) => s, _ => String::new() },
                created_at: match row.get_value(3)? { Value::Text(s) => s, _ => String::new() },
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn get_user_external_id(&self, id: &str) -> anyhow::Result<Option<String>> {
        let conn = self.conn().await;
        let mut rows = conn.query(
            "SELECT external_id FROM users WHERE id = ?1",
            Params::Positional(vec![Value::from(id)]),
        ).await?;
        if let Some(row) = rows.next().await? {
            match row.get_value(0)? {
                Value::Text(s) if !s.is_empty() => Ok(Some(s)),
                _ => Ok(None),
            }
        } else {
            Ok(None)
        }
    }
}
