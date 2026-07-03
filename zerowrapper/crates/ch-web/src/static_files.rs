use std::path::Path;
use hyper::{Body, Response, StatusCode};

pub fn handle_static_request(dist_path: &Path, request_path: &str) -> Result<Response<Body>, ()> {
    let path = request_path.trim_start_matches('/');
    let file_path = if path.is_empty() || path == "/" {
        dist_path.join("index.html")
    } else {
        // Prevent path traversal
        if path.contains("..") {
            return Err(());
        }
        dist_path.join(path)
    };

    if !file_path.starts_with(dist_path) {
        return Err(());
    }

    match std::fs::read(&file_path) {
        Ok(data) => {
            let mime = mime_guess::from_path(&file_path)
                .first_or_octet_stream()
                .to_string();
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("content-type", mime)
                .body(Body::from(data))
                .unwrap())
        }
        Err(_) => Err(()),
    }
}
