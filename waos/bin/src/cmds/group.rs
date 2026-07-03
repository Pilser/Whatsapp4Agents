use clap::Subcommand;

use waos_lib::config::Config;

fn text_val(v: Result<turso::Value, turso::Error>) -> String {
    match v { Ok(turso::Value::Text(s)) => s, _ => String::new() }
}

fn int_val(v: Result<turso::Value, turso::Error>) -> i64 {
    match v { Ok(turso::Value::Integer(n)) => n, _ => 0 }
}

async fn open_conn(alias: &str) -> anyhow::Result<turso::Connection> {
    let config = Config::load();
    let db_path = config.data_dir().join("accounts").join(format!("{alias}.db"));
    if !db_path.exists() {
        anyhow::bail!("No database found for account '{alias}'");
    }
    let db = turso::Builder::new_local(db_path.to_str().unwrap()).build().await?;
    Ok(db.connect()?)
}

async fn agent_jid(data_dir: &std::path::Path, alias: &str) -> Option<String> {
    let session_db = data_dir.join("sessions").join(alias).join("session.db");
    if !session_db.exists() {
        return None;
    }
    let db = turso::Builder::new_local(session_db.to_str()?).build().await.ok()?;
    let conn = db.connect().ok()?;
    let mut rows = conn.query("SELECT pn FROM device LIMIT 1", ()).await.ok()?;
    let row = rows.next().await.ok()??;
    let raw = text_val(row.get_value(0));
    if raw.is_empty() { None } else {
        let jid = if let Some(at) = raw.find('@') {
            if let Some(colon) = raw[..at].rfind(':') {
                format!("{}{}", &raw[..colon], &raw[at..])
            } else { raw }
        } else { raw };
        Some(jid)
    }
}

#[derive(Subcommand)]
pub enum GroupCommand {
    /// List all groups and communities with metadata and enabled status
    List,
    /// Enable message storage for a group/community
    Enable {
        group_id: String,
    },
    /// Disable message storage for a group/community
    Disable {
        group_id: String,
    },
    /// Show messages that mentioned/ tagged the agent
    Mentions {
        /// Optional group JID to filter by
        group_id: Option<String>,
        #[arg(long, default_value = "20")]
        limit: usize,
    },
    /// Show messages that are replies to the agent
    Replies {
        /// Optional group JID to filter by
        group_id: Option<String>,
        #[arg(long, default_value = "20")]
        limit: usize,
    },
}

pub async fn run(alias: &str, cmd: GroupCommand) -> anyhow::Result<()> {
    match cmd {
        GroupCommand::List => list(alias).await,
        GroupCommand::Enable { group_id } => set_enabled(alias, &group_id, true).await,
        GroupCommand::Disable { group_id } => set_enabled(alias, &group_id, false).await,
        GroupCommand::Mentions { group_id, limit } => mentions(alias, group_id, limit).await,
        GroupCommand::Replies { group_id, limit } => replies(alias, group_id, limit).await,
    }
}

async fn list(alias: &str) -> anyhow::Result<()> {
    let conn = open_conn(alias).await?;
    let mut rows = conn.query(
        "SELECT id, name, type, enabled FROM wa_groups ORDER BY name ASC",
        (),
    ).await?;
    let mut results = Vec::new();
    while let Some(row) = rows.next().await? {
        results.push(serde_json::json!({
            "id": text_val(row.get_value(0)),
            "name": text_val(row.get_value(1)),
            "type": text_val(row.get_value(2)),
            "enabled": int_val(row.get_value(3)) != 0,
        }));
    }
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}

async fn set_enabled(alias: &str, group_id: &str, enabled: bool) -> anyhow::Result<()> {
    let conn = open_conn(alias).await?;
    let val: i64 = if enabled { 1 } else { 0 };
    conn.execute(
        "UPDATE wa_groups SET enabled = ?1 WHERE id = ?2",
        turso::params::Params::Positional(vec![
            turso::Value::from(val),
            turso::Value::from(group_id),
        ]),
    ).await?;
    let status = if enabled { "enabled" } else { "disabled" };
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
        "account": alias,
        "group_id": group_id,
        "enabled": enabled,
        "status": status,
    }))?);
    Ok(())
}

async fn mentions(alias: &str, group_id: Option<String>, limit: usize) -> anyhow::Result<()> {
    let config = Config::load();
    let my_jid = agent_jid(&config.data_dir(), alias).await
        .ok_or_else(|| anyhow::anyhow!("Agent not paired yet — no session JID"))?;

    let conn = open_conn(alias).await?;
    let pattern = format!("%{}%", my_jid);
    let (sql, params): (&str, Vec<turso::Value>) = match &group_id {
        Some(gid) => (
            "SELECT id, group_jid, sender_name, content, mentions, quoted_message_id, timestamp
             FROM group_messages WHERE group_jid = ?1 AND mentions LIKE ?2
             ORDER BY timestamp DESC LIMIT ?3",
            vec![
                turso::Value::from(gid.as_str()),
                turso::Value::from(pattern),
                turso::Value::from(limit as i64),
            ],
        ),
        None => (
            "SELECT id, group_jid, sender_name, content, mentions, quoted_message_id, timestamp
             FROM group_messages WHERE mentions LIKE ?1
             ORDER BY timestamp DESC LIMIT ?2",
            vec![
                turso::Value::from(pattern),
                turso::Value::from(limit as i64),
            ],
        ),
    };
    let mut rows = conn.query(sql, turso::params::Params::Positional(params)).await?;
    let mut results = Vec::new();
    while let Some(row) = rows.next().await? {
        results.push(serde_json::json!({
            "id": text_val(row.get_value(0)),
            "group_jid": text_val(row.get_value(1)),
            "sender": text_val(row.get_value(2)),
            "content": text_val(row.get_value(3)),
            "mentions": text_val(row.get_value(4)),
            "quoted_message_id": text_val(row.get_value(5)),
            "timestamp": int_val(row.get_value(6)),
        }));
    }
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}

async fn replies(alias: &str, group_id: Option<String>, limit: usize) -> anyhow::Result<()> {
    let config = Config::load();
    let my_jid = agent_jid(&config.data_dir(), alias).await
        .ok_or_else(|| anyhow::anyhow!("Agent not paired yet — no session JID"))?;

    let conn = open_conn(alias).await?;
    let (sql, params): (&str, Vec<turso::Value>) = match &group_id {
        Some(gid) => (
            "SELECT id, group_jid, sender_name, content, quoted_message_id, quoted_content, quoted_sender, timestamp
             FROM group_messages WHERE group_jid = ?1 AND quoted_sender = ?2
             ORDER BY timestamp DESC LIMIT ?3",
            vec![
                turso::Value::from(gid.as_str()),
                turso::Value::from(my_jid),
                turso::Value::from(limit as i64),
            ],
        ),
        None => (
            "SELECT id, group_jid, sender_name, content, quoted_message_id, quoted_content, quoted_sender, timestamp
             FROM group_messages WHERE quoted_sender = ?1
             ORDER BY timestamp DESC LIMIT ?2",
            vec![
                turso::Value::from(my_jid),
                turso::Value::from(limit as i64),
            ],
        ),
    };
    let mut rows = conn.query(sql, turso::params::Params::Positional(params)).await?;
    let mut results = Vec::new();
    while let Some(row) = rows.next().await? {
        results.push(serde_json::json!({
            "id": text_val(row.get_value(0)),
            "group_jid": text_val(row.get_value(1)),
            "sender": text_val(row.get_value(2)),
            "content": text_val(row.get_value(3)),
            "quoted_message_id": text_val(row.get_value(4)),
            "quoted_content": text_val(row.get_value(5)),
            "quoted_sender": text_val(row.get_value(6)),
            "timestamp": int_val(row.get_value(7)),
        }));
    }
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}
