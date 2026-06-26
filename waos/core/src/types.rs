use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AttentionItem {
    pub rank: usize,
    pub account: String,
    pub chat_id: String,
    pub chat_name: String,
    pub chat_type: String,
    pub reason: String,
    pub score: f64,
    pub unread_count: u32,
    pub has_mention: bool,
    pub last_message_snippet: String,
    pub last_message_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DashboardData {
    pub accounts: serde_json::Value,
    pub summary: serde_json::Value,
    pub attention_queue: Vec<AttentionItem>,
    pub unread_chats: Vec<serde_json::Value>,
    pub health: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WaosConfig {
    pub data_dir: Option<String>,
    pub accounts: Vec<AccountConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccountConfig {
    pub alias: String,
    pub phone: Option<String>,
}
