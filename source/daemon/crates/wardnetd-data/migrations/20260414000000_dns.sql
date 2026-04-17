-- Milestone 1g: DNS Server & Ad Blocking
-- All DNS tables created upfront; populated incrementally across stages.

-- Authoritative local DNS zones (e.g. "lab", "home", "lan").
CREATE TABLE IF NOT EXISTS dns_zones (
    id         TEXT PRIMARY KEY NOT NULL,
    name       TEXT NOT NULL UNIQUE,
    enabled    INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- Custom local DNS records.
CREATE TABLE IF NOT EXISTS dns_custom_records (
    id          TEXT PRIMARY KEY NOT NULL,
    zone_id     TEXT REFERENCES dns_zones(id) ON DELETE SET NULL,
    domain      TEXT NOT NULL,
    record_type TEXT NOT NULL,  -- 'A', 'AAAA', 'CNAME', 'TXT', 'MX', 'SRV'
    value       TEXT NOT NULL,
    ttl         INTEGER NOT NULL DEFAULT 300,
    enabled     INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_dns_records_domain_type
    ON dns_custom_records(domain, record_type);

-- Conditional forwarding rules (domain -> specific upstream).
CREATE TABLE IF NOT EXISTS dns_conditional_rules (
    id         TEXT PRIMARY KEY NOT NULL,
    domain     TEXT NOT NULL UNIQUE,
    upstream   TEXT NOT NULL,
    enabled    INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- Ad blocking: blocklist metadata.
CREATE TABLE IF NOT EXISTS dns_blocklists (
    id            TEXT PRIMARY KEY NOT NULL,
    name          TEXT NOT NULL,
    url           TEXT NOT NULL UNIQUE,
    enabled       INTEGER NOT NULL DEFAULT 1,
    entry_count   INTEGER NOT NULL DEFAULT 0,
    last_updated  TEXT,
    cron_schedule TEXT NOT NULL DEFAULT '0 3 * * *',
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- Ad blocking: bulk domain storage per blocklist (rebuilt on each update).
CREATE TABLE IF NOT EXISTS dns_blocked_domains (
    domain       TEXT NOT NULL,
    blocklist_id TEXT NOT NULL REFERENCES dns_blocklists(id) ON DELETE CASCADE,
    PRIMARY KEY (domain, blocklist_id)
);
CREATE INDEX IF NOT EXISTS idx_dns_blocked_domains_domain
    ON dns_blocked_domains(domain);

-- Ad blocking: allowlist entries (override blocks).
CREATE TABLE IF NOT EXISTS dns_allowlist (
    id         TEXT PRIMARY KEY NOT NULL,
    domain     TEXT NOT NULL UNIQUE,
    reason     TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- Ad blocking: user-created AdGuard-syntax filter rules.
CREATE TABLE IF NOT EXISTS dns_custom_rules (
    id         TEXT PRIMARY KEY NOT NULL,
    rule_text  TEXT NOT NULL,
    enabled    INTEGER NOT NULL DEFAULT 1,
    comment    TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- Per-device ad blocking override.
CREATE TABLE IF NOT EXISTS dns_device_adblock (
    device_id  TEXT PRIMARY KEY NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    enabled    INTEGER NOT NULL DEFAULT 1,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- DNS query log (high-volume, auto-rotated by retention policy).
CREATE TABLE IF NOT EXISTS dns_query_log (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp  TEXT NOT NULL,
    client_ip  TEXT NOT NULL,
    domain     TEXT NOT NULL,
    query_type TEXT NOT NULL,
    result     TEXT NOT NULL,  -- 'forwarded', 'cached', 'blocked', 'local', 'recursive', 'error'
    upstream   TEXT,
    latency_ms REAL NOT NULL DEFAULT 0,
    device_id  TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
CREATE INDEX IF NOT EXISTS idx_dns_query_log_timestamp ON dns_query_log(timestamp);
CREATE INDEX IF NOT EXISTS idx_dns_query_log_client    ON dns_query_log(client_ip);
CREATE INDEX IF NOT EXISTS idx_dns_query_log_domain    ON dns_query_log(domain);
CREATE INDEX IF NOT EXISTS idx_dns_query_log_result    ON dns_query_log(result);

-- Seed default DNS config into system_config KV table.
INSERT OR IGNORE INTO system_config (key, value) VALUES
    ('dns_enabled', 'false'),
    ('dns_resolution_mode', 'forwarding'),
    ('dns_upstream_servers', '[{"address":"1.1.1.1","name":"Cloudflare","protocol":"udp"},{"address":"8.8.8.8","name":"Google","protocol":"udp"}]'),
    ('dns_cache_size', '10000'),
    ('dns_cache_ttl_min_secs', '0'),
    ('dns_cache_ttl_max_secs', '86400'),
    ('dns_dnssec_enabled', 'false'),
    ('dns_rebinding_protection', 'true'),
    ('dns_rate_limit_per_second', '0'),
    ('dns_ad_blocking_enabled', 'true'),
    ('dns_query_log_enabled', 'true'),
    ('dns_query_log_retention_days', '7');

-- Seed default blocklists (disabled until user enables them).
INSERT OR IGNORE INTO dns_blocklists (id, name, url, enabled, cron_schedule) VALUES
    ('00000000-0000-0000-0000-000000000001', 'Steven Black Unified', 'https://raw.githubusercontent.com/StevenBlack/hosts/master/hosts', 0, '0 3 * * *'),
    ('00000000-0000-0000-0000-000000000002', 'OISD Basic', 'https://small.oisd.nl/domainswild', 0, '0 3 * * *');

-- Seed default .lan zone for DHCP hostname integration.
INSERT OR IGNORE INTO dns_zones (id, name, enabled) VALUES
    ('00000000-0000-0000-0000-000000000010', 'lan', 1);
