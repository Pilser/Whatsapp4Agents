use waos_core::events::WaosEvent;

fn sanitize_key(raw: &str) -> String {
    raw.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' || c == '.' { c } else { '_' })
        .collect()
}

#[derive(Clone)]
pub struct WebhookPush {
    agent_url: String,
    auth_token: Option<String>,
}

impl WebhookPush {
    pub fn new(agent_url: String, auth_token: Option<String>) -> Self {
        Self {
            agent_url,
            auth_token,
        }
    }

    pub async fn run(self, mut rx: tokio::sync::broadcast::Receiver<WaosEvent>) {
        while let Ok(event) = rx.recv().await {
            if self.should_notify(&event) {
                self.send_notification(&event).await;
            }
        }
    }

    fn should_notify(&self, event: &WaosEvent) -> bool {
        matches!(
            event,
            WaosEvent::NewMessage { .. } | WaosEvent::MentionReceived { .. }
        )
    }

    fn build_session_key(account: &str, chat_id: &str, sender: &str) -> String {
        sanitize_key(&format!("whatsapp.{account}_{chat_id}_{sender}"))
    }

    async fn send_notification(&self, event: &WaosEvent) {
        let (account, chat_id, sender_jid, sender_name, content, is_group, message_type, has_media) = match event {
            WaosEvent::NewMessage {
                account, chat_id, sender_jid, sender_name,
                content, is_group, message_type, has_media, ..
            } => (
                account.as_str(), chat_id.as_str(), sender_jid.as_str(),
                sender_name.as_str(), content.as_str(), *is_group,
                message_type.as_str(), *has_media,
            ),
            WaosEvent::MentionReceived { account, chat_id, sender_name, message_snippet, .. } => {
                (account.as_str(), chat_id.as_str(), "",
                 sender_name.as_str(), message_snippet.as_str(),
                 false, "mention", false)
            }
            _ => return,
        };

        let session_key = Self::build_session_key(account, chat_id, sender_jid);
        let body = serde_json::json!({
            "event": "new_whatsapp_activity",
            "account": account,
            "channel": "whatsapp",
            "channel_alias": account,
            "session_key": session_key,
            "chat_id": chat_id,
            "sender": sender_jid,
            "sender_name": sender_name,
            "content": content,
            "is_group": is_group,
            "message_type": message_type,
            "has_media": has_media,
        });

        let body_str = serde_json::to_string(&body).unwrap_or_default();
        let client = ureq::Agent::new();
        let mut req = client.post(&self.agent_url).set("Content-Type", "application/json");
        if let Some(token) = &self.auth_token {
            req = req.set("Authorization", &format!("Bearer {token}"));
        }
        match req.send_string(&body_str) {
            Ok(_) => tracing::debug!("Webhook sent successfully"),
            Err(e) => tracing::warn!("Webhook failed: {e}"),
        }
    }
}
