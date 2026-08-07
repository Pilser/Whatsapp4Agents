pub const SCHEMA: &str = "
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
    created_at    TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_wb_apps_owner ON wb_apps(owner_key);

CREATE TABLE IF NOT EXISTS wb_records (
    id          INTEGER PRIMARY KEY,
    board_id    TEXT NOT NULL,
    owner_key   TEXT NOT NULL,
    seq         INTEGER NOT NULL,
    payload     TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_wb_records_board ON wb_records(board_id, seq);
CREATE INDEX IF NOT EXISTS idx_wb_records_owner ON wb_records(owner_key);
CREATE INDEX IF NOT EXISTS idx_wb_records_fts ON wb_records USING fts (payload);
";

pub const MIGRATIONS: &[&str] = &[
    "ALTER TABLE users ADD COLUMN name TEXT NOT NULL DEFAULT '';",
    "ALTER TABLE users ADD COLUMN role TEXT NOT NULL DEFAULT 'public';",
    "ALTER TABLE users ADD COLUMN password_hash TEXT NOT NULL DEFAULT '';",
];
