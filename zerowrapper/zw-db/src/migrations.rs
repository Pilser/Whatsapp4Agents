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
";

pub const MIGRATIONS: &[&str] = &[
    "ALTER TABLE users ADD COLUMN name TEXT NOT NULL DEFAULT '';",
    "ALTER TABLE users ADD COLUMN role TEXT NOT NULL DEFAULT 'public';",
    "ALTER TABLE users ADD COLUMN password_hash TEXT NOT NULL DEFAULT '';",
];
