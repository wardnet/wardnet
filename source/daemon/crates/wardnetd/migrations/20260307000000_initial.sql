PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS admins (
    id            TEXT PRIMARY KEY NOT NULL,
    username      TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS sessions (
    id          TEXT PRIMARY KEY NOT NULL,
    admin_id    TEXT NOT NULL REFERENCES admins(id) ON DELETE CASCADE,
    token_hash  TEXT NOT NULL UNIQUE,
    created_at  TEXT NOT NULL,
    expires_at  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_sessions_token_hash ON sessions(token_hash);
CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON sessions(expires_at);

CREATE TABLE IF NOT EXISTS api_keys (
    id           TEXT PRIMARY KEY NOT NULL,
    label        TEXT NOT NULL,
    key_hash     TEXT NOT NULL,
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    last_used_at TEXT
);

CREATE TABLE IF NOT EXISTS devices (
    id           TEXT PRIMARY KEY NOT NULL,
    mac          TEXT NOT NULL UNIQUE,
    name         TEXT,
    hostname     TEXT,
    device_type  TEXT NOT NULL DEFAULT 'unknown',
    first_seen   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    last_seen    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    last_ip      TEXT NOT NULL,
    admin_locked INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_devices_mac ON devices(mac);
CREATE INDEX IF NOT EXISTS idx_devices_last_ip ON devices(last_ip);

CREATE TABLE IF NOT EXISTS tunnels (
    id             TEXT PRIMARY KEY NOT NULL,
    label          TEXT NOT NULL,
    country_code   TEXT NOT NULL,
    provider       TEXT,
    interface_name TEXT NOT NULL UNIQUE,
    endpoint       TEXT NOT NULL,
    status         TEXT NOT NULL DEFAULT 'down',
    last_handshake TEXT,
    bytes_tx       INTEGER NOT NULL DEFAULT 0,
    bytes_rx       INTEGER NOT NULL DEFAULT 0,
    created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS routing_rules (
    id          TEXT NOT NULL,
    device_id   TEXT PRIMARY KEY NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    target_json TEXT NOT NULL,
    created_by  TEXT NOT NULL DEFAULT 'user',
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS system_config (
    key        TEXT PRIMARY KEY NOT NULL,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- Seed default system config values.
INSERT OR IGNORE INTO system_config (key, value) VALUES
    ('global_default_policy', '{"type":"direct"}'),
    ('setup_completed', 'false'),
    ('daemon_version', '0.1.0');
