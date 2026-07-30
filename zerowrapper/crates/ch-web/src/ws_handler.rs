use std::sync::Arc;
use futures_util::{SinkExt, StreamExt};
use hyper::body::Incoming;
use hyper::upgrade::Upgraded;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use http_body_util::Full;
use bytes::Bytes;
use tokio::sync::mpsc;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use zw_core::{ChannelContext, ChannelMessage, StreamEvent};

pub async fn handle_ws_upgrade(
    req: Request<Incoming>,
    ctx: Arc<ChannelContext>,
    agent_alias: &str,
) -> anyhow::Result<Response<Full<Bytes>>> {
    if !is_ws_upgrade(&req) {
        return Ok(Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Full::new(Bytes::from("Expected WebSocket upgrade")))
            .unwrap());
    }

    let agent_alias = agent_alias.to_string();
    let ctx = ctx.clone();

    let mut req = req;
    let upgraded = hyper::upgrade::on(&mut req).await?;

    tokio::spawn(async move {
        let ws = match accept_async(TokioIo::new(upgraded)).await {
            Ok(ws) => ws,
            Err(e) => {
                tracing::warn!("WebSocket accept error: {}", e);
                return;
            }
        };
        if let Err(e) = handle_ws_connection(ws, ctx, &agent_alias).await {
            tracing::warn!("WebSocket connection error: {}", e);
        }
    });

    Ok(Response::builder()
        .status(StatusCode::SWITCHING_PROTOCOLS)
        .header("Upgrade", "websocket")
        .header("Connection", "Upgrade")
        .body(Full::new(Bytes::new()))
        .unwrap())
}

fn is_ws_upgrade(req: &Request<Incoming>) -> bool {
    req.headers()
        .get("upgrade")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false)
}

async fn handle_ws_connection(
    ws: WebSocketStream<TokioIo<Upgraded>>,
    ctx: Arc<ChannelContext>,
    agent_alias: &str,
) -> anyhow::Result<()> {
    let (ws_sender, mut ws_receiver) = ws.split();
    let ws_sender = Arc::new(tokio::sync::Mutex::new(ws_sender));
    let agent_alias = agent_alias.to_string();

    let mut current_session_id = String::new();

    while let Some(msg) = ws_receiver.next().await {
        let text = match msg {
            Ok(Message::Text(t)) => t,
            Ok(Message::Close(_)) => break,
            Ok(Message::Ping(data)) => {
                let mut ws = ws_sender.lock().await;
                let _ = ws.send(Message::Pong(data)).await;
                continue;
            }
            Err(e) => {
                tracing::warn!("WebSocket error: {}", e);
                break;
            }
            _ => continue,
        };

        let parsed: serde_json::Value = match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(e) => {
                let mut ws = ws_sender.lock().await;
                let err = serde_json::json!({"type": "error", "error": format!("Invalid JSON: {}", e)});
                let _ = ws.send(Message::Text(err.to_string().into())).await;
                continue;
            }
        };

        if parsed.get("type").and_then(|v| v.as_str()) == Some("cancel") {
            let sid = parsed.get("session_id")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    if current_session_id.is_empty() { None } else { Some(current_session_id.as_str()) }
                });
            if let Some(sid) = sid {
                let _ = ctx.zeroclaw.cancel_session(sid.to_string()).await;
            }
            continue;
        }

        let session_id_val = parsed.get("session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let user_id = parsed.get("user_id")
            .and_then(|v| v.as_str())
            .unwrap_or("anonymous");
        let content = parsed.get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let role = parsed.get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("public");

        let attachments = parsed.get("attachments")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter().filter_map(|a| {
                    let filename = a.get("filename").and_then(|v| v.as_str()).unwrap_or("file");
                    let mime_type = a.get("mime_type").and_then(|v| v.as_str()).unwrap_or("application/octet-stream");
                    let path = a.get("path").and_then(|v| v.as_str()).unwrap_or("");
                    let data = a.get("data").and_then(|v| v.as_str()).unwrap_or("");
                    Some(zw_core::Attachment {
                        filename: filename.to_string(),
                        mime_type: mime_type.to_string(),
                        data: data.as_bytes().to_vec(),
                        path: path.to_string(),
                    })
                }).collect()
            })
            .unwrap_or_default();

        let msg = ChannelMessage {
            channel: "web".to_string(),
            session_id: session_id_val.clone(),
            user_id: user_id.to_string(),
            content: content.to_string(),
            agent_alias: agent_alias.clone(),
            role: role.to_string(),
            attachments,
        };

        current_session_id = session_id_val;
        let (event_tx, mut event_rx) = mpsc::channel::<StreamEvent>(256);
        let ws = ws_sender.clone();
        let registry = ctx.registry.clone();

        tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                if let Ok(json) = serde_json::to_string(&event) {
                    let mut ws = ws.lock().await;
                    if ws.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
            }
        });

        tokio::spawn(async move {
            let _ = registry.route_incoming_stream(msg, event_tx).await;
        });
    }

    Ok(())
}
