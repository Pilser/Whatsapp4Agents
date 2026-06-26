use std::sync::Arc;

use hyper::{Body, Response};

use crate::server::ApiState;

const NOT_CONFIGURED: &str = "<!DOCTYPE html><html><body><h1>WAOS</h1><p>Frontend not configured</p></body></html>";

pub async fn spa(state: Arc<ApiState>) -> Response<Body> {
    match &state.frontend_path {
        Some(base) => {
            let res = file_from_disk(&base.join("index.html")).await;
            if res.status() == 404 {
                not_configured()
            } else {
                res
            }
        }
        None => not_configured(),
    }
}

pub async fn file(state: Arc<ApiState>, path: &str) -> Response<Body> {
    let base = match &state.frontend_path {
        Some(b) => b.clone(),
        None => return not_found(),
    };

    let stripped = path.trim_start_matches('/');
    let file_path = base.join(stripped);

    // Try exact path (e.g. /style.css)
    let res = file_from_disk(&file_path).await;
    if res.status() != 404 {
        return res;
    }

    // Try path/index.html (e.g. /accounts → accounts/index.html)
    let index_path = file_path.join("index.html");
    let res = file_from_disk(&index_path).await;
    if res.status() != 404 {
        return res;
    }

    // Try parent directory index.html for client-side routes (e.g. /accounts/my_bot → accounts/index.html)
    if let Some(parent) = file_path.parent() {
        let parent_index = parent.join("index.html");
        let res = file_from_disk(&parent_index).await;
        if res.status() != 404 {
            return res;
        }
    }

    // SPA fallback: serve root index.html
    file_from_disk(&base.join("index.html")).await
}

async fn file_from_disk(path: &std::path::Path) -> Response<Body> {
    match tokio::fs::read(path).await {
        Ok(content) => {
            let mime = mime_guess::from_path(path)
                .first_or_octet_stream()
                .to_string();
            Response::builder()
                .status(200)
                .header("content-type", mime)
                .body(Body::from(content))
                .unwrap()
        }
        Err(_) => not_found(),
    }
}

fn not_configured() -> Response<Body> {
    Response::builder()
        .status(200)
        .header("content-type", "text/html")
        .body(Body::from(NOT_CONFIGURED))
        .unwrap()
}

fn not_found() -> Response<Body> {
    Response::builder()
        .status(404)
        .body(Body::from("Not found"))
        .unwrap()
}
