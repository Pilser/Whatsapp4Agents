use hyper::{Body, Response, StatusCode};

pub fn health_response() -> Response<Body> {
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .body(Body::from(r#"{"status":"ok"}"#))
        .unwrap()
}
