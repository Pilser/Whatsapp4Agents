use std::sync::Arc;
use hyper::{Body, Response, StatusCode};
use zw_cache::CacheManager;

pub async fn handle_file_request(hash: &str, cache: Arc<CacheManager>) -> Response<Body> {
    tracing::debug!("handle_file_request: hash={}, cache_dir={:?}", hash, cache.cache_dir());
    let meta_path = cache.cache_dir().join("meta").join(&hash[..2.min(hash.len())]).join(hash);
    tracing::debug!("  meta path: {:?}, exists: {}", meta_path, meta_path.exists());
    let meta = match cache.get_meta(hash) {
        Ok(Some(m)) => m,
        Ok(None) => return error_response("File not found"),
        Err(e) => return error_response(&format!("Cache error: {}", e)),
    };

    let path = std::path::Path::new(&meta.original_path);
    if !path.exists() {
        return error_response("Original file no longer exists");
    }

    let file_size = cache.current_size(&meta);

    match tokio::fs::read(path).await {
        Ok(data) => {
            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", &meta.mime_type)
                .header("content-length", file_size.to_string())
                .header("cache-control", "public, max-age=3600")
                .body(Body::from(data))
                .unwrap()
        }
        Err(e) => error_response(&format!("Error reading file: {}", e)),
    }
}

fn error_response(msg: &str) -> Response<Body> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::from(msg.to_string()))
        .unwrap()
}
