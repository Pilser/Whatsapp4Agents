use clap::Subcommand;

use waos_lib::config::Config;

fn text_val(v: Result<turso::Value, turso::Error>) -> String {
    match v {
        Ok(turso::Value::Text(s)) => s,
        _ => String::new(),
    }
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
pub enum MemoryCommand {
    Get { chat_id: String },
    Set { chat_id: String, json: String },
    Search { query: String },
}

pub async fn run(alias: &str, cmd: MemoryCommand) -> anyhow::Result<()> {
    match cmd {
        MemoryCommand::Get { chat_id } => get(alias, &chat_id).await,
        MemoryCommand::Set { chat_id, json } => set(alias, &chat_id, &json).await,
        MemoryCommand::Search { query } => search(alias, &query).await,
    }
}

async fn get(alias: &str, chat_id: &str) -> anyhow::Result<()> {
    let conn = open_conn(alias).await?;
    let mut rows = conn.query(
        "SELECT content FROM memory WHERE key = 'conversation' AND context_jid = ?1 ORDER BY created_at DESC LIMIT 1",
        turso::params::Params::Positional(vec![turso::Value::from(chat_id)]),
    ).await?;

    let result = match rows.next().await? {
        Some(row) => {
            let json_str = text_val(row.get_value(0));
            serde_json::from_str::<serde_json::Value>(&json_str).unwrap_or(serde_json::json!({"error": "parse failed"}))
        }
        None => serde_json::json!({"notes": "", "topics": [], "pending": []}),
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

async fn set(alias: &str, chat_id: &str, json_str: &str) -> anyhow::Result<()> {
    let config = Config::load();
    let db_path = config.data_dir().join("accounts").join(format!("{alias}.db"));
    if !db_path.exists() {
        anyhow::bail!("No database found for account '{alias}'");
    }
    let db = turso::Builder::new_local(db_path.to_str().unwrap()).build().await?;
    let conn = db.connect()?;

    let parsed: serde_json::Value = serde_json::from_str(json_str)?;
    let canonical = serde_json::to_string(&parsed)?;

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "INSERT INTO memory (id, key, content, context_jid, importance, created_at)
         VALUES (?1, 'conversation', ?2, ?3, 1.0, ?4)",
        turso::params::Params::Positional(vec![
            turso::Value::from(id.as_str()),
            turso::Value::from(canonical.as_str()),
            turso::Value::from(chat_id),
            turso::Value::from(now),
        ]),
    ).await?;
    println!(r#"{{"success":true,"account":"{alias}","chat_id":"{chat_id}"}}"#);
    Ok(())
}

async fn search(alias: &str, query: &str) -> anyhow::Result<()> {
    let config = Config::load();
    let provider_cfg = config.agent.as_ref().and_then(|a| a.embedding_provider.as_ref());
    let api_key = config.agent.as_ref().and_then(|a| a.embedding_api_key.as_ref());

    if let (Some(provider), Some(key)) = (provider_cfg, api_key) {
        let model = config.agent.as_ref().and_then(|a| a.embedding_model.as_deref()).unwrap_or("text-embedding-3-small");
        let dims = config.agent.as_ref().and_then(|a| a.embedding_dimensions).unwrap_or(1536);
        let embedder = waos_lib::vector_memory::create_embedding_provider(provider, key, model, dims);
        let vm = waos_lib::vector_memory::VectorMemory::new(embedder);
        let conn = open_conn(alias).await?;
        vm.register_account(alias, conn).await;

        let results = vm.search_similar(alias, query, 10).await?;
        println!("{}", serde_json::to_string_pretty(&results)?);
    } else {
        let conn = open_conn(alias).await?;
        let pattern = format!("%{}%", query);
        let mut rows = conn.query(
            "SELECT context_jid, content FROM memory
             WHERE key = 'conversation' AND content LIKE ?1
             GROUP BY context_jid LIMIT 10",
            turso::params::Params::Positional(vec![turso::Value::from(pattern.as_str())]),
        ).await?;
        let mut results = Vec::new();
        while let Some(row) = rows.next().await? {
            results.push(serde_json::json!({
                "chat_id": text_val(row.get_value(0)),
                "memory": text_val(row.get_value(1)),
            }));
        }
        println!("{}", serde_json::to_string_pretty(&results)?);
    }
    Ok(())
}
