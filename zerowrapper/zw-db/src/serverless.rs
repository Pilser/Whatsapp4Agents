use crate::DbPool;
use serde::Serialize;
use std::path::{Path, PathBuf};
use turso::params::Params;
use turso::Value;

pub use crate::filters::{Agg, FilterCond, Op, SrvFilter, parse_filter, srv_filter_where};

#[derive(Debug, Clone, Serialize)]
pub struct SrvApp {
    pub board_id: String,
    pub owner_key: String,
    pub title: String,
    pub schema_json: Option<String>,
    pub public_reads: bool,
    pub record_count: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SrvRecord {
    pub seq: i64,
    pub payload: serde_json::Value,
    pub created_at: String,
}

fn sq(s: &str) -> String {
    s.replace('\'', "''")
}

async fn read_app(rows: &mut turso::Rows) -> anyhow::Result<Option<SrvApp>> {
    if let Some(row) = rows.next().await? {
        let owner_key = match row.get_value(0)? {
            Value::Text(s) => s,
            _ => return Err(anyhow::anyhow!("invalid owner_key")),
        };
        let board_id = match row.get_value(1)? {
            Value::Text(s) => s,
            _ => return Err(anyhow::anyhow!("invalid board_id")),
        };
        let title = match row.get_value(2)? {
            Value::Text(s) => s,
            _ => return Err(anyhow::anyhow!("invalid title")),
        };
        let schema_json = match row.get_value(3)? {
            Value::Text(s) => Some(s),
            _ => None,
        };
        let public_reads = match row.get_value(4)? {
            Value::Integer(i) => i != 0,
            _ => false,
        };
        let record_count = match row.get_value(5)? {
            Value::Integer(i) => i,
            _ => 0,
        };
        let created_at = match row.get_value(6)? {
            Value::Text(s) => s,
            _ => return Err(anyhow::anyhow!("invalid created_at")),
        };
        Ok(Some(SrvApp { board_id, owner_key, title, schema_json, public_reads, record_count, created_at }))
    } else {
        Ok(None)
    }
}

pub(crate) async fn read_record(rows: &mut turso::Rows) -> anyhow::Result<Option<SrvRecord>> {
    if let Some(row) = rows.next().await? {
        let seq = match row.get_value(0)? {
            Value::Integer(i) => i,
            _ => return Err(anyhow::anyhow!("invalid seq")),
        };
        let payload_text = match row.get_value(1)? {
            Value::Text(s) => s,
            _ => return Err(anyhow::anyhow!("invalid payload")),
        };
        let payload = serde_json::from_str::<serde_json::Value>(&payload_text)?;
        let created_at = match row.get_value(2)? {
            Value::Text(s) => s,
            _ => return Err(anyhow::anyhow!("invalid created_at")),
        };
        Ok(Some(SrvRecord { seq, payload, created_at }))
    } else {
        Ok(None)
    }
}

impl DbPool {
    pub async fn srv_app_create(&self, owner: &str, title: &str, schema_json: Option<&str>, public_reads: bool) -> anyhow::Result<String> {
        let conn = self.conn().await;
        let board_id = gen_board_id();
        let b = sq(&board_id);
        let o = sq(owner);
        let t = sq(title);
        let s = match schema_json {
            Some(s) => format!("'{}'", sq(s)),
            None => "NULL".to_string(),
        };
        let p = if public_reads { 1 } else { 0 };
        let sql = format!(
            "INSERT INTO wb_apps (board_id, owner_key, title, schema_json, public_reads) \
             VALUES ('{b}', '{o}', '{t}', {s}, {p})"
        );
        conn.execute(&sql, Params::Positional(vec![])).await?;
        Ok(board_id)
    }

    pub async fn srv_app_by_id(&self, board_id: &str) -> anyhow::Result<Option<SrvApp>> {
        let conn = self.conn().await;
        let mut rows = conn
            .query(
                "SELECT owner_key, board_id, title, schema_json, public_reads, \
                    (SELECT COUNT(*) FROM wb_records WHERE board_id = wb_apps.board_id), \
                    created_at \
                 FROM wb_apps WHERE board_id = ?1",
                Params::Positional(vec![Value::from(board_id)]),
            )
            .await?;
        read_app(&mut rows).await
    }

    pub async fn srv_app_owned(&self, board_id: &str, owner: &str) -> anyhow::Result<Option<SrvApp>> {
        let conn = self.conn().await;
        let mut rows = conn
            .query(
                "SELECT owner_key, board_id, title, schema_json, public_reads, \
                    (SELECT COUNT(*) FROM wb_records WHERE board_id = wb_apps.board_id), \
                    created_at \
                 FROM wb_apps WHERE board_id = ?1 AND owner_key = ?2",
                Params::Positional(vec![Value::from(board_id), Value::from(owner)]),
            )
            .await?;
        read_app(&mut rows).await
    }

    pub async fn srv_app_list(&self, owner: &str) -> anyhow::Result<Vec<SrvApp>> {
        let conn = self.conn().await;
        let mut rows = conn
            .query(
                "SELECT owner_key, board_id, title, schema_json, public_reads, \
                    (SELECT COUNT(*) FROM wb_records WHERE board_id = wb_apps.board_id), \
                    created_at \
                 FROM wb_apps WHERE owner_key = ?1 ORDER BY created_at",
                Params::Positional(vec![Value::from(owner)]),
            )
            .await?;
        let mut out = Vec::new();
        while let Some(app) = read_app(&mut rows).await? {
            out.push(app);
        }
        Ok(out)
    }

    pub async fn srv_app_update(&self, board_id: &str, owner: &str, title: Option<&str>, schema: Option<Option<&str>>, public_reads: Option<bool>) -> anyhow::Result<()> {
        let conn = self.conn().await;
        let mut rows = conn
            .query(
                "SELECT owner_key FROM wb_apps WHERE board_id = ?1",
                Params::Positional(vec![Value::from(board_id)]),
            )
            .await?;
        match rows.next().await? {
            Some(row) => match row.get_value(0)? {
                Value::Text(s) => {
                    if s != owner {
                        anyhow::bail!("board {board_id} is not owned by {owner}");
                    }
                }
                _ => return Err(anyhow::anyhow!("invalid owner_key")),
            },
            None => anyhow::bail!("board {board_id} not found"),
        }
        let mut sets: Vec<String> = Vec::new();
        if let Some(t) = title {
            sets.push(format!("title = '{}'", sq(t)));
        }
        match schema {
            Some(Some(s)) => sets.push(format!("schema_json = '{}'", sq(s))),
            Some(None) => sets.push("schema_json = NULL".to_string()),
            None => {}
        }
        if let Some(b) = public_reads {
            sets.push(format!("public_reads = {}", if b { 1 } else { 0 }));
        }
        if sets.is_empty() {
            return Ok(());
        }
        let b = sq(board_id);
        let sql = format!("UPDATE wb_apps SET {} WHERE board_id = '{b}'", sets.join(", "));
        conn.execute(&sql, Params::Positional(vec![])).await?;
        Ok(())
    }

    pub async fn srv_app_delete(&self, board_id: &str, owner: &str) -> anyhow::Result<()> {
        let conn = self.conn().await;
        let mut rows = conn
            .query(
                "SELECT owner_key FROM wb_apps WHERE board_id = ?1",
                Params::Positional(vec![Value::from(board_id)]),
            )
            .await?;
        let is_owner = match rows.next().await? {
            Some(row) => match row.get_value(0)? {
                Value::Text(s) => s == owner,
                _ => false,
            },
            None => false,
        };
        if !is_owner {
            anyhow::bail!("board {board_id} is not owned by {owner}");
        }
        let b = sq(board_id);
        let o = sq(owner);
        let sql = format!(
            "BEGIN; \
             DELETE FROM wb_records WHERE board_id = '{b}'; \
             DELETE FROM wb_apps WHERE board_id = '{b}' AND owner_key = '{o}'; \
             COMMIT;"
        );
        conn.execute_batch(&sql).await?;
        Ok(())
    }

    pub async fn srv_validate(&self, board_id: &str, payload: &serde_json::Value) -> anyhow::Result<()> {
        let app = self
            .srv_app_by_id(board_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("board {board_id} not found"))?;
        let Some(schema_text) = app.schema_json.as_deref() else {
            return Ok(());
        };
        let schema: serde_json::Value = serde_json::from_str(schema_text)
            .map_err(|e| anyhow::anyhow!("board {board_id} has an invalid schema_json: {e}"))?;
        let compiled = jsonschema::options_for::<jsonschema::json::SerdeJson>()
            .should_validate_formats(true)
            .build(&schema)
            .map_err(|e| anyhow::anyhow!("board {board_id} has an invalid schema_json: {e}"))?;
        match compiled.validate(payload) {
            Ok(()) => Ok(()),
            Err(e) => anyhow::bail!("payload failed schema: {e}"),
        }
    }

    pub async fn srv_record_insert(&self, board_id: &str, payload: &serde_json::Value) -> anyhow::Result<i64> {
        let conn = self.conn().await;
        let mut rows = conn
            .query(
                "SELECT owner_key FROM wb_apps WHERE board_id = ?1",
                Params::Positional(vec![Value::from(board_id)]),
            )
            .await?;
        let owner = match rows.next().await? {
            Some(row) => match row.get_value(0)? {
                Value::Text(s) => s,
                _ => return Err(anyhow::anyhow!("invalid owner_key")),
            },
            None => anyhow::bail!("board {board_id} does not exist"),
        };
        let mut seq_rows = conn
            .query(
                "SELECT COALESCE((SELECT MAX(seq) FROM wb_records WHERE board_id = ?1), 0) + 1",
                Params::Positional(vec![Value::from(board_id)]),
            )
            .await?;
        let seq = match seq_rows.next().await? {
            Some(row) => match row.get_value(0)? {
                Value::Integer(i) => i,
                _ => return Err(anyhow::anyhow!("invalid next seq")),
            },
            None => 1,
        };
        let text = serde_json::to_string(payload)?;
        let b = sq(board_id);
        let o = sq(&owner);
        let t = sq(&text);
        let sql = format!(
            "BEGIN; \
             INSERT INTO wb_records (board_id, owner_key, seq, payload) VALUES ('{b}', '{o}', {seq}, '{t}'); \
             COMMIT;"
        );
        conn.execute_batch(&sql).await?;
        Ok(seq)
    }

    pub async fn srv_record_list(&self, board_id: &str, limit: i64, before: Option<i64>) -> anyhow::Result<Vec<SrvRecord>> {
        let conn = self.conn().await;
        let limit = limit.clamp(1, 200);
        let (sql, params) = match before {
            Some(b) => (
                "SELECT seq, payload, created_at FROM wb_records \
                 WHERE board_id = ?1 AND seq < ?2 ORDER BY seq DESC LIMIT ?3",
                Params::Positional(vec![Value::from(board_id), Value::Integer(b), Value::Integer(limit)]),
            ),
            None => (
                "SELECT seq, payload, created_at FROM wb_records \
                 WHERE board_id = ?1 ORDER BY seq DESC LIMIT ?2",
                Params::Positional(vec![Value::from(board_id), Value::Integer(limit)]),
            ),
        };
        let mut rows = conn.query(sql, params).await?;
        let mut out = Vec::new();
        while let Some(record) = read_record(&mut rows).await? {
            out.push(record);
        }
        Ok(out)
    }

    pub async fn srv_record_get(&self, board_id: &str, seq: i64) -> anyhow::Result<Option<SrvRecord>> {
        let conn = self.conn().await;
        let mut rows = conn
            .query(
                "SELECT seq, payload, created_at FROM wb_records WHERE board_id = ?1 AND seq = ?2",
                Params::Positional(vec![Value::from(board_id), Value::Integer(seq)]),
            )
            .await?;
        read_record(&mut rows).await
    }

    pub async fn srv_record_count(&self, board_id: &str) -> anyhow::Result<i64> {
        let conn = self.conn().await;
        let mut rows = conn
            .query(
                "SELECT COUNT(*) FROM wb_records WHERE board_id = ?1",
                Params::Positional(vec![Value::from(board_id)]),
            )
            .await?;
        match rows.next().await? {
            Some(row) => match row.get_value(0)? {
                Value::Integer(i) => Ok(i),
                _ => Ok(0),
            },
            None => Ok(0),
        }
    }

    pub async fn srv_record_query(&self, board_id: &str, json_path: &str, eq: Option<&str>) -> anyhow::Result<Vec<SrvRecord>> {
        let conn = self.conn().await;
        let (sql, params) = match eq {
            Some(v) => (
                "SELECT seq, payload, created_at FROM wb_records \
                 WHERE board_id = ?1 AND CAST(json_extract(payload, ?2) AS TEXT) = ?3 ORDER BY seq DESC",
                Params::Positional(vec![Value::from(board_id), Value::from(json_path), Value::from(v)]),
            ),
            None => (
                "SELECT seq, payload, created_at FROM wb_records \
                 WHERE board_id = ?1 ORDER BY seq DESC",
                Params::Positional(vec![Value::from(board_id)]),
            ),
        };
        let mut rows = conn.query(sql, params).await?;
        let mut out = Vec::new();
        while let Some(record) = read_record(&mut rows).await? {
            out.push(record);
        }
        Ok(out)
    }

    pub async fn srv_record_delete(&self, board_id: &str, seq: Option<i64>) -> anyhow::Result<()> {
        let conn = self.conn().await;
        match seq {
            Some(s) => {
                conn.execute(
                    "DELETE FROM wb_records WHERE board_id = ?1 AND seq = ?2",
                    Params::Positional(vec![Value::from(board_id), Value::Integer(s)]),
                )
                .await?;
            }
            None => {
                conn.execute(
                    "DELETE FROM wb_records WHERE board_id = ?1",
                    Params::Positional(vec![Value::from(board_id)]),
                )
                .await?;
            }
        }
        Ok(())
    }
}

pub fn srv_session_dir(root: &Path, owner: &str) -> PathBuf {
    root.join("sessions").join(owner)
}

pub fn srv_board_files_dir(root: &Path, owner: &str, board_id: &str) -> PathBuf {
    srv_session_dir(root, owner).join("apps").join(board_id).join("files")
}

pub fn srv_board_file_path(root: &Path, owner: &str, board_id: &str, file_id: &str) -> PathBuf {
    srv_board_files_dir(root, owner, board_id).join(format!("{file_id}.bin"))
}

pub fn srv_list_files(root: &Path, owner: &str, board_id: &str) -> anyhow::Result<Vec<(String, u64)>> {
    let dir = srv_board_files_dir(root, owner, board_id);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("bin") {
            let meta = std::fs::metadata(&path)?;
            let file_id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            out.push((file_id, meta.len()));
        }
    }
    Ok(out)
}

fn gen_id(prefix: &str) -> String {
    const CHARS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut s = prefix.to_string();
    let mut n = nanos;
    for _ in 0..16 {
        s.push(CHARS[(n % 36) as usize] as char);
        n /= 36;
    }
    s
}

pub fn gen_board_id() -> String {
    gen_id("b_")
}

pub fn gen_file_id() -> String {
    gen_id("f_")
}