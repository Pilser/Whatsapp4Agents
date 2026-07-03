pub mod protocol;
pub mod dispatch;

use std::sync::Arc;
use tokio::sync::RwLock;
use zw_core::McpToolProvider;

pub struct McpRegistry {
    providers: Arc<RwLock<Vec<Box<dyn McpToolProvider>>>>,
}

impl McpRegistry {
    pub fn new() -> Self {
        Self {
            providers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Register a provider. Call from async context with `.await`.
    pub async fn register_provider(&self, provider: Box<dyn McpToolProvider>) {
        let name = provider.name().to_string();
        tracing::info!("Registering MCP tool provider: {}", name);
        self.providers.write().await.push(provider);
    }

    pub async fn run_stdio(self) -> anyhow::Result<()> {
        dispatch::run_stdio_loop(self.providers).await
    }
}

impl Default for McpRegistry {
    fn default() -> Self {
        Self::new()
    }
}
