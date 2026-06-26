use std::sync::Arc;

use futures_util::SinkExt;
use hyper::{Body, Request, Response, StatusCode};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;

use crate::server::ApiState;

pub async fn handle(
    req: Request<Body>,
    state: Arc<ApiState>,
) -> Result<Response<Body>, std::convert::Infallible> {
    let mut rx = state.daemon.event_bus.subscribe();

    match tokio_tungstenite::hyper::upgrade(req, None).await {
        Ok((response, websocket)) => {
            tokio::spawn(async move {
                let ws = WebSocketStream::from_raw_socket(websocket, tokio_tungstenite::tungstenite::protocol::Role::Server, None).await;
                let (mut sender, _) = ws.split();
                while let Ok(event) = rx.recv().await {
                    let json = match serde_json::to_string(&event) {
                        Ok(j) => j,
                        Err(_) => continue,
                    };
                    if sender.send(Message::Text(json)).await.is_err() {
                        break;
                    }
                }
            });
            Ok(response)
        }
        Err(e) => {
            let body = format!("WebSocket upgrade failed: {e}");
            let response = Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(body))
                .unwrap();
            Ok(response)
        }
    }
}
