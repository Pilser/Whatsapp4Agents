use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn generate_id() -> String {
    Uuid::new_v4().to_string()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum WaosEvent {
    Connected { account: String },
    Disconnected { account: String },
    Reconnecting { account: String },
    QrCodeReady { account: String, code: String },
    PairingCode { account: String, code: String },
    NewMessage {
        account: String,
        chat_id: String,
        message_id: String,
        sender_jid: String,
        sender_name: String,
        content: String,
        mentions: Vec<String>,
        has_media: bool,
        is_group: bool,
        is_from_me: bool,
        message_type: String,
        media_caption: String,
        timestamp: i64,
    },
    MessageEdited {
        account: String,
        chat_id: String,
        message_id: String,
        new_content: String,
    },
    MessageDeleted {
        account: String,
        chat_id: String,
        message_id: String,
    },
    ReactionAdded {
        account: String,
        chat_id: String,
        message_id: String,
        emoji: String,
        sender_jid: String,
    },
    MentionReceived {
        account: String,
        chat_id: String,
        chat_name: String,
        sender_name: String,
        message_snippet: String,
        timestamp: i64,
    },
    ContactPushNameUpdated {
        account: String,
        jid: String,
        push_name: String,
    },
    GroupMetadataChanged {
        account: String,
        group_jid: String,
        name: String,
        description: String,
    },
    GroupParticipantsChanged {
        account: String,
        group_jid: String,
        action: String,
        participant_jids: Vec<String>,
    },
    ChatArchived {
        account: String,
        chat_id: String,
        archived: bool,
    },
    ChatMarkedRead {
        account: String,
        chat_id: String,
    },
    ChatDeleted {
        account: String,
        chat_id: String,
    },
    ChatMuted {
        account: String,
        chat_id: String,
        muted_until: i64,
    },
    ChatPinned {
        account: String,
        chat_id: String,
    },
    PictureChanged {
        account: String,
        jid: String,
        removed: bool,
    },
    MessageReceipt {
        account: String,
        chat_id: String,
        message_ids: Vec<String>,
        receipt_type: String,
        timestamp: i64,
    },
    UndecryptableMessage {
        account: String,
        chat_id: String,
        message_id: String,
        sender_jid: String,
        timestamp: i64,
    },
    AccountTempBan {
        account: String,
        reason: String,
        expire_secs: u64,
    },
    ContactPhoneChanged {
        account: String,
        old_jid: String,
        new_jid: String,
    },
    ChatPresenceUpdate {
        account: String,
        chat_id: String,
        sender_jid: String,
        state: String,
    },
    PresenceUpdate {
        account: String,
        jid: String,
        available: bool,
        last_seen: Option<i64>,
    },
    AccountStatusUpdate {
        account: String,
        status: String,
    },
    HistorySyncProgress {
        account: String,
        sync_type: i32,
        progress: Option<u32>,
        chunk_order: Option<u32>,
    },
    QueueItemStatusChanged {
        account: String,
        chat_id: String,
        queue_id: String,
        status: String,
        message_id: Option<String>,
        error: Option<String>,
    },
    StatusUpdate {
        account: String,
        status_id: String,
        action: String,
        message_id: Option<String>,
        error: Option<String>,
        timestamp: i64,
    },
}
