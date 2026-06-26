use waos_lib::config::Config;

pub async fn run() -> anyhow::Result<()> {
    let config = Config::load();
    let dashboard_path = config.data_dir().join("dashboard.json");

    if !dashboard_path.exists() {
        println!("Dashboard file not found at {}", dashboard_path.display());
        return Ok(());
    }

    let content = tokio::fs::read_to_string(&dashboard_path).await?;
    println!("{content}");

    Ok(())
}
