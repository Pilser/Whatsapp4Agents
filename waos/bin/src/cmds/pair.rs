use waos_lib::config::Config;

pub async fn run(alias: &str) -> anyhow::Result<()> {
    let config = Config::load();
    let data_dir = config.data_dir();
    let accounts_dir = data_dir.join("accounts");

    tokio::fs::create_dir_all(&accounts_dir).await?;

    println!("Starting pairing for account '{alias}'...");
    println!("Session data will be stored in: {}/accounts/{alias}.db", data_dir.display());
    println!();
    println!("This will require a QR code scan. The daemon must be running.");
    println!();
    println!("Usage:");
    println!("  1. Start the daemon: waos daemon");
    println!("  2. Generate QR:       waos pair {alias}");
    println!("  3. Scan QR with WhatsApp mobile app");
    println!();

    Ok(())
}
