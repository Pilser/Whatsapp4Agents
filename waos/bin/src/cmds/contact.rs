use clap::Subcommand;

use waos_lib::config::Config;

fn text_val(v: Result<turso::Value, turso::Error>) -> String {
    match v {
        Ok(turso::Value::Text(s)) => s,
        Ok(turso::Value::Integer(n)) => n.to_string(),
        Ok(turso::Value::Real(f)) => f.to_string(),
        _ => String::new(),
    }
}

fn real_val(v: Result<turso::Value, turso::Error>) -> f64 {
    match v { Ok(turso::Value::Real(f)) => f, Ok(turso::Value::Integer(n)) => n as f64, _ => 0.0 }
}

fn int_val(v: Result<turso::Value, turso::Error>) -> i64 {
    match v { Ok(turso::Value::Integer(n)) => n, Ok(turso::Value::Real(f)) => f as i64, _ => 0 }
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

#[derive(Subcommand)]
pub enum ContactCommand {
    Search {
        query: String,
        /// Block and wait up to N seconds for result (--wait alone defaults to 30)
        #[arg(long, default_missing_value = "30", num_args = 0..=1)]
        wait: Option<u64>,
    },
    Info {
        jid: String,
        /// Block and wait up to N seconds for result (--wait alone defaults to 30)
        #[arg(long, default_missing_value = "30", num_args = 0..=1)]
        wait: Option<u64>,
    },
    Score {
        jid: String,
        value: f64,
        /// Block and wait up to N seconds for result (--wait alone defaults to 30)
        #[arg(long, default_missing_value = "30", num_args = 0..=1)]
        wait: Option<u64>,
    },
}

pub async fn run(alias: &str, cmd: ContactCommand) -> anyhow::Result<()> {
    match cmd {
        ContactCommand::Search { query, .. } => search(alias, &query).await,
        ContactCommand::Info { jid, .. } => info(alias, &jid).await,
        ContactCommand::Score { jid, value, .. } => set_score(alias, &jid, value).await,
    }
}

async fn search(alias: &str, query: &str) -> anyhow::Result<()> {
    let conn = open_conn(alias).await?;
    let pattern = format!("%{}%", query);
    let mut rows = conn.query(
        "SELECT c.jid, COALESCE(c.push_name, c.name, ''), c.phone,
                COALESCE(cs.importance, 0.0), COALESCE(cs.frequency, 0)
         FROM contacts c
         LEFT JOIN contact_scores cs ON cs.jid = c.jid
         WHERE c.push_name LIKE ?1 OR c.name LIKE ?1 OR c.phone LIKE ?1
         ORDER BY cs.importance DESC LIMIT 20",
        turso::params::Params::Positional(vec![turso::Value::from(pattern)]),
    ).await?;

    let mut results = Vec::new();
    while let Some(row) = rows.next().await? {
        results.push(serde_json::json!({
            "jid": text_val(row.get_value(0)),
            "name": text_val(row.get_value(1)),
            "phone": text_val(row.get_value(2)),
            "importance": real_val(row.get_value(3)),
            "interactions": int_val(row.get_value(4)),
        }));
    }
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}

async fn info(alias: &str, jid: &str) -> anyhow::Result<()> {
    let conn = open_conn(alias).await?;
    let mut rows = conn.query(
        "SELECT c.jid, COALESCE(c.push_name, c.name, ''), c.phone, c.is_group,
                c.last_interaction_at,
                COALESCE(cs.frequency, 0), COALESCE(cs.recency, 0),
                COALESCE(cs.reply_rate, 0.0), COALESCE(cs.importance, 0.0)
         FROM contacts c
         LEFT JOIN contact_scores cs ON cs.jid = c.jid
         WHERE c.jid = ?1",
        turso::params::Params::Positional(vec![turso::Value::from(jid)]),
    ).await?;

    let row = match rows.next().await? {
        Some(r) => r,
        None => { println!("{{}}"); return Ok(()); }
    };

    let mut mem_rows = conn.query(
        "SELECT content FROM memory WHERE context_jid = ?1 AND key = 'conversation' ORDER BY created_at DESC LIMIT 1",
        turso::params::Params::Positional(vec![turso::Value::from(jid)]),
    ).await?;
    let memory: Option<serde_json::Value> = if let Some(mr) = mem_rows.next().await? {
        serde_json::from_str(&text_val(mr.get_value(0))).ok()
    } else { None };

    let mut edge_rows = conn.query(
        "SELECT to_jid, edge_type, strength FROM graph_edges WHERE from_jid = ?1 LIMIT 20",
        turso::params::Params::Positional(vec![turso::Value::from(jid)]),
    ).await?;
    let mut edges = Vec::new();
    while let Some(er) = edge_rows.next().await? {
        edges.push(serde_json::json!({
            "to": text_val(er.get_value(0)),
            "type": text_val(er.get_value(1)),
            "strength": real_val(er.get_value(2)),
        }));
    }

    let result = serde_json::json!({
        "jid": text_val(row.get_value(0)),
        "name": text_val(row.get_value(1)),
        "phone": text_val(row.get_value(2)),
        "is_group": int_val(row.get_value(3)) != 0,
        "last_interaction": int_val(row.get_value(4)),
        "scores": {
            "frequency": int_val(row.get_value(5)),
            "recency": int_val(row.get_value(6)),
            "reply_rate": real_val(row.get_value(7)),
            "importance": real_val(row.get_value(8)),
        },
        "memory": memory,
        "graph_edges": edges,
    });
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

async fn set_score(alias: &str, jid: &str, value: f64) -> anyhow::Result<()> {
    let conn = open_conn(alias).await?;
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "INSERT INTO contact_scores (jid, importance, updated_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(jid) DO UPDATE SET importance = ?2, updated_at = ?3",
        turso::params::Params::Positional(vec![
            turso::Value::from(jid),
            turso::Value::from(value.clamp(0.0, 1.0)),
            turso::Value::from(now),
        ]),
    ).await?;
    println!(r#"{{"success":true,"jid":"{jid}","importance":{value}}}"#);
    Ok(())
}
