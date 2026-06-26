use clap::Subcommand;

use waos_lib::config::Config;

fn text_val(v: Result<turso::Value, turso::Error>) -> String {
    match v { Ok(turso::Value::Text(s)) => s, _ => String::new() }
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
pub enum CommunityCommand {
    List,
    Info { community_id: String },
}

pub async fn run(alias: &str, cmd: CommunityCommand) -> anyhow::Result<()> {
    match cmd {
        CommunityCommand::List => list(alias).await,
        CommunityCommand::Info { community_id } => info(alias, &community_id).await,
    }
}

async fn list(alias: &str) -> anyhow::Result<()> {
    let conn = open_conn(alias).await?;
    let mut rows = conn.query(
        "SELECT id, name FROM chats WHERE type = 'community' ORDER BY name ASC",
        (),
    ).await?;
    let mut results = Vec::new();
    while let Some(row) = rows.next().await? {
        results.push(serde_json::json!({
            "id": text_val(row.get_value(0)),
            "name": text_val(row.get_value(1)),
        }));
    }
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}

async fn info(alias: &str, community_id: &str) -> anyhow::Result<()> {
    let conn = open_conn(alias).await?;
    let mut rows = conn.query(
        "SELECT id, name, unread_count, last_message_snippet, last_message_at
         FROM chats WHERE id = ?1",
        turso::params::Params::Positional(vec![turso::Value::from(community_id)]),
    ).await?;

    let row = match rows.next().await? {
        Some(r) => r,
        None => { println!("{{}}"); return Ok(()); }
    };

    let mut members = conn.query(
        "SELECT c.jid, COALESCE(c.push_name, c.name, 'Unknown')
         FROM contacts c
         JOIN graph_edges e ON e.from_jid = c.jid
         WHERE e.to_jid = ?1 AND e.edge_type = 'member_of'
         LIMIT 50",
        turso::params::Params::Positional(vec![turso::Value::from(community_id)]),
    ).await?;
    let mut member_list = Vec::new();
    while let Some(m) = members.next().await? {
        member_list.push(serde_json::json!({
            "jid": text_val(m.get_value(0)),
            "name": text_val(m.get_value(1)),
        }));
    }

    let result = serde_json::json!({
        "id": text_val(row.get_value(0)),
        "name": text_val(row.get_value(1)),
        "unread_count": text_val(row.get_value(2)),
        "last_message": text_val(row.get_value(3)),
        "last_activity": text_val(row.get_value(4)),
        "members": member_list,
    });
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
