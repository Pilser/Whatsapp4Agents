use std::path::PathBuf;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with_writer(std::io::stderr)
        .init();

    let args: Vec<String> = std::env::args().collect();
    let mcp_only = args.iter().any(|a| a == "--mcp");

    if mcp_only {
        let cache_dir = std::env::var("ZEROWRAPPER__CACHE_DIR")
            .unwrap_or_else(|_| "./cache".to_string());
        let cache_dir_path = PathBuf::from(&cache_dir);
        let cache = Arc::new(zw_cache::CacheManager::new(&cache_dir_path));

        let mcp = zw_mcp::McpRegistry::new();
        mcp.register_provider(Box::new(
            mcp_web_files_render::WebFilesRenderProvider::new(cache.clone())
        )).await;
        mcp.register_provider(Box::new(
            mcp_web_files_render::GetFilePathProvider::new(cache)
        )).await;

        tracing::info!("MCP-only mode — cache_dir={}", cache_dir);
        mcp.run_stdio().await?;
        return Ok(());
    }

    let config = zw_core::WrapperConfig::load()?;
    let cache = Arc::new(zw_cache::CacheManager::new(&config.cache_dir));

    let mcp = zw_mcp::McpRegistry::new();
    mcp.register_provider(Box::new(
        mcp_web_files_render::WebFilesRenderProvider::new(cache.clone())
    )).await;
    mcp.register_provider(Box::new(
        mcp_web_files_render::GetFilePathProvider::new(cache.clone())
    )).await;

    if config.mcp.is_none() {
        tracing::info!("No [zerowrapper.mcp] configured (only needed if zeroclaw daemon needs to discover this server)");
    }

    tracing::info!("Config loaded: zeroclaw_socket={}, db={}, web_channel={{{}}}",
        config.zeroclaw_socket, config.db_path, config.ch_web.bind);

    let db = zw_db::DbPool::open(&config.db_path).await?;
    db.migrate().await?;
    tracing::info!("Database migrated");

    if !config.ch_web.accounts.is_empty() {
        let seeds: std::collections::HashMap<String, zw_db::AccountSeed> = config.ch_web.accounts.iter().map(|(k, v)| {
            (k.clone(), zw_db::AccountSeed { role: v.role.clone(), password: v.password.clone() })
        }).collect();
        db.seed_accounts(&seeds).await?;
    }

    let zeroclaw = zw_channels::ZeroClawBridge::new(&config.zeroclaw_socket, db.clone());
    let mut channels = zw_channels::ChannelRegistry::new(zeroclaw.clone());

    channels.register(Box::new(
        ch_web::WebChannel::new(&config.ch_web.bind, &config.ch_web.agent, cache.clone(), config.ch_web.dist_path.clone(), config.ch_web.upload_dir.clone(), db.clone())
    ));

    let registry_handle = channels.registry_handle();
    let zeroclaw_handle = Arc::new(channels.zeroclaw_handle());

    let ui_ctx = ch_web_ui::context::AppContext::new(
        registry_handle,
        zeroclaw_handle,
        db.clone(),
        cache.clone(),
        &config.ch_web.agent,
        config.ch_web.upload_dir.clone(),
    );
    let ui_bind = config.ch_web.ui_bind();
    let ui_handle = tokio::spawn(async move {
        if let Err(e) = ch_web_ui::app::start_on(ui_bind, ui_ctx).await {
            tracing::error!("ch-web-ui exited: {}", e);
        }
    });

    let zc_handle = tokio::spawn(async move {
        if let Err(e) = zeroclaw.run().await {
            tracing::error!("ZeroClaw bridge exited: {}", e);
        }
    });

    let ch_handle = tokio::spawn(async move {
        if let Err(e) = channels.run().await {
            tracing::error!("Channel registry exited: {}", e);
        }
    });

    tracing::info!("Starting MCP stdio loop");
    mcp.run_stdio().await?;

    let _ = zc_handle.await;
    let _ = ch_handle.await;
    let _ = ui_handle.await;

    Ok(())
}
