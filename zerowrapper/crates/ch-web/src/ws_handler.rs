use std::sync::Arc;
use futures_util::{pin_mut, SinkExt, StreamExt};
use hyper::upgrade::Upgraded;
use hyper::{Body, Request, Response, StatusCode};
use tokio::sync::mpsc;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;
use tokio_tungstenite::tungstenite::Message;
use zw_core::{ChannelContext, ChannelMessage, StreamEvent};

pub async fn handle_ws_upgrade(
    req: Request<Body>,
    ctx: Arc<ChannelContext>,
    agent_alias: &str,
) -> anyhow::Result<Response<Body>> {
    if !is_ws_upgrade(&req) {
        return Ok(Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from("Expected WebSocket upgrade"))
            .unwrap());
    }

    let key = req.headers()
        .get("sec-websocket-key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| anyhow::anyhow!("Missing Sec-WebSocket-Key header"))?
        .to_string();

    let accept_key = derive_accept_key(key.as_bytes());

    let upgraded = hyper::upgrade::on(req);
    let ctx = ctx.clone();
    let agent_alias = agent_alias.to_string();

    tokio::spawn(async move {
        match upgraded.await {
            Ok(upgraded) => {
                let ws = WebSocketStream::from_raw_socket(
                    upgraded,
                    tokio_tungstenite::tungstenite::protocol::Role::Server,
                    None,
                ).await;
                if let Err(e) = handle_ws_connection(ws, ctx, &agent_alias).await {
                    tracing::warn!("WebSocket connection error: {}", e);
                }
            }
            Err(e) => {
                tracing::warn!("Hyper upgrade error: {}", e);
            }
        }
    });

    Ok(Response::builder()
        .status(StatusCode::SWITCHING_PROTOCOLS)
        .header("Upgrade", "websocket")
        .header("Connection", "Upgrade")
        .header("Sec-WebSocket-Accept", &accept_key)
        .body(Body::empty())
        .unwrap())
}

fn is_ws_upgrade(req: &Request<Body>) -> bool {
    req.headers()
        .get("upgrade")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false)
}

async fn handle_ws_connection(
    ws: WebSocketStream<Upgraded>,
    ctx: Arc<ChannelContext>,
    agent_alias: &str,
) -> anyhow::Result<()> {
    let (mut ws_sender, mut ws_receiver) = ws.split();
    let agent_alias = agent_alias.to_string();

    while let Some(msg) = ws_receiver.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                let parsed: serde_json::Value = match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(e) => {
                        let err = serde_json::json!({"type": "error", "error": format!("Invalid JSON: {}", e)});
                        let _ = ws_sender.send(Message::Text(err.to_string())).await;
                        continue;
                    }
                };

                let session_id = parsed.get("session_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
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
                    session_id: session_id.to_string(),
                    user_id: user_id.to_string(),
                    content: content.to_string(),
                    agent_alias: agent_alias.clone(),
                    role: role.to_string(),
                    attachments,
                };

                // Create event channel for streaming
                let (event_tx, mut event_rx) = mpsc::channel::<StreamEvent>(256);

                // Use select! to interleave event forwarding with route completion
                let route_fut = ctx.registry.route_incoming_stream(msg, event_tx);
                pin_mut!(route_fut);

                let route_result = loop {
                    tokio::select! {
                        result = &mut route_fut => break result,
                        Some(event) = event_rx.recv() => {
                            if let Ok(json) = serde_json::to_string(&event) {
                                let _ = ws_sender.send(Message::Text(json.into())).await;
                            }
                        }
                        else => break Err(anyhow::anyhow!("event channel closed")),
                    }
                };

                match route_result {
                    Ok(_) => {}
                    Err(e) => {
                        let err = serde_json::json!({
                            "type": "error",
                            "error": format!("Route error: {}", e)
                        });
                        let _ = ws_sender.send(Message::Text(err.to_string())).await;
                    }
                }
            }
            Ok(Message::Close(_)) => break,
            Ok(Message::Ping(data)) => {
                let _ = ws_sender.send(Message::Pong(data)).await;
            }
            Err(e) => {
                tracing::warn!("WebSocket error: {}", e);
                break;
            }
            _ => {}
        }
    }

    Ok(())
}
