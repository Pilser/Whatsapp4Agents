use anyhow::{Context, Result};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tracing::{debug, warn};

use super::protocol::{self, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse, SessionUpdateEvent};

#[derive(Debug, Clone)]
pub enum ZeroClawEvent {
    Thinking,
    TextChunk,
    ToolCall,
    ToolResult,
    Complete,
}

pub struct ZeroClawClient {
    writer: OwnedWriteHalf,
    reader: BufReader<OwnedReadHalf>,
    next_id: u64,
}

impl ZeroClawClient {
    pub async fn connect(socket_path: &str) -> Result<Self> {
        let stream = UnixStream::connect(socket_path)
            .await
            .with_context(|| format!("connect to ZeroClaw socket at {socket_path}"))?;
        let (reader, writer) = stream.into_split();
        Ok(Self {
            writer,
            reader: BufReader::new(reader),
            next_id: 0,
        })
    }

    pub async fn initialize(&mut self) -> Result<protocol::InitializeResult> {
        let params = serde_json::json!({"protocol_version": 1});
        let resp = self.send_request("initialize", params).await?;
        let result: protocol::InitializeResult =
            serde_json::from_value(resp).context("parse initialize result")?;
        debug!("Connected to ZeroClaw v{}", result.server_version);
        Ok(result)
    }

    pub async fn session_new(
        &mut self,
        agent_alias: &str,
        session_id: &str,
    ) -> Result<protocol::SessionNewResult> {
        let params = serde_json::json!({
            "agent_alias": agent_alias,
            "session_id": session_id,
        });
        let resp = self.send_request("session/new", params).await?;
        let result: protocol::SessionNewResult =
            serde_json::from_value(resp).context("parse session/new result")?;
        Ok(result)
    }

    pub async fn session_prompt(
        &mut self,
        session_id: &str,
        prompt: &str,
        event_tx: mpsc::UnboundedSender<ZeroClawEvent>,
    ) -> Result<String> {
        let params = serde_json::json!({
            "session_id": session_id,
            "prompt": prompt,
        });
        let req_id = self.next_id;
        let raw = serde_json::to_string(&JsonRpcRequest::new(req_id, "session/prompt", params))?;
        let mut line = raw;
        line.push('\n');
        self.writer
            .write_all(line.as_bytes())
            .await
            .context("write session/prompt")?;

        let mut full_text = String::new();

        loop {
            let mut buf = String::new();
            let n = self
                .reader
                .read_line(&mut buf)
                .await
                .context("read from ZeroClaw socket")?;
            if n == 0 {
                anyhow::bail!("ZeroClaw connection closed while reading session/prompt");
            }

            match self.parse_frame(&buf) {
                Frame::Response { id, result, .. } => {
                    if id == req_id {
                        // If we got a response before TurnComplete, use its content
                        if let Ok(sr) = serde_json::from_value::<protocol::SessionPromptResult>(result) {
                            if !sr.content.is_empty() {
                                full_text = sr.content;
                            }
                        }
                        return Ok(full_text);
                    }
                }
                Frame::Notification { params, .. } => {
                    if let Ok(event) =
                        serde_json::from_value::<SessionUpdateEvent>(params.clone())
                    {
                        match event {
                            SessionUpdateEvent::AgentMessageChunk { text, .. } => {
                                full_text.push_str(&text);
                                let _ = event_tx.send(ZeroClawEvent::TextChunk);
                            }
                            SessionUpdateEvent::AgentThoughtChunk { .. } => {
                                let _ = event_tx.send(ZeroClawEvent::Thinking);
                            }
                            SessionUpdateEvent::ToolCall { .. } => {
                                let _ = event_tx.send(ZeroClawEvent::ToolCall);
                            }
                            SessionUpdateEvent::ToolResult { .. } => {
                                let _ = event_tx.send(ZeroClawEvent::ToolResult);
                            }
                            SessionUpdateEvent::TurnComplete { outcome, content, .. } => {
                                let _ = event_tx.send(ZeroClawEvent::Complete);
                                if outcome == "completed" {
                                    if !content.is_empty() {
                                        full_text = content;
                                    }
                                    return Ok(full_text);
                                } else {
                                    anyhow::bail!("ZeroClaw turn {outcome}: {content}");
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Frame::ResponseError { id, code, message } => {
                    if id == req_id {
                        anyhow::bail!("ZeroClaw session/prompt error ({}): {message}", code);
                    }
                }
                Frame::Unknown => {}
            }
        }
    }

    async fn send_request(&mut self, method: &str, params: Value) -> Result<Value> {
        self.next_id += 1;
        let req_id = self.next_id;
        let raw = serde_json::to_string(&JsonRpcRequest::new(req_id, method, params))?;
        let mut line = raw;
        line.push('\n');
        self.writer
            .write_all(line.as_bytes())
            .await
            .with_context(|| format!("write {method} request"))?;

        loop {
            let mut buf = String::new();
            let n = self
                .reader
                .read_line(&mut buf)
                .await
                .with_context(|| format!("read {method} response"))?;
            if n == 0 {
                anyhow::bail!("ZeroClaw connection closed while waiting for {method} response");
            }

            match self.parse_frame(&buf) {
                Frame::Response { id, result, .. } => {
                    if id == req_id {
                        return Ok(result);
                    }
                }
                Frame::Notification { .. } => {
                    continue;
                }
                Frame::ResponseError { id, code, message } => {
                    if id == req_id {
                        anyhow::bail!("ZeroClaw {method} error ({}): {message}", code);
                    }
                }
                Frame::Unknown => {
                    continue;
                }
            }
        }
    }

    fn parse_frame(&self, line: &str) -> Frame {
        let trimmed = line.trim();
        match serde_json::from_str::<JsonRpcResponse>(trimmed) {
            Ok(resp) => {
                if let Some(id) = resp.id {
                    if let Some(err) = resp.error {
                        return Frame::ResponseError {
                            id,
                            code: err.code,
                            message: err.message,
                        };
                    }
                    if let Some(result) = resp.result {
                        return Frame::Response { id, result };
                    }
                }
            }
            Err(_) => {}
        }

        if let Ok(notif) = serde_json::from_str::<JsonRpcNotification>(trimmed) {
            return Frame::Notification {
                _method: notif.method,
                params: notif.params,
            };
        }

        warn!("Unrecognized frame from ZeroClaw: {trimmed}");
        Frame::Unknown
    }
}

enum Frame {
    Response { id: u64, result: Value },
    ResponseError { id: u64, code: i64, message: String },
    Notification { _method: String, params: Value },
    Unknown,
}
