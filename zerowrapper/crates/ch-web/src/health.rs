use hyper::{Response, StatusCode};
use http_body_util::Full;
use bytes::Bytes;

pub fn health_response() -> Response<Full<Bytes>> {
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(r#"{"status":"ok"}"#)))
        .unwrap()
}
