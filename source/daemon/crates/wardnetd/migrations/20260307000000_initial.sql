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
    manufacturer TEXT,
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
    address        TEXT NOT NULL DEFAULT '[]',
    dns            TEXT NOT NULL DEFAULT '[]',
    peer_config    TEXT NOT NULL DEFAULT '{}',
    listen_port    INTEGER,
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

-- DHCP leases: active and expired leases assigned by the built-in DHCP server.
CREATE TABLE IF NOT EXISTS dhcp_leases (
    id          TEXT PRIMARY KEY NOT NULL,
    mac_address TEXT NOT NULL,
    ip_address  TEXT NOT NULL,
    hostname    TEXT,
    lease_start TEXT NOT NULL,
    lease_end   TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'active',
    device_id   TEXT REFERENCES devices(id) ON DELETE SET NULL,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
CREATE INDEX IF NOT EXISTS idx_dhcp_leases_mac ON dhcp_leases(mac_address);
CREATE INDEX IF NOT EXISTS idx_dhcp_leases_ip_status ON dhcp_leases(ip_address, status);

-- DHCP reservations: static MAC-to-IP bindings.
CREATE TABLE IF NOT EXISTS dhcp_reservations (
    id          TEXT PRIMARY KEY NOT NULL,
    mac_address TEXT NOT NULL UNIQUE,
    ip_address  TEXT NOT NULL UNIQUE,
    hostname    TEXT,
    description TEXT,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- DHCP lease log: audit trail for lease lifecycle events.
CREATE TABLE IF NOT EXISTS dhcp_lease_log (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    lease_id   TEXT NOT NULL,
    event_type TEXT NOT NULL,
    details    TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
CREATE INDEX IF NOT EXISTS idx_dhcp_lease_log_lease ON dhcp_lease_log(lease_id);

-- Seed default DHCP config.
INSERT OR IGNORE INTO system_config (key, value) VALUES
    ('dhcp_enabled', 'false'),
    ('dhcp_pool_start', '192.168.1.100'),
    ('dhcp_pool_end', '192.168.1.200'),
    ('dhcp_subnet_mask', '255.255.255.0'),
    ('dhcp_upstream_dns', '["1.1.1.1","8.8.8.8"]'),
    ('dhcp_lease_duration_secs', '86400'),
    ('dhcp_router_ip', '');
