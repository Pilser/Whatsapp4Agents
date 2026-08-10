pub const SYSTEM_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS users (
    id          TEXT PRIMARY KEY,
    external_id TEXT UNIQUE,
    name        TEXT NOT NULL DEFAULT '',
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS sessions (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL REFERENCES users(id),
    agent_alias TEXT NOT NULL,
    account     TEXT,
    chat_id     TEXT,
    channel     TEXT NOT NULL DEFAULT 'web',
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    last_active TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS wb_keys (
    bucket      TEXT PRIMARY KEY,
    board_id    TEXT NOT NULL,
    key_hash    TEXT NOT NULL,
    role        TEXT NOT NULL DEFAULT 'reader',
    writer      TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    revoked_at  TEXT
);
CREATE INDEX IF NOT EXISTS idx_wb_keys_board ON wb_keys(board_id);
CREATE INDEX IF NOT EXISTS idx_wb_keys_hash ON wb_keys(key_hash);

CREATE TABLE IF NOT EXISTS wb_hook_deliveries (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    board_id      TEXT NOT NULL,
    url           TEXT NOT NULL,
    event         TEXT NOT NULL,
    attempts      INTEGER NOT NULL DEFAULT 0,
    last_status   TEXT,
    next_attempt_at TEXT,
    delivered_at  TEXT,
    created_at    TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_wb_hook_due_idx ON wb_hook_deliveries(delivered_at, next_attempt_at);

CREATE TABLE IF NOT EXISTS srv_board_index (
    board_id   TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    owner_key  TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_srv_board_index_session ON srv_board_index(session_id);

CREATE TABLE IF NOT EXISTS wb_public_index (
    slug       TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    page_no    INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
";

pub const SESSION_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS whiteboard_pages (
    session_id TEXT NOT NULL,
    page_no    INTEGER NOT NULL,
    title      TEXT NOT NULL DEFAULT '',
    body_md    TEXT NOT NULL DEFAULT '',
    public     INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (session_id, page_no)
);

CREATE TABLE IF NOT EXISTS whiteboard_meta (
    session_id  TEXT PRIMARY KEY,
    active_page INTEGER,
    locked      INTEGER NOT NULL DEFAULT 0,
    rev         INTEGER NOT NULL DEFAULT 0,
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS whiteboard_public_pages (
    id         TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    page_no    INTEGER NOT NULL,
    body_md    TEXT NOT NULL,
    pins       TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_wb_public_session ON whiteboard_public_pages(session_id);

CREATE TABLE IF NOT EXISTS wb_apps (
    board_id      TEXT PRIMARY KEY,
    owner_key     TEXT NOT NULL,
    title         TEXT NOT NULL,
    schema_json   TEXT,
    public_reads  INTEGER NOT NULL DEFAULT 0,
    unique_key    TEXT,
    created_at    TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_wb_apps_owner ON wb_apps(owner_key);

CREATE TABLE IF NOT EXISTS wb_records (
    id          INTEGER PRIMARY KEY,
    board_id    TEXT NOT NULL,
    owner_key   TEXT NOT NULL,
    seq         INTEGER NOT NULL,
    payload     TEXT NOT NULL,
    writer      TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_wb_records_board ON wb_records(board_id, seq);
CREATE INDEX IF NOT EXISTS idx_wb_records_owner ON wb_records(owner_key);
CREATE INDEX IF NOT EXISTS idx_wb_records_fts ON wb_records USING fts (payload) WITH (tokenizer='ngram');

CREATE TABLE IF NOT EXISTS wb_hooks (
    board_id TEXT NOT NULL,
    url      TEXT NOT NULL,
    secret   TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (board_id, url)
);
CREATE INDEX IF NOT EXISTS idx_wb_hooks_board ON wb_hooks(board_id);

CREATE TABLE IF NOT EXISTS wb_links (
    board_id     TEXT PRIMARY KEY,
    from_key     TEXT NOT NULL,
    parent_board TEXT NOT NULL,
    parent_key   TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS wb_audit (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    board_id     TEXT NOT NULL,
    event        TEXT NOT NULL,
    actor        TEXT,
    writer       TEXT,
    payload_hash TEXT,
    ts           TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_wb_audit_board ON wb_audit(board_id, id);
";

pub const SYSTEM_MIGRATIONS: &[&str] = &[
    "ALTER TABLE users ADD COLUMN name TEXT NOT NULL DEFAULT '';",
    "ALTER TABLE users ADD COLUMN role TEXT NOT NULL DEFAULT 'public';",
    "ALTER TABLE users ADD COLUMN password_hash TEXT NOT NULL DEFAULT '';",
];

pub const SESSION_MIGRATIONS: &[&str] = &[
    "ALTER TABLE wb_apps ADD COLUMN unique_key TEXT;",
    "ALTER TABLE wb_records ADD COLUMN writer TEXT;",
    "DROP INDEX IF EXISTS idx_wb_records_fts; \
     CREATE INDEX idx_wb_records_fts ON wb_records USING fts (payload) WITH (tokenizer='ngram');",
    "ALTER TABLE wb_apps ADD COLUMN rate_json TEXT;",
    "ALTER TABLE wb_apps ADD COLUMN ttl_seconds INTEGER;",
    "ALTER TABLE wb_apps ADD COLUMN ttl_field TEXT;",
    "ALTER TABLE wb_apps ADD COLUMN audit INTEGER NOT NULL DEFAULT 0;",
];
