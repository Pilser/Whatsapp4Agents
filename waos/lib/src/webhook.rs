use waos_core::events::WaosEvent;

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

    async fn send_notification(&self, event: &WaosEvent) {
        let account = match event {
            WaosEvent::NewMessage { account, .. } => account,
            WaosEvent::MentionReceived { account, .. } => account,
            _ => return,
        };

        let body = serde_json::json!({
            "message": "new_whatsapp_activity",
            "account": account,
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
