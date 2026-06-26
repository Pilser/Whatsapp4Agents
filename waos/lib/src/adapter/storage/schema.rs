pub(crate) const SCHEMA_SQL: &str = "
CREATE TABLE IF NOT EXISTS device (
    id INTEGER PRIMARY KEY,
    lid TEXT,
    pn TEXT,
    registration_id INTEGER NOT NULL,
    noise_key BLOB NOT NULL,
    identity_key BLOB NOT NULL,
    signed_pre_key BLOB NOT NULL,
    signed_pre_key_id INTEGER NOT NULL,
    signed_pre_key_signature BLOB NOT NULL,
    adv_secret_key BLOB NOT NULL,
    account BLOB,
    push_name TEXT NOT NULL,
    app_version_primary INTEGER NOT NULL,
    app_version_secondary INTEGER NOT NULL,
    app_version_tertiary INTEGER NOT NULL,
    app_version_last_fetched_ms INTEGER NOT NULL,
    edge_routing_info BLOB,
    props_hash TEXT,
    next_pre_key_id INTEGER NOT NULL DEFAULT 0,
    server_has_prekeys INTEGER NOT NULL DEFAULT 0,
    nct_salt BLOB,
    server_cert_chain BLOB,
    login_counter INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS identities (
    address TEXT NOT NULL,
    key BLOB NOT NULL,
    device_id INTEGER NOT NULL,
    PRIMARY KEY (address, device_id)
);

CREATE TABLE IF NOT EXISTS sessions (
    address TEXT NOT NULL,
    record BLOB NOT NULL,
    device_id INTEGER NOT NULL,
    PRIMARY KEY (address, device_id)
);

CREATE TABLE IF NOT EXISTS prekeys (
    id INTEGER NOT NULL,
    key BLOB NOT NULL,
    uploaded INTEGER NOT NULL DEFAULT 0,
    device_id INTEGER NOT NULL,
    PRIMARY KEY (id, device_id)
);

CREATE TABLE IF NOT EXISTS signed_prekeys (
    id INTEGER NOT NULL,
    record BLOB NOT NULL,
    device_id INTEGER NOT NULL,
    PRIMARY KEY (id, device_id)
);

CREATE TABLE IF NOT EXISTS sender_keys (
    address TEXT NOT NULL,
    record BLOB NOT NULL,
    device_id INTEGER NOT NULL,
    PRIMARY KEY (address, device_id)
);

CREATE TABLE IF NOT EXISTS app_state_keys (
    key_id BLOB NOT NULL,
    key_data BLOB NOT NULL,
    device_id INTEGER NOT NULL,
    PRIMARY KEY (key_id, device_id)
);

CREATE TABLE IF NOT EXISTS app_state_versions (
    name TEXT NOT NULL,
    state_data BLOB NOT NULL,
    device_id INTEGER NOT NULL,
    PRIMARY KEY (name, device_id)
);

CREATE TABLE IF NOT EXISTS app_state_mutation_macs (
    name TEXT NOT NULL,
    version INTEGER NOT NULL,
    index_mac BLOB NOT NULL,
    value_mac BLOB NOT NULL,
    device_id INTEGER NOT NULL,
    PRIMARY KEY (name, index_mac, device_id)
);

CREATE TABLE IF NOT EXISTS lid_pn_mapping (
    lid TEXT NOT NULL,
    phone_number TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    learning_source TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    device_id INTEGER NOT NULL,
    PRIMARY KEY (lid, device_id)
);

CREATE TABLE IF NOT EXISTS skdm_recipients (
    group_jid TEXT NOT NULL,
    device_jid TEXT NOT NULL,
    device_id INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (group_jid, device_jid, device_id)
);

CREATE TABLE IF NOT EXISTS device_registry (
    user_id TEXT NOT NULL,
    devices_json TEXT NOT NULL,
    timestamp INTEGER NOT NULL,
    phash TEXT,
    raw_id INTEGER,
    device_id INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, device_id)
);

CREATE TABLE IF NOT EXISTS sender_key_devices (
    group_jid TEXT NOT NULL,
    device_jid TEXT NOT NULL,
    has_key INTEGER NOT NULL,
    device_id INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (group_jid, device_jid, device_id)
);

CREATE TABLE IF NOT EXISTS sent_messages (
    chat_jid TEXT NOT NULL,
    message_id TEXT NOT NULL,
    payload BLOB NOT NULL,
    device_id INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (chat_jid, message_id, device_id)
);

CREATE TABLE IF NOT EXISTS base_keys (
    address TEXT NOT NULL,
    message_id TEXT NOT NULL,
    base_key BLOB NOT NULL,
    device_id INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (address, message_id, device_id)
);

CREATE TABLE IF NOT EXISTS sender_key_status (
    group_jid TEXT NOT NULL,
    participant TEXT NOT NULL,
    device_id INTEGER NOT NULL,
    marked_at INTEGER NOT NULL,
    PRIMARY KEY (group_jid, participant, device_id)
);

CREATE TABLE IF NOT EXISTS tc_tokens (
    jid TEXT NOT NULL,
    token BLOB NOT NULL,
    token_timestamp INTEGER NOT NULL,
    sender_timestamp INTEGER,
    device_id INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (jid, device_id)
);

CREATE INDEX IF NOT EXISTS idx_sent_messages_device_created
    ON sent_messages(device_id, created_at);
";
