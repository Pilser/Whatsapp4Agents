use std::sync::Arc;
use zw_core::{ChannelRegistryHandle, ZeroClawHandle};
use zw_db::DbPool;
use zw_cache::CacheManager;
use crate::config::{SiteSettings, LandingData};

#[derive(Clone)]
pub struct AppContext {
    pub registry: ChannelRegistryHandle,
    pub zeroclaw: Arc<ZeroClawHandle>,
    pub db: DbPool,
    pub cache: Arc<CacheManager>,
    pub agent: String,
    pub upload_dir: std::path::PathBuf,
    pub config_dir: std::path::PathBuf,
    pub settings: SiteSettings,
    pub landing: LandingData,
}

impl AppContext {
    pub fn new(
        registry: ChannelRegistryHandle,
        zeroclaw: Arc<ZeroClawHandle>,
        db: DbPool,
        cache: Arc<CacheManager>,
        agent: &str,
        upload_dir: std::path::PathBuf,
        config_dir: std::path::PathBuf,
        settings: SiteSettings,
        landing: LandingData,
    ) -> Self {
        Self {
            registry,
            zeroclaw,
            db,
            cache,
            agent: agent.to_string(),
            upload_dir,
            config_dir,
            settings,
            landing,
        }
    }
}
