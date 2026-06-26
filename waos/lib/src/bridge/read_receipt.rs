use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use whatsapp_rust::client::Client;
use whatsapp_rust::Jid;

pub struct ReadReceiptBatcher {
    client: Arc<Client>,
    interval: Duration,
    pending: Arc<Mutex<HashMap<String, Vec<String>>>>,
}

impl ReadReceiptBatcher {
    pub fn new(client: Arc<Client>) -> Self {
        Self {
            client,
            interval: Duration::from_secs(5),
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn run(&self) {
        loop {
            tokio::time::sleep(self.interval).await;
            self.flush().await;
        }
    }

    pub async fn mark_read(&self, chat_id: String, message_id: String) {
        self.pending
            .lock()
            .await
            .entry(chat_id)
            .or_default()
            .push(message_id);
    }

    async fn flush(&self) {
        let mut pending = self.pending.lock().await;
        if pending.is_empty() {
            return;
        }
        let batch = std::mem::take(&mut *pending);
        for (chat_id, msg_ids) in batch {
            if let Some(_last_id) = msg_ids.last() {
                if let Ok(jid) = Jid::from_str(&chat_id) {
                    self.client.chatstate().send_paused(&jid).await.ok();
                }
            }
        }
    }
}
