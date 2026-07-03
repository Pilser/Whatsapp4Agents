use std::sync::Arc;

use hyper::{Body, Request, Response, Method};

use crate::server::ApiState;

pub mod accounts;
pub mod agents;
pub mod auth;
pub mod dashboard;
pub mod events;
pub mod health;
pub mod static_;

pub async fn handle_request(
    req: Request<Body>,
    state: Arc<ApiState>,
) -> Result<Response<Body>, std::convert::Infallible> {
    let origin = req
        .headers()
        .get("origin")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("*")
        .to_string();

    if req.method() == Method::OPTIONS {
        return Ok(cors_response(origin));
    }

    let method = req.method().clone();
    let path = req.uri().path().to_string();

    if method == Method::POST || method == Method::PUT || method == Method::DELETE {
        if let Some(resp) = auth::require_auth(&req, &state) {
            return Ok(resp);
        }
    }

    let response = match (&method, path.as_str()) {
        (&Method::GET, "/api/health") => health::detailed(&state).await,
        (&Method::GET, "/api/accounts") => accounts::list(&state).await,
        (&Method::POST, "/api/accounts") => accounts::create(&state, req).await,
        (&Method::POST, p) if p.starts_with("/api/accounts/") && p.ends_with("/regenerate-qr") => {
            accounts::regenerate_qr(&state, &extract_alias(&path)).await
        }
        (&Method::POST, p) if p.starts_with("/api/accounts/") && p.ends_with("/pair") => {
            accounts::pair(&state, &extract_alias(&path), req).await
        }
        (&Method::PUT, p) if p.starts_with("/api/accounts/") && p.ends_with("/agents") => {
            accounts::update_account_agents(&state, &extract_alias(&path), req).await
        }
        (&Method::GET, "/api/agents") => agents::list(&state).await,
        (&Method::POST, "/api/agents") => agents::create(&state, req).await,
        (&Method::DELETE, p) if p.starts_with("/api/agents/") => {
            agents::delete(&state, &extract_alias(&path)).await
        }
        (&Method::GET, p) if p.starts_with("/api/accounts/") && p.ends_with("/qr") => {
            accounts::qr(&state, &extract_alias(&path)).await
        }
        (&Method::GET, p) if p.starts_with("/api/accounts/") => {
            accounts::get(&state, &extract_alias(&path)).await
        }
        (&Method::DELETE, p) if p.starts_with("/api/accounts/") => {
            accounts::delete(&state, &extract_alias(&path)).await
        }
        (&Method::GET, "/api/dashboard") => dashboard::handle(&state).await,
        (&Method::GET, "/api/auth/token") => auth::token(&state).await,
        (&Method::GET, "/events") => events::stream(state).await,
        (&Method::GET, "/") => static_::spa(state).await,
        (&Method::GET, _) => static_::file(state, &path).await,
        _ => not_found(),
    };

    let mut response = response;
    let headers = response.headers_mut();
    headers.insert(
        "access-control-allow-origin",
        origin.parse().unwrap(),
    );
    headers.insert(
        "access-control-allow-methods",
        "GET, POST, DELETE, OPTIONS".parse().unwrap(),
    );
    headers.insert(
        "access-control-allow-headers",
        "content-type, authorization".parse().unwrap(),
    );

    Ok(response)
}

fn cors_response(origin: String) -> Response<Body> {
    Response::builder()
        .status(204)
        .header("access-control-allow-origin", origin)
        .header("access-control-allow-methods", "GET, POST, DELETE, OPTIONS")
        .header("access-control-allow-headers", "content-type, authorization")
        .body(Body::empty())
        .unwrap()
}

fn extract_alias(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() >= 4 {
        parts[3].to_string()
    } else {
        String::new()
    }
}

fn not_found() -> Response<Body> {
    Response::builder()
        .status(404)
        .body(Body::from("Not found"))
        .unwrap()
}
