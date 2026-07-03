pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS accounts (
    alias TEXT PRIMARY KEY,
    phone TEXT,
    status TEXT NOT NULL DEFAULT 'disconnected',
    error TEXT NOT NULL DEFAULT '',
    zeroclaw_agents TEXT NOT NULL DEFAULT '',
    bridge_dm_enabled INTEGER NOT NULL DEFAULT 1,
    bridge_group_enabled INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS agents (
    alias TEXT PRIMARY KEY,
    description TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS rate_counters (
    account TEXT NOT NULL,
    window_start INTEGER NOT NULL,
    sends INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (account, window_start)
);

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    account TEXT,
    action TEXT NOT NULL,
    params TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS health_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    account TEXT NOT NULL,
    event TEXT NOT NULL,
    details TEXT,
    timestamp INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS wa_version (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    primary_v INTEGER NOT NULL,
    secondary_v INTEGER NOT NULL,
    tertiary_v INTEGER NOT NULL,
    fetched_at INTEGER NOT NULL
);
";

pub async fn migrate_accounts(conn: &turso::Connection) {
    use turso::params::Params;
    let _ = conn.execute(
        "ALTER TABLE accounts ADD COLUMN error TEXT NOT NULL DEFAULT ''",
        Params::Positional(vec![]),
    ).await;
    let _ = conn.execute(
        "ALTER TABLE accounts ADD COLUMN zeroclaw_agents TEXT NOT NULL DEFAULT ''",
        Params::Positional(vec![]),
    ).await;
    let _ = conn.execute(
        "ALTER TABLE accounts ADD COLUMN bridge_enabled INTEGER NOT NULL DEFAULT 1",
        Params::Positional(vec![]),
    ).await;
    let _ = conn.execute(
        "ALTER TABLE accounts ADD COLUMN bridge_dm_enabled INTEGER NOT NULL DEFAULT 1",
        Params::Positional(vec![]),
    ).await;
    let _ = conn.execute(
        "ALTER TABLE accounts ADD COLUMN bridge_group_enabled INTEGER NOT NULL DEFAULT 1",
        Params::Positional(vec![]),
    ).await;
}
