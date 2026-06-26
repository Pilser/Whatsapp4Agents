use std::path::Path;
use waos_lib::config::Config;

fn text_val(v: Result<turso::Value, turso::Error>) -> String {
    match v {
        Ok(turso::Value::Text(s)) => s,
        Ok(turso::Value::Integer(n)) => n.to_string(),
        Ok(turso::Value::Real(f)) => f.to_string(),
        _ => String::new(),
    }
}

fn int_val(v: Result<turso::Value, turso::Error>) -> i64 {
    match v {
        Ok(turso::Value::Integer(n)) => n,
        Ok(turso::Value::Real(f)) => f as i64,
        _ => 0,
    }
}

pub async fn list(alias: &str) -> anyhow::Result<()> {
    let config = Config::load();
    let data_dir = config.data_dir();
    let db_path = data_dir.join("accounts").join(format!("{alias}.db"));

    if !db_path.exists() {
        println!("No database found for account '{alias}'");
        return Ok(());
    }

    let db = turso::Builder::new_local(db_path.to_str().unwrap())
        .build()
        .await?;
    let conn = db.connect()?;

    let mut rows = conn
        .query(
            "SELECT id, name, type, unread_count, last_message_snippet, last_message_at
             FROM chats ORDER BY last_message_at DESC",
            (),
        )
        .await?;

    let mut chats = Vec::new();
    while let Some(row) = rows.next().await? {
        chats.push(serde_json::json!({
            "id": text_val(row.get_value(0)),
            "name": text_val(row.get_value(1)),
            "type": text_val(row.get_value(2)),
            "unread": int_val(row.get_value(3)),
            "last_message": text_val(row.get_value(4)),
            "last_activity": int_val(row.get_value(5)),
        }));
    }
    println!("{}", serde_json::to_string_pretty(&chats)?);
    Ok(())
}

pub async fn unread(alias: &str) -> anyhow::Result<()> {
    let config = Config::load();
    let data_dir = config.data_dir();
    let db_path = data_dir.join("accounts").join(format!("{alias}.db"));

    if !db_path.exists() {
        println!("No database found for account '{alias}'");
        return Ok(());
    }

    let db = turso::Builder::new_local(db_path.to_str().unwrap())
        .build()
        .await?;
    let conn = db.connect()?;

    let mut rows = conn
        .query(
            "SELECT id, name, type, unread_count, last_message_snippet, last_message_at
             FROM chats WHERE unread_count > 0
             ORDER BY last_message_at DESC",
            (),
        )
        .await?;

    let mut chats = Vec::new();
    while let Some(row) = rows.next().await? {
        chats.push(serde_json::json!({
            "id": text_val(row.get_value(0)),
            "name": text_val(row.get_value(1)),
            "unread": int_val(row.get_value(3)),
        }));
    }
    println!("{}", serde_json::to_string_pretty(&chats)?);
    Ok(())
}

pub async fn search(alias: &str, query_str: &str) -> anyhow::Result<()> {
    let config = Config::load();
    let data_dir = config.data_dir();
    let db_path = data_dir.join("accounts").join(format!("{alias}.db"));

    if !db_path.exists() {
        println!("No database found for account '{alias}'");
        return Ok(());
    }

    let db = turso::Builder::new_local(db_path.to_str().unwrap())
        .build()
        .await?;
    let conn = db.connect()?;

    let pattern = format!("%{}%", query_str);
    let mut rows = conn
        .query(
            "SELECT m.id, m.chat_id, c.name, m.sender_name, m.content, m.timestamp
             FROM messages m JOIN chats c ON c.id = m.chat_id
             WHERE m.content LIKE ?1 ORDER BY m.timestamp DESC LIMIT 50",
            turso::params::Params::Positional(vec![turso::Value::from(pattern)]),
        )
        .await?;

    let mut results = Vec::new();
    while let Some(row) = rows.next().await? {
        results.push(serde_json::json!({
            "id": text_val(row.get_value(0)),
            "chat": text_val(row.get_value(2)),
            "sender": text_val(row.get_value(3)),
            "content": text_val(row.get_value(4)),
        }));
    }
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}

fn normalize_jid(jid: &str) -> String {
    if let Some(at_pos) = jid.find('@') {
        if let Some(colon_pos) = jid[..at_pos].rfind(':') {
            return format!("{}{}", &jid[..colon_pos], &jid[at_pos..]);
        }
    }
    jid.to_string()
}

async fn agent_jid(data_dir: &Path, alias: &str) -> Option<String> {
    let session_db = data_dir.join("sessions").join(alias).join("session.db");
    if !session_db.exists() {
        return None;
    }
    let db = turso::Builder::new_local(session_db.to_str()?).build().await.ok()?;
    let conn = db.connect().ok()?;
    let mut rows = conn.query("SELECT pn FROM device LIMIT 1", ()).await.ok()?;
    let row = rows.next().await.ok()??;
    let raw = text_val(row.get_value(0));
    if raw.is_empty() { None } else { Some(normalize_jid(&raw)) }
}

pub async fn history(alias: &str, chat_id: &str, limit: usize, before: Option<i64>, after: Option<i64>, offset: usize, total: bool) -> anyhow::Result<()> {
    let config = Config::load();
    let db_path = config.account_db(alias);

    if !db_path.exists() {
        println!("No database found for account '{alias}'");
        return Ok(());
    }

    let db = turso::Builder::new_local(db_path.to_str().unwrap()).build().await?;
    let conn = db.connect()?;

    let my_jid = agent_jid(&config.data_dir(), alias).await;

    let total_count = if total {
        let mut count_rows = conn.query(
            "SELECT COUNT(*) FROM messages WHERE chat_id = ?1",
            turso::params::Params::Positional(vec![turso::Value::from(chat_id)]),
        ).await?;
        match count_rows.next().await.ok().flatten().and_then(|r| r.get_value(0).ok()) {
            Some(turso::Value::Integer(n)) => n,
            _ => 0,
        }
    } else {
        0
    };

    let (sql, params): (&str, Vec<turso::Value>) = match (before, after) {
        (Some(ts), _) => (
            "SELECT id, sender_jid, sender_name, content, timestamp, message_type, has_media, status, media_path
             FROM messages WHERE chat_id = ?1 AND timestamp < ?2
             ORDER BY timestamp DESC LIMIT ?3 OFFSET ?4",
            vec![turso::Value::from(chat_id), turso::Value::from(ts), turso::Value::from(limit as i64), turso::Value::from(offset as i64)],
        ),
        (_, Some(ts)) => (
            "SELECT id, sender_jid, sender_name, content, timestamp, message_type, has_media, status, media_path
             FROM messages WHERE chat_id = ?1 AND timestamp > ?2
             ORDER BY timestamp ASC LIMIT ?3 OFFSET ?4",
            vec![turso::Value::from(chat_id), turso::Value::from(ts), turso::Value::from(limit as i64), turso::Value::from(offset as i64)],
        ),
        _ => (
            "SELECT id, sender_jid, sender_name, content, timestamp, message_type, has_media, status, media_path
             FROM messages WHERE chat_id = ?1
             ORDER BY timestamp DESC LIMIT ?2 OFFSET ?3",
            vec![turso::Value::from(chat_id), turso::Value::from(limit as i64), turso::Value::from(offset as i64)],
        ),
    };

    let mut rows = conn.query(sql, turso::params::Params::Positional(params)).await?;

    let mut msgs = Vec::new();
    while let Some(row) = rows.next().await? {
        let jid = text_val(row.get_value(1));
        let is_me = my_jid.as_ref().is_some_and(|mine| normalize_jid(&jid) == *mine);
        msgs.push(serde_json::json!({
            "id": text_val(row.get_value(0)),
            "sender": text_val(row.get_value(2)),
            "content": text_val(row.get_value(3)),
            "timestamp": int_val(row.get_value(4)),
            "type": text_val(row.get_value(5)),
            "has_media": int_val(row.get_value(6)) != 0,
            "is_me": is_me,
            "status": text_val(row.get_value(7)),
            "media_path": text_val(row.get_value(8)),
        }));
    }

    if before.is_none() && after.is_none() {
        msgs.reverse();
    }
    if before.is_some() {
        msgs.reverse();
    }

    if total {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({"messages": msgs, "total": total_count, "chat_id": chat_id, "limit": limit}))?);
    } else {
        println!("{}", serde_json::to_string_pretty(&msgs)?);
    }
    Ok(())
}

pub async fn mark_read(alias: &str, chat_id: &str) -> anyhow::Result<()> {
    let config = Config::load();
    let data_dir = config.data_dir();
    let db_path = data_dir.join("accounts").join(format!("{alias}.db"));

    if !db_path.exists() {
        println!("No database found for account '{alias}'");
        return Ok(());
    }

    let db = turso::Builder::new_local(db_path.to_str().unwrap())
        .build()
        .await?;
    let conn = db.connect()?;

    conn.execute(
        "UPDATE chats SET unread_count = 0, updated_at = ?1 WHERE id = ?2",
        turso::params::Params::Positional(vec![
            turso::Value::from(chrono::Utc::now().timestamp()),
            turso::Value::from(chat_id),
        ]),
    ).await?;

    println!(r#"{{"success":true,"account":"{alias}","chat_id":"{chat_id}","action":"mark_read"}}"#);
    Ok(())
}
