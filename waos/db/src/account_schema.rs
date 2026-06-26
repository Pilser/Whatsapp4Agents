pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS chats (
    id TEXT PRIMARY KEY,
    name TEXT,
    type TEXT NOT NULL DEFAULT 'individual',
    unread_count INTEGER NOT NULL DEFAULT 0,
    last_message_id TEXT,
    last_message_snippet TEXT,
    last_message_at INTEGER,
    archived INTEGER NOT NULL DEFAULT 0,
    muted_until INTEGER,
    description TEXT,
    photo_url TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    chat_id TEXT NOT NULL,
    sender_jid TEXT NOT NULL,
    sender_name TEXT NOT NULL DEFAULT '',
    content TEXT NOT NULL DEFAULT '',
    has_media INTEGER NOT NULL DEFAULT 0,
    message_type TEXT NOT NULL DEFAULT '',
    media_caption TEXT NOT NULL DEFAULT '',
    media_path TEXT,
    media_mime TEXT,
    mentions TEXT,
    quoted_message_id TEXT,
    timestamp INTEGER NOT NULL,
    edited_at INTEGER,
    deleted_at INTEGER,
    status TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS contacts (
    jid TEXT PRIMARY KEY,
    name TEXT,
    push_name TEXT,
    phone TEXT,
    is_group INTEGER NOT NULL DEFAULT 0,
    last_interaction_at INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS group_participants (
    group_jid TEXT NOT NULL,
    participant_jid TEXT NOT NULL,
    is_admin INTEGER NOT NULL DEFAULT 0,
    added_at INTEGER NOT NULL,
    PRIMARY KEY (group_jid, participant_jid)
);

CREATE TABLE IF NOT EXISTS contact_scores (
    jid TEXT PRIMARY KEY,
    frequency INTEGER NOT NULL DEFAULT 0,
    recency INTEGER NOT NULL DEFAULT 0,
    reply_rate REAL NOT NULL DEFAULT 0.0,
    importance REAL NOT NULL DEFAULT 0.0,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS graph_nodes (
    jid TEXT PRIMARY KEY,
    node_type TEXT NOT NULL,
    name TEXT,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS graph_edges (
    from_jid TEXT NOT NULL,
    to_jid TEXT NOT NULL,
    edge_type TEXT NOT NULL,
    strength REAL NOT NULL DEFAULT 1.0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (from_jid, to_jid, edge_type)
);

CREATE TABLE IF NOT EXISTS memory (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL,
    content TEXT NOT NULL,
    context_jid TEXT,
    importance REAL NOT NULL DEFAULT 0.0,
    created_at INTEGER NOT NULL,
    expires_at INTEGER
);

CREATE TABLE IF NOT EXISTS outbound_queue (
    id TEXT PRIMARY KEY,
    chat_id TEXT NOT NULL,
    content TEXT NOT NULL,
    media_path TEXT,
    media_mime TEXT,
    quoted_message_id TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    priority INTEGER NOT NULL DEFAULT 0,
    scheduled_at INTEGER,
    created_at INTEGER NOT NULL,
    sent_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_messages_chat_id ON messages(chat_id);
CREATE INDEX IF NOT EXISTS idx_messages_timestamp ON messages(timestamp);
CREATE INDEX IF NOT EXISTS idx_graph_edges_from ON graph_edges(from_jid);
CREATE INDEX IF NOT EXISTS idx_graph_edges_to ON graph_edges(to_jid);
CREATE INDEX IF NOT EXISTS idx_memory_key ON memory(key);
CREATE INDEX IF NOT EXISTS idx_outbound_queue_status ON outbound_queue(status);

CREATE TABLE IF NOT EXISTS message_vectors (
    message_id TEXT PRIMARY KEY REFERENCES messages(id),
    embedding BLOB NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS status_history (
    id TEXT PRIMARY KEY,
    media_type TEXT NOT NULL,
    content TEXT NOT NULL DEFAULT '',
    media_path TEXT,
    thumbnail_path TEXT,
    bg_color TEXT DEFAULT '',
    font INTEGER DEFAULT 0,
    message_id TEXT,
    status TEXT NOT NULL DEFAULT 'posted',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_status_history_created_at ON status_history(created_at);
CREATE INDEX IF NOT EXISTS idx_status_history_message_id ON status_history(message_id);
CREATE INDEX IF NOT EXISTS idx_status_history_status ON status_history(status);
";

pub async fn migrate_outbound_queue(conn: &turso::Connection) {
    use turso::params::Params;
    let _ = conn.execute(
        "ALTER TABLE outbound_queue ADD COLUMN media_type TEXT NOT NULL DEFAULT ''",
        Params::Positional(vec![]),
    ).await;
}

pub async fn migrate_outbound_queue_v2(conn: &turso::Connection) {
    let _ = conn.execute(
        "ALTER TABLE outbound_queue ADD COLUMN metadata TEXT NOT NULL DEFAULT ''",
        turso::params::Params::Positional(vec![]),
    ).await;
}
