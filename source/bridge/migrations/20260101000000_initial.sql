-- Bridge service initial schema.
--
-- installs    — one row per Pi registration; tracks the subdomain name,
--               Ed25519 public key, bearer-token hash, Cloudflare record IDs.
-- registration_log — rate-limit table: one row per registration attempt,
--               keyed on the caller's remote IP for the 5-per-day guard.

CREATE TABLE installs (
    id                  TEXT PRIMARY KEY NOT NULL,   -- UUIDv4
    name                TEXT NOT NULL UNIQUE,        -- subdomain slug, e.g. "happy-einstein"
    public_key          TEXT NOT NULL,               -- base64-encoded Ed25519 verifying key (32 raw bytes)
    token_hash          TEXT NOT NULL UNIQUE,        -- hex SHA-256 of the bearer token
    ip                  TEXT,                        -- last known public IPv4 address
    cf_a_record_id      TEXT,                        -- Cloudflare DNS record ID for the A record
    cf_acme_record_id   TEXT,                        -- Cloudflare DNS record ID for the ACME TXT record
    created_at          TEXT NOT NULL,               -- ISO 8601 UTC
    updated_at          TEXT NOT NULL                -- ISO 8601 UTC
);

CREATE TABLE registration_log (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    remote_ip   TEXT NOT NULL,
    created_at  TEXT NOT NULL   -- ISO 8601 UTC
);

-- Index used by the rate-limit query:
--   SELECT COUNT(*) FROM registration_log
--   WHERE remote_ip = ? AND created_at > datetime('now', '-1 day')
CREATE INDEX idx_registration_log_ip_time
    ON registration_log (remote_ip, created_at);
