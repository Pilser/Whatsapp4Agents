use std::str::FromStr;
use tokio::net::TcpListener;
use topcoat::router::{Router, RouterBuilderDiscoverExt};
use topcoat::asset::{AssetBundle, RouterBuilderAssetExt};

use crate::context::AppContext;

pub fn build_router(ctx: AppContext) -> Router {
    Router::builder()
        .app_context(ctx)
        .assets(AssetBundle::load().unwrap())
        .discover()
        .build()
}

pub async fn start(ctx: AppContext) {
    let router = build_router(ctx);
    topcoat::start(router).await.unwrap();
}

pub async fn start_on(bind: impl AsRef<str>, ctx: AppContext) -> std::io::Result<()> {
    let addr = std::net::SocketAddr::from_str(bind.as_ref())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    let listener = TcpListener::bind(addr).await?;
    let router = build_router(ctx);
    topcoat::serve(listener, router).await
}
