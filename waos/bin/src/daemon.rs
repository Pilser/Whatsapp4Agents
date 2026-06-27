use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::signal;
use tokio::sync::Mutex;
use tracing::{error, info, warn};
use turso::Connection;

use waos_core::events::WaosEvent;
use waos_db::{DbManager, wa_version};
use waos_lib::adapter::account_manager::AccountManager;
use waos_lib::attention::AttentionEngine;
use waos_lib::bridge::anti_ban::AntiBan;
use waos_lib::bridge::queue::OutboundQueue;
use waos_lib::config::Config;
use waos_lib::dashboard::DashboardWriter;
use waos_lib::event_bus::EventBus;
use waos_lib::graph::RelationshipGraph;
use waos_lib::health::HealthCollector;
use waos_lib::memory::MemoryStore;
use waos_lib::orchestration::Orchestrator;
use waos_lib::resource_monitor::ResourceMonitor;
use waos_lib::scoring::ScoringEngine;
use waos_lib::sync_engine::SyncEngine;
use waos_lib::vector_memory::{create_embedding_provider, VectorMemory};
use waos_lib::webhook::WebhookPush;

pub async fn run(port: u16) -> anyhow::Result<()> {
    let config = Config::load();
    let data_dir = config.data_dir();
    let accounts_dir = config.accounts_dir();
    let sessions_dir = config.sessions_dir();
    let media_dir = config.media_dir();
    let main_db_path = config.main_db();
    let dash_path = config.dashboard_path();
    let delta_path = config.dashboard_delta_path();

    tokio::fs::create_dir_all(&data_dir).await?;
    tokio::fs::create_dir_all(&accounts_dir).await?;
    tokio::fs::create_dir_all(&sessions_dir).await?;
    tokio::fs::create_dir_all(&media_dir).await?;

    let event_bus = EventBus::new(4096);
    let db_mgr = Arc::new(Mutex::new(
        DbManager::open(&main_db_path, &accounts_dir, &sessions_dir).await?
    ));

    // Resolve WA version: try CDN → fallback DB → fallback config
    let mut dev_cfg = config.device.clone();
    let main_conn = db_mgr.lock().await.open_main_conn()?;
    let fetch_result = tokio::task::spawn_blocking(waos_lib::version_fetch::fetch_wa_version).await;
    let version_source = match fetch_result {
        Ok(Ok(fetched)) => {
            let now = chrono::Utc::now().timestamp();
            wa_version::write_wa_version(
                &main_conn,
                &wa_version::WaVersion {
                    primary: fetched.primary,
                    secondary: fetched.secondary,
                    tertiary: fetched.tertiary,
                    fetched_at: now,
                },
            )
            .await;
            dev_cfg.version_primary = fetched.primary as u32;
            dev_cfg.version_secondary = fetched.secondary as u32;
            dev_cfg.version_tertiary = fetched.tertiary as u32;
            format!("{fetched}, fetched from CDN")
        }
        Ok(Err(fetch_err)) => {
            match wa_version::read_wa_version(&main_conn).await {
                Some(cached) => {
                    dev_cfg.version_primary = cached.primary as u32;
                    dev_cfg.version_secondary = cached.secondary as u32;
                    dev_cfg.version_tertiary = cached.tertiary as u32;
                    format!("{}.{}.{}, DB cache (fetch failed: {fetch_err})", cached.primary, cached.secondary, cached.tertiary)
                }
                None => {
                    format!("{}.{}.{}, config default (fetch failed: {fetch_err}, no cached version)",
                        dev_cfg.version_primary, dev_cfg.version_secondary, dev_cfg.version_tertiary)
                }
            }
        }
        Err(join_err) => {
            warn!("Version fetch task panicked: {join_err}, falling back to config default");
            format!("{}.{}.{}, config default (fetch panicked)", dev_cfg.version_primary, dev_cfg.version_secondary, dev_cfg.version_tertiary)
        }
    };
    drop(main_conn);

    info!("WAOS daemon starting, data dir: {}, version: {version_source}", data_dir.display());

    let account_manager = Arc::new(Mutex::new(AccountManager::new(dev_cfg)));

    // Resolve accounts from main DB. On first run (DB empty) this bootstraps
    // from config `[[accounts]]`. After that, all account management is
    // done via the REST API / main DB directly.
    let mut config = config;
    config.resolve_accounts(&db_mgr.lock().await.main_conn).await;

    // Phase 2 subsystems
    let mut dashboard = DashboardWriter::new(&dash_path, &delta_path);
    // Each background subsystem gets its own main DB connection to avoid
    // turso's ConcurrentGuard "concurrent use forbidden" error.
    dashboard.set_main_conn(db_mgr.lock().await.open_main_conn()?);
    let attention = AttentionEngine::new();

    // Phase 3 subsystems
    let anti_ban = Arc::new(AntiBan::new(
        db_mgr.lock().await.open_main_conn()?,
        config.anti_ban.hourly_cap,
        config.anti_ban.daily_cap,
        config.anti_ban.pacer_min_delay_ms,
        config.anti_ban.pacer_jitter_ms,
    ));
    let db_conns: Arc<tokio::sync::Mutex<HashMap<String, Connection>>> =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));
    let clients = account_manager.lock().await.clients.clone();
    let outbound_queue = Arc::new(OutboundQueue::new(
        db_conns.clone(),
        clients.clone(),
        anti_ban.clone(),
        event_bus.clone(),
        accounts_dir.clone(),
    ));

    // Phase 4+5 subsystems
    let scoring = waos_lib::scoring::ScoringEngine::new();
    let graph = waos_lib::graph::RelationshipGraph::new();
    let memory_store = waos_lib::memory::MemoryStore::new();

    let vector_memory = config.agent.as_ref().and_then(|a| {
        let provider = a.embedding_provider.as_ref()?;
        let api_key = a.embedding_api_key.as_ref()?;
        let model = a.embedding_model.as_deref().unwrap_or("text-embedding-3-small");
        let dims = a.embedding_dimensions.unwrap_or(1536);
        Some(VectorMemory::new(create_embedding_provider(provider, api_key, model, dims)))
    });

    let sync_engine = SyncEngine::new()
        .with_scoring(scoring.clone())
        .with_graph(graph.clone());

    let sync_engine = if let Some(ref vm) = vector_memory {
        sync_engine.with_vector_memory(vm.clone())
    } else {
        sync_engine
    };

    let orchestrator = Orchestrator::new(
        config.agent.as_ref().and_then(|a| a.webhook_url.clone()),
        config.agent.as_ref().and_then(|a| a.webhook_token.clone()),
    );

    // Wire sync_engine into AccountManager for HistorySync processing
    account_manager.lock().await.sync_engine = Some(sync_engine.clone());

    // Production hardening subsystems
    let resource_monitor = ResourceMonitor::new(
        media_dir.clone(),
        main_db_path.clone(),
        dash_path.clone(),
        delta_path.clone(),
    );
    let health_collector = HealthCollector::new(main_db_path.clone(), media_dir.clone());

    // Startup checks
    info!("Running startup checks...");
    resource_monitor.startup_checks().await;
    {
        let conns = db_conns.lock().await;
        resource_monitor.replay_outbound_queues(&conns).await;
    }
    if let Err(e) = resource_monitor.clean_media_cache().await {
        warn!("Media cache cleanup failed: {e}");
    }

    for acct in &config.accounts {
        let alias = &acct.alias;

        let conn = db_mgr.lock().await.open_account_db(alias).await?;

        // Open a separate connection for sync engine (HistorySync processing)
        // to avoid turso's ConcurrentGuard "concurrent use forbidden" error
        // when the bot and HistorySync try to use the same connection at once.
        let sync_conn = db_mgr.lock().await.open_account_db_fresh(alias).await?;
        sync_engine.register_account(alias, sync_conn).await;
        dashboard.register_account(alias, conn.clone()).await;
        attention.register_account(alias, conn.clone()).await;
        scoring.register_account(alias, conn.clone()).await;
        graph.register_account(alias, conn.clone()).await;
        memory_store.register_account(alias, conn.clone()).await;
        orchestrator.register_account(alias, conn.clone()).await;
        health_collector.register_account(alias, conn.clone()).await;
        if let Some(ref vm) = vector_memory {
            vm.register_account(alias, conn.clone()).await;
        }
        db_conns.lock().await.insert(alias.clone(), conn);

        // Dedicated HistorySync connection — separate from SyncEngine's pool
        // so HistorySync processing doesn't contend with event_processor
        // on the same turso ConcurrentGuard. Must be inserted BEFORE the
        // bot starts (add_account spawns run_bot).
        let history_conn = db_mgr.lock().await.open_account_db_fresh(alias).await?;
        account_manager
            .lock()
            .await
            .history_conns
            .lock()
            .await
            .insert(alias.clone(), history_conn);

        account_manager
            .lock()
            .await
            .add_account(alias, &sessions_dir, &event_bus)
            .await?;

        info!("Account '{}' initialized", alias);
    }

    let event_processor = {
        let bus = event_bus.clone();
        let sync = sync_engine.clone();
        let dash = dashboard.clone();
        let db = db_mgr.clone();
        tokio::spawn(async move {
            let mut rx = bus.subscribe();
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        sync.handle_event(&event, &bus).await;
                        if let Err(e) = dash.write_delta(&event).await {
                            error!("Dashboard delta write failed: {e}");
                        }
                        // Handle logged_out: clear all pending outbound messages
                        // and let the agent get immediate rejection on new sends.
                        if let WaosEvent::AccountStatusUpdate { account, status } = &event {
                            if status == "logged_out" {
                                if let Ok(conn) = db.lock().await.open_account_db_fresh(account).await {
                                    let deleted = conn.execute(
                                        "DELETE FROM outbound_queue WHERE status = 'pending'",
                                        (),
                                    ).await.unwrap_or(0);
                                    if deleted > 0 {
                                        info!("Cleared {deleted} pending messages for logged-out account '{account}'");
                                    }
                                }
                            }
                        }

                        // Sync live connection status to the main DB so the
                        // REST API returns accurate status without polling.
                        if let Some((alias, status, error)) = match &event {
                            WaosEvent::Connected { account } => {
                                Some((account.as_str(), "connected", ""))
                            }
                            WaosEvent::Disconnected { account } => {
                                Some((account.as_str(), "disconnected", ""))
                            }
                            WaosEvent::Reconnecting { account } => {
                                Some((account.as_str(), "reconnecting", ""))
                            }
                            WaosEvent::AccountStatusUpdate { account, status } => {
                                let err = if status.starts_with("pair_error:") {
                                    status.strip_prefix("pair_error:").unwrap_or("")
                                } else if status.starts_with("connect_failure:") {
                                    status.strip_prefix("connect_failure:").unwrap_or("")
                                } else { "" };
                                Some((account.as_str(), status.as_str(), err))
                            }
                            WaosEvent::PairingCode { account, .. } => {
                                Some((account.as_str(), "pairing", ""))
                            }
                            WaosEvent::AccountTempBan { account, reason, .. } => {
                                Some((account.as_str(), "banned", reason.as_str()))
                            }
                            _ => None,
                        } {
                            let now = chrono::Utc::now().timestamp();
                            let alias_str = alias.to_string();
                            let status_str = status.to_string();
                            let error_str = error.to_string();
                            if let Err(e) = db.lock().await.main_conn.execute(
                                "UPDATE accounts SET status = ?1, error = ?2, updated_at = ?3 WHERE alias = ?4",
                                turso::params::Params::Positional(vec![
                                    turso::Value::from(status_str),
                                    turso::Value::from(error_str),
                                    turso::Value::from(now),
                                    turso::Value::from(alias_str),
                                ]),
                            ).await {
                                error!("Failed to update account status in DB: {e}");
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        error!("Event bus lagged by {n} messages");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        })
    };

    let dashboard_ticker = {
        let d = dashboard.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(5));
            loop {
                interval.tick().await;
                if let Err(e) = d.write().await {
                    error!("Dashboard write failed: {e}");
                }
            }
        })
    };

    if let Some(ref agent) = config.agent {
        if let Some(ref webhook_url) = agent.webhook_url {
            let pusher =
                WebhookPush::new(webhook_url.clone(), agent.webhook_token.clone());
            tokio::spawn(pusher.run(event_bus.subscribe()));
            info!("Webhook push enabled: {webhook_url}");
        }
    }

    let queue_handle = {
        let q = outbound_queue.clone();
        tokio::spawn(async move {
            q.run().await;
        })
    };

    let orchestrator_handle = {
        let o = orchestrator.clone();
        tokio::spawn(async move {
            o.run().await;
        })
    };

    // Resource monitoring ticker
    let resource_ticker = {
        let rm = resource_monitor.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300));
            loop {
                interval.tick().await;
                let warnings = rm.check().await;
                for w in &warnings {
                    warn!("{w}");
                }
                if rm.clean_media_cache().await.unwrap_or(0) > 0 {
                    info!("Media cache cleaned");
                }
            }
        })
    };

    // Auto-populate frontend dir from web/dist if it doesn't exist
    ensure_frontend_symlink(&config).await;

    let daemon_state = Arc::new(DaemonState {
        db_mgr: db_mgr.clone(),
        account_manager: account_manager.clone(),
        event_bus: event_bus.clone(),
        dashboard: dashboard.clone(),
        attention: attention.clone(),
        scoring: scoring.clone(),
        graph: graph.clone(),
        memory_store: memory_store.clone(),
        vector_memory: vector_memory.clone(),
        orchestrator: orchestrator.clone(),
        health_collector: health_collector.clone(),
        resource_monitor: resource_monitor.clone(),
        anti_ban: anti_ban.clone(),
        db_conns: db_conns.clone(),
        outbound_queue: outbound_queue.clone(),
        sessions_dir: sessions_dir.clone(),
        media_dir: media_dir.clone(),
        sync_engine: sync_engine.clone(),
        dash_path: dash_path.clone(),
        config: config.clone(),
    });

    let server_handle = if config.server.enabled.unwrap_or(false) {
        let port = config.server.port.unwrap_or(8080);
        let bind = config.server.bind.clone().unwrap_or("127.0.0.1".into());
        let frontend_path = if config.server.no_frontend.unwrap_or(false) {
            None
        } else {
            Some(config.frontend_dir())
        };

        let bind_clone = bind.clone();
        let ds = daemon_state.clone();
        let handle = tokio::spawn(async move {
            if let Err(e) = crate::server::run(port, &bind_clone, frontend_path, ds).await {
                error!("Management server failed: {e}");
            }
        });
        info!("Management API server started on {bind}:{port}");
        Some(handle)
    } else {
        None
    };

    // Spawn IPC command server (Unix socket — no DB lock contention)
    let ipc_state = daemon_state.clone();
    let _ipc_handle = tokio::spawn(async move {
        crate::ipc::run_server(ipc_state).await;
    });

    info!("WAOS daemon running. Press Ctrl+C to stop.");

    signal::ctrl_c().await?;

    // Clean up IPC socket
    let sock_path = crate::ipc::socket_path(&config.data_dir());
    let _ = tokio::fs::remove_file(&sock_path).await;
    info!("Shutting down...");

    event_processor.abort();
    dashboard_ticker.abort();
    queue_handle.abort();
    orchestrator_handle.abort();
    resource_ticker.abort();
    if let Some(h) = server_handle {
        h.abort();
    }
    account_manager.lock().await.shutdown_all().await;

    info!("WAOS daemon stopped.");
    Ok(())
}

// ---------------------------------------------------------------------------
// DaemonState — shared state for runtime account management via REST API
// ---------------------------------------------------------------------------

pub struct DaemonState {
    pub db_mgr: Arc<Mutex<DbManager>>,
    pub account_manager: Arc<Mutex<AccountManager>>,
    pub event_bus: EventBus,
    pub dashboard: DashboardWriter,
    pub attention: AttentionEngine,
    pub scoring: ScoringEngine,
    pub graph: RelationshipGraph,
    pub memory_store: MemoryStore,
    pub vector_memory: Option<VectorMemory>,
    pub orchestrator: Orchestrator,
    pub health_collector: HealthCollector,
    pub resource_monitor: ResourceMonitor,
    pub anti_ban: Arc<AntiBan>,
    pub db_conns: Arc<Mutex<HashMap<String, Connection>>>,
    pub outbound_queue: Arc<OutboundQueue>,
    pub sessions_dir: PathBuf,
    pub media_dir: PathBuf,
    pub sync_engine: SyncEngine,
    pub dash_path: PathBuf,
    pub config: Config,
}

impl DaemonState {
    /// Initialize a new account: register in all subsystems, create per-account DB, start Bot.
    pub async fn init_account(&self, alias: &str, phone: Option<&str>) -> anyhow::Result<()> {
        info!("Initializing account '{}'", alias);

        // 1. Register in main DB
        let now = chrono::Utc::now().timestamp();
        let phone_str = phone.unwrap_or("");
        self.db_mgr.lock().await.main_conn.execute(
            "INSERT INTO accounts (alias, phone, status, created_at, updated_at)
             VALUES (?1, ?2, 'disconnected', ?3, ?3)
             ON CONFLICT(alias) DO UPDATE SET phone = ?2, updated_at = ?3",
            turso::params::Params::Positional(vec![
                turso::Value::from(alias),
                turso::Value::from(phone_str),
                turso::Value::from(now),
            ]),
        ).await?;

        // 2. Create per-account DB
        let conn = self.db_mgr.lock().await.open_account_db(alias).await?;

        // 3. Register in all subsystems
        self.sync_engine.register_account(alias, conn.clone()).await;
        self.dashboard.register_account(alias, conn.clone()).await;
        self.attention.register_account(alias, conn.clone()).await;
        self.scoring.register_account(alias, conn.clone()).await;
        self.graph.register_account(alias, conn.clone()).await;
        self.memory_store.register_account(alias, conn.clone()).await;
        self.orchestrator.register_account(alias, conn.clone()).await;
        self.health_collector.register_account(alias, conn.clone()).await;
        if let Some(ref vm) = self.vector_memory {
            vm.register_account(alias, conn.clone()).await;
        }
        self.db_conns.lock().await.insert(alias.to_string(), conn);

        // 4. Dedicated HistorySync connection (BEFORE starting bot)
        let history_conn = self.db_mgr.lock().await.open_account_db_fresh(alias).await?;
        self.account_manager.lock().await.history_conns.lock().await.insert(alias.to_string(), history_conn);

        // 5. Start Bot
        let phone_owned = phone.map(String::from);
        self.account_manager.lock().await
            .add_account_with_phone(alias, &self.sessions_dir, &self.event_bus, phone_owned)
            .await?;

        info!("Account '{}' initialized", alias);
        Ok(())
    }

    /// Remove an account: stop Bot, delete per-account DB, wipe session credentials,
    /// unregister from all subsystems, remove from main DB.
    pub async fn destroy_account(&self, alias: &str) -> anyhow::Result<()> {
        info!("Destroying account '{}'", alias);

        // 1. Stop Bot
        self.account_manager.lock().await.remove_account(alias).await;

        // 2. Remove client reference
        self.db_conns.lock().await.remove(alias);

        // 3. Unregister from all subsystems
        self.account_manager.lock().await.history_conns.lock().await.remove(alias);
        self.sync_engine.remove_account(alias).await;
        self.dashboard.remove_account(alias).await;
        self.attention.remove_account(alias).await;
        self.scoring.remove_account(alias).await;
        self.graph.remove_account(alias).await;
        self.memory_store.remove_account(alias).await;
        self.orchestrator.remove_account(alias).await;
        self.health_collector.remove_account(alias).await;
        if let Some(ref vm) = self.vector_memory {
            vm.remove_account(alias).await;
        }

        // 4. Delete per-account DB file
        self.db_mgr.lock().await.delete_account_db(alias).await?;

        // 5. Wipe session credentials (WhatsApp keys, prekeys, etc.)
        let session_dir = self.sessions_dir.join(alias);
        if session_dir.exists() {
            let mut entries = tokio::fs::read_dir(&session_dir).await?;
            while let Some(entry) = entries.next_entry().await? {
                let _ = tokio::fs::remove_file(entry.path()).await;
            }
            let _ = tokio::fs::remove_dir(&session_dir).await;
            info!("Session directory for '{}' removed", alias);
        }

        // 6. Remove from main DB accounts table
        self.db_mgr.lock().await.main_conn.execute(
            "DELETE FROM accounts WHERE alias = ?1",
            turso::params::Params::Positional(vec![turso::Value::from(alias)]),
        ).await?;

        // 7. Clean up rate counter entries
        self.db_mgr.lock().await.main_conn.execute(
            "DELETE FROM rate_counters WHERE account = ?1",
            turso::params::Params::Positional(vec![turso::Value::from(alias)]),
        ).await?;

        info!("Account '{}' destroyed", alias);
        Ok(())
    }

    /// Check if an account is in logged_out state by querying the main DB.
    /// Returns true if the account status is 'logged_out'.
    pub async fn is_account_logged_out(&self, alias: &str) -> bool {
        let lock = self.db_mgr.lock().await;
        let mut rows = match lock.main_conn.query(
            "SELECT status FROM accounts WHERE alias = ?1",
            turso::params::Params::Positional(vec![turso::Value::from(alias)]),
        ).await {
            Ok(r) => r,
            Err(_) => return false,
        };
        match rows.next().await {
            Ok(Some(row)) => matches!(row.get_value(0), Ok(turso::Value::Text(s)) if s == "logged_out"),
            _ => false,
        }
    }

    /// Reject if account is logged_out. Returns an error message if blocked.
    pub async fn reject_if_logged_out(&self, alias: &str) -> Result<(), String> {
        if self.is_account_logged_out(alias).await {
            Err(format!("Account '{alias}' is logged out. Re-pair before sending messages."))
        } else {
            Ok(())
        }
    }
}

/// Create a symlink at the configured frontend path pointing to `web/dist/`
/// so the server can serve the built frontend assets.
async fn ensure_frontend_symlink(config: &Config) {
    let frontend_target = config.frontend_dir();
    if frontend_target.exists() {
        return;
    }

    // Prefer dist/ next to the binary's Cargo.toml (development), then next to the binary itself
    let candidates = [
        PathBuf::from("./web/dist"),
        PathBuf::from("../web/dist"),
    ];

    let dist_source = candidates.iter().find(|p| p.exists());
    let source = match dist_source {
        Some(s) => s.canonicalize().unwrap_or_else(|_| s.clone()),
        None => {
            info!("No built frontend found at ./web/dist, skipping symlink");
            return;
        }
    };

    // Create parent dir if needed
    if let Some(parent) = frontend_target.parent() {
        if !parent.exists() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                warn!("Failed to create frontend dir parent {parent:?}: {e}");
                return;
            }
        }
    }

    match std::os::unix::fs::symlink(&source, &frontend_target) {
        Ok(()) => info!("Frontend symlink created: {} → {}", frontend_target.display(), source.display()),
        Err(e) => warn!("Failed to create frontend symlink: {e}"),
    }
}
