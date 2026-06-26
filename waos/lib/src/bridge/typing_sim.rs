use std::time::Duration;

use rand::Rng;
use whatsapp_rust::client::Client;
use whatsapp_rust::Jid;

pub struct TypingSimulator {
    duration_ms: u64,
}

impl TypingSimulator {
    pub fn new(duration_ms: u64) -> Self {
        Self { duration_ms }
    }

    pub async fn simulate(&self, client: &Client, chat_id: &Jid) {
        let _ = client.chatstate().send_composing(chat_id).await;
        let jitter = rand::thread_rng().gen_range(0..self.duration_ms);
        tokio::time::sleep(Duration::from_millis(self.duration_ms + jitter)).await;
        let _ = client.chatstate().send_paused(chat_id).await;
    }
}
