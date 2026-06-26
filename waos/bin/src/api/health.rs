use std::sync::Arc;

use hyper::{Body, Response};

use crate::server::ApiState;

pub async fn detailed(state: &Arc<ApiState>) -> Response<Body> {
    let health = state.daemon.health_collector.collect().await;
    let body = serde_json::to_string(&health).unwrap_or_default();
    Response::builder()
        .status(200)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap()
}
