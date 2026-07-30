use std::sync::Arc;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use topcoat::router::content::websocket::{Message, WebSocket};
use zw_core::{ChannelMessage, ChannelRegistryHandle, ZeroClawHandle, StreamEvent};

pub async fn handle_ws_connection(
    socket: WebSocket,
    registry: ChannelRegistryHandle,
    zeroclaw: Arc<ZeroClawHandle>,
    agent: Arc<String>,
) {
    let (ws_sender, mut ws_receiver) = socket.split();
    let ws_sender = Arc::new(tokio::sync::Mutex::new(ws_sender));

    let mut current_session_id = String::new();

    while let Some(msg) = ws_receiver.next().await {
        let text = match msg {
            Ok(Message::Text(t)) => t,
            Ok(Message::Close(_)) => break,
            Ok(Message::Ping(_)) => {
                let mut sender = ws_sender.lock().await;
                let _ = sender.send(Message::Pong(vec![].into())).await;
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
                let mut sender = ws_sender.lock().await;
                let err = serde_json::json!({"type": "error", "error": format!("Invalid JSON: {}", e)});
                let _ = sender.send(Message::Text(err.to_string().into())).await;
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
                let _ = zeroclaw.cancel_session(sid.to_string()).await;
            }
            continue;
        }

        let session_id = parsed.get("session_id")
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

        let msg = ChannelMessage {
            channel: "web".to_string(),
            session_id: session_id.clone(),
            user_id: user_id.to_string(),
            content: content.to_string(),
            agent_alias: agent.to_string(),
            role: role.to_string(),
            attachments: Vec::new(),
        };

        current_session_id = session_id;
        let (event_tx, mut event_rx) = mpsc::channel::<StreamEvent>(256);
        let ws = ws_sender.clone();
        let registry = registry.clone();

        tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                if let Ok(json) = serde_json::to_string(&event) {
                    let mut sender = ws.lock().await;
                    if sender.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
            }
        });

        tokio::spawn(async move {
            let _ = registry.route_incoming_stream(msg, event_tx).await;
        });
    }

    let mut sender = ws_sender.lock().await;
    let _ = sender.close().await;
}
