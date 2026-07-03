pub mod file_routes;
pub mod ws_handler;
pub mod health;
pub mod static_files;

use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use async_trait::async_trait;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server, StatusCode};
use serde_json::json;
use zw_cache::CacheManager;
use zw_core::{Channel, ChannelContext};
use zw_db::DbPool;
use crate::file_routes::handle_file_request;
use crate::ws_handler::handle_ws_upgrade;
use crate::static_files::handle_static_request;

pub struct WebChannel {
    bind: String,
    agent: String,
    cache: Arc<CacheManager>,
    dist_path: Option<PathBuf>,
    db: DbPool,
}

impl WebChannel {
    pub fn new(bind: &str, agent: &str, cache: Arc<CacheManager>, dist_path: Option<PathBuf>, db: DbPool) -> Self {
        Self {
            bind: bind.to_string(),
            agent: agent.to_string(),
            cache,
            dist_path,
            db,
        }
    }
}

#[async_trait]
impl Channel for WebChannel {
    fn name(&self) -> &str {
        "web"
    }

    async fn start(self: Box<Self>, ctx: ChannelContext) -> anyhow::Result<()> {
        let addr: SocketAddr = self.bind.parse()?;
        let cache = self.cache.clone();
        let ctx = Arc::new(ctx);
        let dist_path = self.dist_path.clone();
        let agent = self.agent.clone();
        let db = self.db.clone();

        tracing::info!("Web channel starting on {}, agent={}", self.bind, agent);

        let make_svc = make_service_fn(move |_| {
            let ctx = ctx.clone();
            let cache = cache.clone();
            let dist_path = dist_path.clone();
            let agent = agent.clone();
            let db = db.clone();
            async move {
                Ok::<_, Infallible>(service_fn(move |req| {
                    handle_request(req, ctx.clone(), cache.clone(), dist_path.clone(), agent.clone(), db.clone())
                }))
            }
        });

        let server = Server::bind(&addr).serve(make_svc);
        server.await?;
        Ok(())
    }
}

async fn handle_request(
    req: Request<Body>,
    ctx: Arc<ChannelContext>,
    cache: Arc<CacheManager>,
    dist_path: Option<PathBuf>,
    agent: String,
    db: DbPool,
) -> Result<Response<Body>, Infallible> {
    let path = req.uri().path().to_string();

    if path == "/health" {
        return Ok(health::health_response());
    }

    if let Some(hash) = path.strip_prefix("/files/") {
        return Ok(handle_file_request(hash, cache.clone()).await);
    }

    if path == "/ws" {
        match handle_ws_upgrade(req, ctx.clone(), &agent).await {
            Ok(resp) => return Ok(resp),
            Err(e) => {
                tracing::warn!("WebSocket upgrade failed: {}", e);
                return Ok(Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(Body::from(format!("WebSocket upgrade failed: {}", e)))
                    .unwrap());
            }
        }
    }

    if path == "/api/register" && req.method() == hyper::Method::POST {
        return Ok(handle_register(req, db.clone()).await);
    }

    if path == "/api/login" && req.method() == hyper::Method::GET {
        return Ok(handle_login(req, db.clone()).await);
    }

    if path == "/api/history" && req.method() == hyper::Method::GET {
        return Ok(handle_history(req, ctx.clone(), &agent).await);
    }

    if let Some(ref dist) = dist_path {
        if let Ok(resp) = handle_static_request(dist, &path) {
            return Ok(resp);
        }
    }

    Ok(Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::from("Not found"))
        .unwrap())
}

async fn handle_history(req: Request<Body>, ctx: Arc<ChannelContext>, agent: &str) -> Response<Body> {
    let user_id = req.uri().query()
        .and_then(|q| {
            for pair in q.split('&') {
                let mut parts = pair.splitn(2, '=');
                if parts.next() == Some("user_id") {
                    return parts.next().map(|v| url_decode(v));
                }
            }
            None
        })
        .unwrap_or_default();

    // Build the same session ID format used during prompt: web.agent.user_id
    let session_id = format!("web.{}.{}", agent, sanitize_key(&user_id));

    tracing::info!("handle_history: user_id='{}' session_id='{}'", user_id, session_id);

    if user_id.is_empty() {
        tracing::warn!("handle_history: missing user_id");
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header("Content-Type", "application/json")
            .body(Body::from(json!({"error": "Missing user_id"}).to_string()))
            .unwrap();
    }

    match ctx.zeroclaw.get_session_messages(session_id.clone(), Some(100)).await {
        Ok(result) => {
            tracing::info!("handle_history: got {} messages", result.messages.len());
            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&result).unwrap_or_default()))
                .unwrap()
        }
        Err(e) => {
            tracing::warn!("handle_history: zeroclaw error: {}", e);
            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .body(Body::from(json!({
                    "session_id": session_id,
                    "messages": [],
                    "total": 0,
                    "start": 0
                }).to_string()))
                .unwrap()
        }
    }
}

async fn handle_register(req: Request<Body>, db: DbPool) -> Response<Body> {
    let body_bytes = match hyper::body::to_bytes(req.into_body()).await {
        Ok(b) => b,
        Err(e) => {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("Content-Type", "application/json")
                .body(Body::from(json!({"error": format!("Failed to read body: {}", e)}).to_string()))
                .unwrap();
        }
    };

    let parsed: serde_json::Value = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(e) => {
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .header("Content-Type", "application/json")
                .body(Body::from(json!({"error": format!("Invalid JSON: {}", e)}).to_string()))
                .unwrap();
        }
    };

    let phone = parsed.get("phone").and_then(|v| v.as_str()).unwrap_or("");
    if phone.is_empty() {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header("Content-Type", "application/json")
            .body(Body::from(json!({"error": "Missing phone"}).to_string()))
            .unwrap();
    }

    let name = parsed.get("name").and_then(|v| v.as_str()).filter(|s| !s.is_empty());

    match db.upsert_user(phone, name, None).await {
        Ok(_) => {
            let display_name = name.unwrap_or(phone);
            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .body(Body::from(json!({"ok": true, "phone": phone, "name": display_name}).to_string()))
                .unwrap()
        }
        Err(e) => {
            tracing::warn!("handle_register: db error: {}", e);
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header("Content-Type", "application/json")
                .body(Body::from(json!({"error": format!("Database error: {}", e)}).to_string()))
                .unwrap()
        }
    }
}

async fn handle_login(req: Request<Body>, db: DbPool) -> Response<Body> {
    let phone = req.uri().query()
        .and_then(|q| {
            for pair in q.split('&') {
                let mut parts = pair.splitn(2, '=');
                if parts.next() == Some("phone") {
                    return parts.next().map(|v| url_decode(v));
                }
            }
            None
        })
        .unwrap_or_default();

    if phone.is_empty() {
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .header("Content-Type", "application/json")
            .body(Body::from(json!({"error": "Missing phone"}).to_string()))
            .unwrap();
    }

    match db.get_user(&phone).await {
        Ok(Some(user)) => {
            let display_name = if user.name.is_empty() { phone.clone() } else { user.name };
            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .body(Body::from(json!({
                    "exists": true,
                    "phone": user.id,
                    "name": display_name
                }).to_string()))
                .unwrap()
        }
        Ok(None) => {
            Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .body(Body::from(json!({"exists": false, "phone": phone}).to_string()))
                .unwrap()
        }
        Err(e) => {
            tracing::warn!("handle_login: db error: {}", e);
            Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header("Content-Type", "application/json")
                .body(Body::from(json!({"error": format!("Database error: {}", e)}).to_string()))
                .unwrap()
        }
    }
}

fn sanitize_key(raw: &str) -> String {
    raw.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' || c == '.' { c } else { '_' })
        .collect()
}

fn url_decode(s: &str) -> String {
    s.replace("%20", " ").replace("%3A", ":").replace("%2F", "/")
        .replace("%3F", "?").replace("%3D", "=").replace("%26", "&")
}
