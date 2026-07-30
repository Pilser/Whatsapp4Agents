use topcoat::Result;
use topcoat::router::{Body, Response, StatusCode, route, path_param};
use topcoat::context::{Cx, app_context};

use crate::context::AppContext;

#[path_param]
struct FileHash(str);

#[route(GET "/files/{file_hash}")]
pub async fn files(cx: &Cx) -> Result<Response> {
    let ctx: &AppContext = app_context(cx);
    let hash = path_param::<FileHash>(cx);

    let meta = match ctx.cache.get_meta(hash) {
        Ok(Some(m)) => m,
        Ok(None) => {
            return Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("File not found"))
                .unwrap());
        }
        Err(e) => {
            return Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from(format!("Cache error: {}", e)))
                .unwrap());
        }
    };

    let data = match tokio::fs::read(&meta.original_path).await {
        Ok(d) => d,
        Err(e) => {
            return Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from(format!("Error reading file: {}", e)))
                .unwrap());
        }
    };

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("content-type", &meta.mime_type)
        .header("cache-control", "public, max-age=3600")
        .body(Body::from(data))
        .unwrap())
}