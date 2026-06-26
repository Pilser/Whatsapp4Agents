use std::sync::Arc;

use hyper::{Body, Response};
use tracing::warn;

use crate::server::ApiState;

pub async fn stream(state: Arc<ApiState>) -> Response<Body> {
    let mut rx = state.daemon.event_bus.subscribe();

    let (mut tx, body) = Body::channel();

    tokio::spawn(async move {
        // Send initial SSE comment to flush HTTP response headers immediately.
        // Without this, hyper may buffer the status 200 + Content-Type header
        // until the first data chunk, causing the browser's EventSource to never
        // fire `onopen` and stay stuck in "connecting..." state.
        if tx.send_data(":\n\n".into()).await.is_err() {
            return;
        }
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let json = match serde_json::to_string(&event) {
                        Ok(j) => j,
                        Err(_) => continue,
                    };
                    let msg = format!("data: {}\n\n", json);
                    if tx.send_data(msg.into()).await.is_err() {
                        warn!("SSE client disconnected");
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    Response::builder()
        .status(200)
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .header("connection", "keep-alive")
        .body(body)
        .unwrap()
}
