use turso::{Connection, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaVersion {
    pub primary: i64,
    pub secondary: i64,
    pub tertiary: i64,
    pub fetched_at: i64,
}

pub async fn read_wa_version(conn: &Connection) -> Option<WaVersion> {
    let mut rows = conn
        .query(
            "SELECT primary_v, secondary_v, tertiary_v, fetched_at FROM wa_version WHERE id = 1",
            (),
        )
        .await
        .ok()?;

    let row = rows.next().await.ok()??;
    let read_int = |idx: usize| -> Option<i64> {
        match row.get_value(idx).ok()? {
            Value::Integer(n) => Some(n),
            _ => None,
        }
    };
    Some(WaVersion {
        primary: read_int(0)?,
        secondary: read_int(1)?,
        tertiary: read_int(2)?,
        fetched_at: read_int(3)?,
    })
}

pub async fn write_wa_version(conn: &Connection, version: &WaVersion) {
    let _ = conn
        .execute(
            "INSERT OR REPLACE INTO wa_version (id, primary_v, secondary_v, tertiary_v, fetched_at) VALUES (1, ?1, ?2, ?3, ?4)",
            turso::params::Params::Positional(vec![
                Value::from(version.primary),
                Value::from(version.secondary),
                Value::from(version.tertiary),
                Value::from(version.fetched_at),
            ]),
        )
        .await;
}
