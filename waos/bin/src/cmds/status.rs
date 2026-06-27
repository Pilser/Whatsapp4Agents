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
pub enum StatusCommand {
    /// List all accounts and their connection status
    List {
        /// Block and wait up to N seconds for result (--wait alone defaults to 30)
        #[arg(long, default_missing_value = "30", num_args = 0..=1)]
        wait: Option<u64>,
    },
    /// Post a text status update (story) visible to all contacts
    SendText {
        #[arg(long)]
        account: String,
        text: String,
        /// Background color as 0xAARRGGBB (e.g., 0xFF1E6E4F)
        #[arg(long, default_value = "0xFF1E6E4F")]
        bg_color: String,
        /// Font style index (0-4)
        #[arg(long, default_value = "0")]
        font: i32,
        /// Block and wait up to N seconds for send confirmation (--wait alone defaults to 30)
        #[arg(long, default_missing_value = "30", num_args = 0..=1)]
        wait: Option<u64>,
    },
    /// Post an image status update (story) visible to all contacts
    SendImage {
        #[arg(long)]
        account: String,
        file_path: String,
        /// Path to a JPEG thumbnail file (required). Generate via external tools (Python/Pillow, ffmpeg, etc.)
        #[arg(long)]
        thumbnail_file: String,
        /// Optional caption
        #[arg(long)]
        caption: Option<String>,
        /// Block and wait up to N seconds for send confirmation (--wait alone defaults to 30)
        #[arg(long, default_missing_value = "30", num_args = 0..=1)]
        wait: Option<u64>,
    },
    /// Post a video status update (story) visible to all contacts
    SendVideo {
        #[arg(long)]
        account: String,
        file_path: String,
        /// Path to a JPEG thumbnail file (required). Generate via external tools (Python/Pillow, ffmpeg, etc.)
        #[arg(long)]
        thumbnail_file: String,
        /// Optional caption
        #[arg(long)]
        caption: Option<String>,
        /// Block and wait up to N seconds for send confirmation
        #[arg(long)]
        wait: Option<u64>,
    },
    /// Revoke a previously posted status update
    Revoke {
        #[arg(long)]
        account: String,
        message_id: String,
        /// Block and wait up to N seconds for send confirmation (--wait alone defaults to 30)
        #[arg(long, default_missing_value = "30", num_args = 0..=1)]
        wait: Option<u64>,
    },
    /// List posted status history with date range and pagination
    History {
        #[arg(long)]
        account: String,
        /// Filter by status: posted, revoked, expired, all (default: all)
        #[arg(long, default_value = "all")]
        status: String,
        /// Only show statuses older than this timestamp
        #[arg(long)]
        before: Option<i64>,
        /// Only show statuses newer than this timestamp
        #[arg(long)]
        after: Option<i64>,
        /// Maximum results (default: 5)
        #[arg(long, default_value = "5")]
        limit: usize,
        /// Skip N results for pagination
        #[arg(long, default_value = "0")]
        offset: usize,
        /// Block and wait up to N seconds for result (--wait alone defaults to 30)
        #[arg(long, default_missing_value = "30", num_args = 0..=1)]
        wait: Option<u64>,
    },
    /// Delete status history entries by date range (DB cleanup, does NOT revoke from WhatsApp)
    Delete {
        #[arg(long)]
        account: String,
        /// Delete entries older than this timestamp
        #[arg(long)]
        older_than: Option<i64>,
        /// Delete entries newer than this timestamp
        #[arg(long)]
        newer_than: Option<i64>,
        /// Filter by status: posted, revoked, expired, all (default: all)
        #[arg(long, default_value = "all")]
        status: String,
        /// If true, actually delete (dry-run by default)
        #[arg(long)]
        yes: bool,
        /// Block and wait up to N seconds for result (--wait alone defaults to 30)
        #[arg(long, default_missing_value = "30", num_args = 0..=1)]
        wait: Option<u64>,
    },
}

pub async fn run() -> anyhow::Result<()> {
    let config = Config::load();
    let data_dir = config.data_dir();
    let db_path = data_dir.join("waos.db");

    if !db_path.exists() {
        println!("WAOS not initialized. No database found at {}", db_path.display());
        return Ok(());
    }

    let db = turso::Builder::new_local(db_path.to_str().unwrap())
        .build()
        .await?;
    let conn = db.connect()?;

    let mut rows = conn
        .query("SELECT alias, phone, status, error, updated_at FROM accounts", ())
        .await?;

    println!("Accounts:");
    println!("{:-<85}", "");
    while let Some(row) = rows.next().await? {
        let alias = match row.get_value(0)? {
            turso::Value::Text(s) => s,
            _ => "?".to_string(),
        };
        let phone = match row.get_value(1)? {
            turso::Value::Text(s) => s,
            _ => "-".to_string(),
        };
        let status = match row.get_value(2)? {
            turso::Value::Text(s) => s,
            _ => "unknown".to_string(),
        };
        let error = match row.get_value(3)? {
            turso::Value::Text(s) => s,
            _ => String::new(),
        };
        let updated = match row.get_value(4)? {
            turso::Value::Integer(n) => format_timestamp(n as i64),
            _ => "-".to_string(),
        };

        if error.is_empty() {
            println!("  {alias:<15} {phone:<15} {status:<15} {updated}");
        } else {
            println!("  {alias:<15} {phone:<15} {status:<15} {error:<25} {updated}");
        }
    }

    Ok(())
}

pub async fn run_cmd(account: &str, cmd: &StatusCommand) -> anyhow::Result<()> {
    match cmd {
        StatusCommand::List { .. } => run().await,
        StatusCommand::SendText { text, bg_color, font, .. } => {
            cmd_send_text(account, text, bg_color, *font).await
        }
        StatusCommand::SendImage { file_path, thumbnail_file, caption, .. } => {
            cmd_send_media(account, file_path, thumbnail_file, caption.as_deref(), "status_image").await
        }
        StatusCommand::SendVideo { file_path, thumbnail_file, caption, .. } => {
            cmd_send_media(account, file_path, thumbnail_file, caption.as_deref(), "status_video").await
        }
        StatusCommand::Revoke { message_id, .. } => {
            cmd_revoke(account, message_id).await
        }
        StatusCommand::History { status: status_filter, before, after, limit, offset, .. } => {
            cmd_history(account, status_filter, *before, *after, *limit, *offset).await
        }
        StatusCommand::Delete { older_than, newer_than, status: status_filter, yes, .. } => {
            cmd_delete(account, *older_than, *newer_than, status_filter, *yes).await
        }
    }
}

async fn cmd_send_text(alias: &str, text: &str, bg_color: &str, font: i32) -> anyhow::Result<()> {
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
    let metadata = serde_json::json!({"bg": bg_color, "font": font}).to_string();
    conn.execute(
        "INSERT INTO outbound_queue (id, chat_id, content, media_type, metadata, status, created_at)
         VALUES (?1, 'status@broadcast', ?2, 'status_text', ?3, 'pending', ?4)",
        turso::params::Params::Positional(vec![
            turso::Value::from(id.as_str()),
            turso::Value::from(text),
            turso::Value::from(metadata.as_str()),
            turso::Value::from(chrono::Utc::now().timestamp()),
        ]),
    ).await?;
    println!(r#"{{"success":true,"account":"{alias}","queue_id":"{id}","action":"status_send_text"}}"#);
    Ok(())
}

async fn cmd_send_media(alias: &str, file_path: &str, thumbnail_file: &str, caption: Option<&str>, media_type: &str) -> anyhow::Result<()> {
    let path = std::path::PathBuf::from(file_path);
    if !path.exists() {
        println!(r#"{{"success":false,"error":"File not found: {file_path}"}}"#);
        return Ok(());
    }

    let thumb_path = std::path::PathBuf::from(thumbnail_file);
    if !thumb_path.exists() {
        println!(r#"{{"success":false,"error":"Thumbnail file not found: {thumbnail_file}. Generate a JPEG thumbnail using external tools (Python with Pillow, ffmpeg, etc.) and pass via --thumbnail-file"}}"#);
        return Ok(());
    }

    let media_kind = if media_type == "status_video" { "video" } else { "image" };
    if let Err(e) = waos_lib::media::check_status_file_size(file_path, media_kind) {
        println!(r#"{{"success":false,"error":"{}"}}"#, e.to_string().replace('"', "'"));
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
    let caption_text = caption.unwrap_or("");
    let metadata = serde_json::json!({"thumbnail": thumbnail_file}).to_string();
    conn.execute(
        "INSERT INTO outbound_queue (id, chat_id, content, media_path, media_type, metadata, status, created_at)
         VALUES (?1, 'status@broadcast', ?2, ?3, ?4, ?5, 'pending', ?6)",
        turso::params::Params::Positional(vec![
            turso::Value::from(id.as_str()),
            turso::Value::from(caption_text),
            turso::Value::from(file_path),
            turso::Value::from(media_type),
            turso::Value::from(metadata.as_str()),
            turso::Value::from(chrono::Utc::now().timestamp()),
        ]),
    ).await?;
    println!(r#"{{"success":true,"account":"{alias}","queue_id":"{id}","action":"{media_type}"}}"#);
    Ok(())
}

async fn cmd_revoke(alias: &str, message_id: &str) -> anyhow::Result<()> {
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
        "INSERT INTO outbound_queue (id, chat_id, content, media_type, status, created_at)
         VALUES (?1, 'status@broadcast', ?2, 'status_revoke', 'pending', ?3)",
        turso::params::Params::Positional(vec![
            turso::Value::from(id.as_str()),
            turso::Value::from(message_id),
            turso::Value::from(chrono::Utc::now().timestamp()),
        ]),
    ).await?;
    println!(r#"{{"success":true,"account":"{alias}","queue_id":"{id}","action":"status_revoke"}}"#);
    Ok(())
}

async fn cmd_history(alias: &str, status_filter: &str, before: Option<i64>, after: Option<i64>, limit: usize, offset: usize) -> anyhow::Result<()> {
    let config = Config::load();
    let data_dir = config.data_dir();
    let db_path = data_dir.join("accounts").join(format!("{alias}.db"));
    if !db_path.exists() {
        println!("No database found for account '{alias}'");
        return Ok(());
    }
    let db = turso::Builder::new_local(db_path.to_str().unwrap()).build().await?;
    let conn = db.connect()?;

    let now = chrono::Utc::now().timestamp();
    let mut conditions: Vec<String> = Vec::new();
    let mut params: Vec<turso::Value> = Vec::new();

    if status_filter != "all" && status_filter != "expired" {
        conditions.push(format!("status = ?{}", params.len() + 1));
        params.push(turso::Value::from(status_filter));
    }
    if let Some(ts) = before {
        conditions.push(format!("created_at < ?{}", params.len() + 1));
        params.push(turso::Value::from(ts));
    }
    if let Some(ts) = after {
        conditions.push(format!("created_at > ?{}", params.len() + 1));
        params.push(turso::Value::from(ts));
    }
    let where_clause = if conditions.is_empty() { String::new() } else { format!("WHERE {}", conditions.join(" AND ")) };

    // Count total matching
    let count_sql = format!("SELECT COUNT(*) FROM status_history {}", where_clause);
    let mut count_rows = conn.query(&count_sql, turso::params::Params::Positional(params.clone())).await?;
    let total: i64 = match count_rows.next().await {
        Ok(Some(row)) => match row.get_value(0) { Ok(turso::Value::Integer(n)) => n, _ => 0 },
        _ => 0,
    };

    // Fetch with pagination
    let sql = format!(
        "SELECT id, media_type, content, media_path, thumbnail_path, bg_color, font, message_id, status, created_at, updated_at
         FROM status_history {} ORDER BY created_at DESC LIMIT ?{} OFFSET ?{}",
        where_clause,
        params.len() + 1,
        params.len() + 2,
    );
    params.push(turso::Value::from(limit as i64));
    params.push(turso::Value::from(offset as i64));

    let mut rows = conn.query(&sql, turso::params::Params::Positional(params)).await?;

    let mut results = Vec::new();
    while let Ok(Some(row)) = rows.next().await {
        let id = text_val(row.get_value(0));
        let media_type = text_val(row.get_value(1));
        let content = text_val(row.get_value(2));
        let media_path = text_val(row.get_value(3));
        let thumbnail_path = text_val(row.get_value(4));
        let bg_color = text_val(row.get_value(5));
        let font = match row.get_value(6) { Ok(turso::Value::Integer(n)) => n, _ => 0 };
        let message_id = text_val(row.get_value(7));
        let status = text_val(row.get_value(8));
        let created_at = match row.get_value(9) { Ok(turso::Value::Integer(n)) => n, _ => 0 };
        let _updated_at = match row.get_value(10) { Ok(turso::Value::Integer(n)) => n, _ => 0 };

        let is_expired = status == "posted" && created_at + 86400 < now;
        if status_filter == "expired" && !is_expired { continue; }

        let mut entry = serde_json::json!({
            "id": id,
            "media_type": media_type,
            "content": content,
            "status": status,
            "is_expired": is_expired,
            "created_at": created_at,
        });
        if !media_path.is_empty() { entry["media_path"] = serde_json::json!(media_path); }
        if !thumbnail_path.is_empty() { entry["thumbnail_path"] = serde_json::json!(thumbnail_path); }
        if !bg_color.is_empty() { entry["bg_color"] = serde_json::json!(bg_color); }
        if font != 0 { entry["font"] = serde_json::json!(font); }
        if !message_id.is_empty() { entry["message_id"] = serde_json::json!(message_id); }

        results.push(entry);
    }

    println!(r#"{{"success":true,"account":"{alias}","statuses":{},"total":{},"limit":{},"offset":{}}}"#,
        serde_json::to_string(&results).unwrap_or_default(),
        total, limit, offset);
    Ok(())
}

async fn cmd_delete(alias: &str, older_than: Option<i64>, newer_than: Option<i64>, status_filter: &str, yes: bool) -> anyhow::Result<()> {
    let config = Config::load();
    let data_dir = config.data_dir();
    let db_path = data_dir.join("accounts").join(format!("{alias}.db"));
    if !db_path.exists() {
        println!("No database found for account '{alias}'");
        return Ok(());
    }
    let db = turso::Builder::new_local(db_path.to_str().unwrap()).build().await?;
    let conn = db.connect()?;

    let now = chrono::Utc::now().timestamp();
    let mut conditions: Vec<String> = Vec::new();
    let mut params: Vec<turso::Value> = Vec::new();

    match status_filter {
        "posted" | "revoked" => {
            conditions.push(format!("status = ?{}", params.len() + 1));
            params.push(turso::Value::from(status_filter));
        }
        "expired" => {
            conditions.push(format!("status = 'posted' AND created_at < ?{}", params.len() + 1));
            params.push(turso::Value::from(now - 86400));
        }
        "all" => {}
        _ => {
            println!(r#"{{"success":false,"error":"Invalid status filter '{}'. Use: posted, revoked, expired, all"}}"#, status_filter);
            return Ok(());
        }
    }
    if let Some(ts) = older_than {
        conditions.push(format!("created_at < ?{}", params.len() + 1));
        params.push(turso::Value::from(ts));
    }
    if let Some(ts) = newer_than {
        conditions.push(format!("created_at > ?{}", params.len() + 1));
        params.push(turso::Value::from(ts));
    }

    let where_clause = if conditions.is_empty() { String::new() } else { format!("WHERE {}", conditions.join(" AND ")) };

    // Count what will be deleted
    let count_sql = format!("SELECT COUNT(*) FROM status_history {}", where_clause);
    let mut count_rows = conn.query(&count_sql, turso::params::Params::Positional(params.clone())).await?;
    let total: i64 = match count_rows.next().await {
        Ok(Some(row)) => match row.get_value(0) { Ok(turso::Value::Integer(n)) => n, _ => 0 },
        _ => 0,
    };

    if !yes {
        let noun = if total == 1 { "entry" } else { "entries" };
        println!(r#"{{"success":true,"account":"{alias}","dry_run":true,"count":{},"message":"{} {} would be deleted. Run with --yes to confirm"}}"#, total, total, noun);
        return Ok(());
    }

    conn.execute(&format!("DELETE FROM status_history {}", where_clause), turso::params::Params::Positional(params)).await?;
    println!(r#"{{"success":true,"account":"{alias}","deleted":{}}}"#, total);
    Ok(())
}

fn format_timestamp(ts: i64) -> String {
    let dt = chrono::DateTime::from_timestamp(ts, 0)
        .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| ts.to_string());
    dt
}
