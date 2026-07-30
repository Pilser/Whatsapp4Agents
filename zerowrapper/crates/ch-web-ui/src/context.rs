use std::sync::Arc;
use zw_core::{ChannelRegistryHandle, ZeroClawHandle};
use zw_db::DbPool;
use zw_cache::CacheManager;

#[derive(Clone)]
pub struct AppContext {
    pub registry: ChannelRegistryHandle,
    pub zeroclaw: Arc<ZeroClawHandle>,
    pub db: DbPool,
    pub cache: Arc<CacheManager>,
    pub agent: String,
    pub upload_dir: std::path::PathBuf,
}

impl AppContext {
    pub fn new(
        registry: ChannelRegistryHandle,
        zeroclaw: Arc<ZeroClawHandle>,
        db: DbPool,
        cache: Arc<CacheManager>,
        agent: &str,
        upload_dir: std::path::PathBuf,
    ) -> Self {
        Self {
            registry,
            zeroclaw,
            db,
            cache,
            agent: agent.to_string(),
            upload_dir,
        }
    }
}
