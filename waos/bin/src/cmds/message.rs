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

#[derive(Subcommand)]
pub enum MessageCommand {
    Send {
        chat_id: String,
        text: String,
        /// Block and wait up to N seconds for send confirmation
        #[arg(long)]
        wait: Option<u64>,
    },
    Reply {
        chat_id: String,
        reply_to_id: String,
        text: String,
        /// Block and wait up to N seconds for send confirmation
        #[arg(long)]
        wait: Option<u64>,
    },
    React {
        chat_id: String,
        message_id: String,
        emoji: String,
        /// Block and wait up to N seconds for send confirmation
        #[arg(long)]
        wait: Option<u64>,
    },
    Edit {
        chat_id: String,
        message_id: String,
        text: String,
        /// Block and wait up to N seconds for send confirmation
        #[arg(long)]
        wait: Option<u64>,
    },
    Delete {
        chat_id: String,
        message_id: String,
        /// Block and wait up to N seconds for send confirmation
        #[arg(long)]
        wait: Option<u64>,
    },
}

pub async fn run(alias: &str, cmd: MessageCommand) -> anyhow::Result<()> {
    match cmd {
        MessageCommand::Send { chat_id, text, .. } => cmd_send(alias, &chat_id, &text).await,
        MessageCommand::Reply { chat_id, reply_to_id, text, .. } => cmd_reply(alias, &chat_id, &reply_to_id, &text).await,
        MessageCommand::React { chat_id, message_id, emoji, .. } => cmd_react(alias, &chat_id, &message_id, &emoji).await,
        MessageCommand::Edit { chat_id, message_id, text, .. } => cmd_edit(alias, &chat_id, &message_id, &text).await,
        MessageCommand::Delete { chat_id, message_id, .. } => cmd_delete(alias, &chat_id, &message_id).await,
    }
}

async fn cmd_send(alias: &str, chat_id: &str, text: &str) -> anyhow::Result<()> {
    let config = Config::load();
    let data_dir = config.data_dir();
    let db_path = data_dir.join("accounts").join(format!("{alias}.db"));

    if !db_path.exists() {
        println!("No database found for account '{alias}'");
        return Ok(());
    }

    let db = turso::Builder::new_local(db_path.to_str().unwrap()).build().await?;
    let conn = db.connect()?;
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO outbound_queue (id, chat_id, content, status, created_at)
         VALUES (?1, ?2, ?3, 'pending', ?4)",
        turso::params::Params::Positional(vec![
            turso::Value::from(id.as_str()),
            turso::Value::from(chat_id),
            turso::Value::from(text),
            turso::Value::from(chrono::Utc::now().timestamp()),
        ]),
    ).await?;
    println!(r#"{{"success":true,"account":"{alias}","queue_id":"{id}","action":"send"}}"#);
    Ok(())
}

async fn cmd_reply(alias: &str, chat_id: &str, reply_to_id: &str, text: &str) -> anyhow::Result<()> {
    let config = Config::load();
    let data_dir = config.data_dir();
    let db_path = data_dir.join("accounts").join(format!("{alias}.db"));

    if !db_path.exists() {
        println!("No database found for account '{alias}'");
        return Ok(());
    }

    let db = turso::Builder::new_local(db_path.to_str().unwrap()).build().await?;
    let conn = db.connect()?;
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO outbound_queue (id, chat_id, content, quoted_message_id, status, created_at)
         VALUES (?1, ?2, ?3, ?4, 'pending', ?5)",
        turso::params::Params::Positional(vec![
            turso::Value::from(id.as_str()),
            turso::Value::from(chat_id),
            turso::Value::from(text),
            turso::Value::from(reply_to_id),
            turso::Value::from(chrono::Utc::now().timestamp()),
        ]),
    ).await?;
    println!(r#"{{"success":true,"account":"{alias}","queue_id":"{id}","action":"reply"}}"#);
    Ok(())
}

async fn cmd_react(alias: &str, chat_id: &str, message_id: &str, emoji: &str) -> anyhow::Result<()> {
    let config = Config::load();
    let data_dir = config.data_dir();
    let db_path = data_dir.join("accounts").join(format!("{alias}.db"));

    if !db_path.exists() {
        println!("No database found for account '{alias}'");
        return Ok(());
    }

    let db = turso::Builder::new_local(db_path.to_str().unwrap()).build().await?;
    let conn = db.connect()?;
    let id = uuid::Uuid::new_v4().to_string();
    let content = format!("REACTION:{message_id}:{emoji}");
    conn.execute(
        "INSERT INTO outbound_queue (id, chat_id, content, status, created_at)
         VALUES (?1, ?2, ?3, 'pending', ?4)",
        turso::params::Params::Positional(vec![
            turso::Value::from(id.as_str()),
            turso::Value::from(chat_id),
            turso::Value::from(content.as_str()),
            turso::Value::from(chrono::Utc::now().timestamp()),
        ]),
    ).await?;
    println!(r#"{{"success":true,"account":"{alias}","queue_id":"{id}","action":"react"}}"#);
    Ok(())
}

async fn cmd_edit(alias: &str, chat_id: &str, message_id: &str, text: &str) -> anyhow::Result<()> {
    let config = Config::load();
    let data_dir = config.data_dir();
    let db_path = data_dir.join("accounts").join(format!("{alias}.db"));

    if !db_path.exists() {
        println!("No database found for account '{alias}'");
        return Ok(());
    }

    let db = turso::Builder::new_local(db_path.to_str().unwrap()).build().await?;
    let conn = db.connect()?;
    let id = uuid::Uuid::new_v4().to_string();
    let content = format!("EDIT:{message_id}:{text}");
    conn.execute(
        "INSERT INTO outbound_queue (id, chat_id, content, status, created_at)
         VALUES (?1, ?2, ?3, 'pending', ?4)",
        turso::params::Params::Positional(vec![
            turso::Value::from(id.as_str()),
            turso::Value::from(chat_id),
            turso::Value::from(content.as_str()),
            turso::Value::from(chrono::Utc::now().timestamp()),
        ]),
    ).await?;
    println!(r#"{{"success":true,"account":"{alias}","queue_id":"{id}","action":"edit"}}"#);
    Ok(())
}

async fn cmd_delete(alias: &str, chat_id: &str, message_id: &str) -> anyhow::Result<()> {
    let config = Config::load();
    let data_dir = config.data_dir();
    let db_path = data_dir.join("accounts").join(format!("{alias}.db"));

    if !db_path.exists() {
        println!("No database found for account '{alias}'");
        return Ok(());
    }

    let db = turso::Builder::new_local(db_path.to_str().unwrap()).build().await?;
    let conn = db.connect()?;
    let id = uuid::Uuid::new_v4().to_string();
    let content = format!("DELETE:{message_id}");
    conn.execute(
        "INSERT INTO outbound_queue (id, chat_id, content, status, created_at)
         VALUES (?1, ?2, ?3, 'pending', ?4)",
        turso::params::Params::Positional(vec![
            turso::Value::from(id.as_str()),
            turso::Value::from(chat_id),
            turso::Value::from(content.as_str()),
            turso::Value::from(chrono::Utc::now().timestamp()),
        ]),
    ).await?;
    println!(r#"{{"success":true,"account":"{alias}","queue_id":"{id}","action":"delete"}}"#);
    Ok(())
}
