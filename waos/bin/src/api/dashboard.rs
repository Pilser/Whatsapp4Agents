use std::sync::Arc;

use hyper::{Body, Response};

use crate::server::ApiState;

pub async fn handle(state: &Arc<ApiState>) -> Response<Body> {
    let path = &state.daemon.dash_path;
    match tokio::fs::read_to_string(path).await {
        Ok(content) => Response::builder()
            .status(200)
            .header("content-type", "application/json")
            .body(Body::from(content))
            .unwrap(),
        Err(_) => Response::builder()
            .status(200)
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap(),
    }
}
