use std::sync::Arc;

use hyper::{Body, Response};

use crate::server::ApiState;

pub async fn stream(state: Arc<ApiState>) -> Response<Body> {
    let mut rx = state.daemon.event_bus.subscribe();

    let (mut tx, body) = Body::channel();

    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let json = match serde_json::to_string(&event) {
                        Ok(j) => j,
                        Err(_) => continue,
                    };
                    let msg = format!("data: {}\n\n", json);
                    if tx.send_data(msg.into()).await.is_err() {
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
