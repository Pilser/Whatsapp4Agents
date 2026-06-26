use clap::Subcommand;

use waos_lib::config::Config;

fn text_val(v: Result<turso::Value, turso::Error>) -> String {
    match v { Ok(turso::Value::Text(s)) => s, _ => String::new() }
}

fn int_val(v: Result<turso::Value, turso::Error>) -> i64 {
    match v { Ok(turso::Value::Integer(n)) => n, _ => 0 }
}

async fn open_main_conn() -> anyhow::Result<turso::Connection> {
    let config = Config::load();
    let db_path = config.data_dir().join("waos.db");
    if !db_path.exists() {
        anyhow::bail!("No main database found");
    }
    let db = turso::Builder::new_local(db_path.to_str().unwrap()).build().await?;
    Ok(db.connect()?)
}

#[derive(Subcommand)]
pub enum TaskCommand {
    List {
        #[arg(long)]
        status: Option<String>,
    },
    Complete {
        task_id: usize,
    },
}

pub async fn run(cmd: TaskCommand) -> anyhow::Result<()> {
    match cmd {
        TaskCommand::List { status } => list(status.as_deref()).await,
        TaskCommand::Complete { task_id } => complete(task_id).await,
    }
}

async fn list(status: Option<&str>) -> anyhow::Result<()> {
    let conn = open_main_conn().await?;
    let (sql, params) = match status {
        Some(s) => (
            "SELECT id, account, action, params, status, created_at FROM tasks WHERE status = ?1 ORDER BY created_at DESC".to_string(),
            turso::params::Params::Positional(vec![turso::Value::from(s)]),
        ),
        None => (
            "SELECT id, account, action, params, status, created_at FROM tasks ORDER BY created_at DESC".to_string(),
            turso::params::Params::Positional(vec![]),
        ),
    };
    let mut rows = conn.query(&sql, params).await?;
    let mut results = Vec::new();
    while let Some(row) = rows.next().await? {
        results.push(serde_json::json!({
            "id": int_val(row.get_value(0)),
            "account": text_val(row.get_value(1)),
            "action": text_val(row.get_value(2)),
            "params": text_val(row.get_value(3)),
            "status": text_val(row.get_value(4)),
            "created_at": int_val(row.get_value(5)),
        }));
    }
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}

async fn complete(task_id: usize) -> anyhow::Result<()> {
    let conn = open_main_conn().await?;
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "UPDATE tasks SET status = 'completed', updated_at = ?1 WHERE id = ?2",
        turso::params::Params::Positional(vec![
            turso::Value::from(now),
            turso::Value::from(task_id as i64),
        ]),
    ).await?;
    println!(r#"{{"success":true,"task_id":{task_id}}}"#);
    Ok(())
}
