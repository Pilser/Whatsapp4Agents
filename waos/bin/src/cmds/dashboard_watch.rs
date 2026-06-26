use std::time::Duration;

use waos_lib::config::Config;

pub async fn run() -> anyhow::Result<()> {
    let config = Config::load();
    let delta_path = config.data_dir().join("dashboard.delta");

    let mut last_content = String::new();
    loop {
        if delta_path.exists() {
            let content = tokio::fs::read_to_string(&delta_path).await?;
            if content != last_content {
                println!("{}", content);
                last_content = content;
            }
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}
