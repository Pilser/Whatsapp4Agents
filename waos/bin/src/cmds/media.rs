use clap::Subcommand;
use std::path::PathBuf;

use waos_lib::config::Config;

#[derive(Subcommand)]
pub enum MediaCommand {
    Send {
        chat_id: String,
        file_path: String,
        /// Optional text caption to accompany the media
        #[arg(long)]
        text: Option<String>,
        /// Reply to a specific message by ID
        #[arg(long)]
        reply_to: Option<String>,
        /// Force send as document instead of auto-detecting image/video/audio
        #[arg(long)]
        as_document: bool,
        /// Block and wait up to N seconds for send confirmation
        #[arg(long)]
        wait: Option<u64>,
    },
    Download {
        message_id: String,
    },
}

pub async fn run(alias: &str, cmd: MediaCommand) -> anyhow::Result<()> {
    match cmd {
        MediaCommand::Send { chat_id, file_path, text, reply_to, as_document, .. } => cmd_media_send(alias, &chat_id, &file_path, text.as_deref(), reply_to.as_deref(), as_document).await,
        MediaCommand::Download { message_id } => cmd_media_download(alias, &message_id).await,
    }
}

async fn cmd_media_send(alias: &str, chat_id: &str, file_path: &str, caption: Option<&str>, reply_to: Option<&str>, as_document: bool) -> anyhow::Result<()> {
    let path = PathBuf::from(file_path);
    if !path.exists() {
        println!(r#"{{"success":false,"error":"File not found: {file_path}"}}"#);
        return Ok(());
    }

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

    let ext = path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin")
        .to_lowercase();

    let media_type = if as_document {
        "document"
    } else {
        match ext.as_str() {
            "jpg" | "jpeg" | "png" | "gif" | "webp" => "image",
            "mp4" | "mov" | "avi" | "mkv" => "video",
            "mp3" | "ogg" | "wav" | "opus" => "audio",
            _ => "document",
        }
    };

    let mime = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "mp4" => "video/mp4",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "opus" => "audio/ogg; codecs=opus",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    };

    let caption_text = caption.unwrap_or("");
    conn.execute(
        "INSERT INTO outbound_queue (id, chat_id, content, media_path, media_mime, quoted_message_id, media_type, status, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', ?8)",
        turso::params::Params::Positional(vec![
            turso::Value::from(id.as_str()),
            turso::Value::from(chat_id),
            turso::Value::from(caption_text),
            turso::Value::from(file_path),
            turso::Value::from(mime),
            turso::Value::from(reply_to.unwrap_or("")),
            turso::Value::from(media_type),
            turso::Value::from(chrono::Utc::now().timestamp()),
        ]),
    ).await?;

    println!(r#"{{"success":true,"account":"{alias}","queue_id":"{id}","action":"media_send","media_type":"{media_type}","as_document":{as_document},"file":"{file_path}"}}"#);
    Ok(())
}

async fn cmd_media_download(alias: &str, message_id: &str) -> anyhow::Result<()> {
    let config = Config::load();
    let data_dir = config.data_dir();
    let cache_dir = data_dir.join("cache");
    tokio::fs::create_dir_all(&cache_dir).await?;

    let db_path = data_dir.join("accounts").join(format!("{alias}.db"));
    if !db_path.exists() {
        println!("No database found for account '{alias}'");
        return Ok(());
    }

    let db = turso::Builder::new_local(db_path.to_str().unwrap()).build().await?;
    let conn = db.connect()?;

    let mut rows = conn.query(
        "SELECT chat_id, media_path, media_mime FROM messages WHERE id = ?1 AND has_media = 1",
        turso::params::Params::Positional(vec![turso::Value::from(message_id)]),
    ).await?;

    if let Some(row) = rows.next().await? {
        let chat_id = match row.get_value(0) { Ok(turso::Value::Text(s)) => s, _ => String::new() };
        let media_path = match row.get_value(1) { Ok(turso::Value::Text(s)) => s, _ => String::new() };
        let _media_mime = match row.get_value(2) { Ok(turso::Value::Text(s)) => s, _ => String::new() };

        println!(r#"{{"success":true,"account":"{alias}","message_id":"{message_id}","chat_id":"{chat_id}","media_path":"{media_path}","note":"Download not yet available via CLI; use daemon media download feature"}}"#);
    } else {
        println!(r#"{{"success":false,"error":"No media message found with id '{message_id}'"}}"#);
    }
    Ok(())
}
