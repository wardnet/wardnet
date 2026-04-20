-- Persistent history of update install attempts.
--
-- One row per install attempt (auto or manual). Runtime state (pending
-- version, last check, channel) lives in `system_config` so it can be
-- edited via the same key-value surface as DHCP/DNS settings.
CREATE TABLE IF NOT EXISTS update_history (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    from_version  TEXT NOT NULL,
    to_version    TEXT NOT NULL,
    phase         TEXT NOT NULL,
    status        TEXT NOT NULL CHECK (status IN ('started','succeeded','failed','rolled_back')),
    error         TEXT,
    started_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    finished_at   TEXT
);
CREATE INDEX IF NOT EXISTS idx_update_history_started ON update_history(started_at DESC);

-- Seed default update runtime config. Auto-update is OFF by default — admins
-- must opt in via the web UI. Channel defaults to stable.
INSERT OR IGNORE INTO system_config (key, value) VALUES
    ('update_auto_update_enabled', 'false'),
    ('update_channel', 'stable'),
    ('update_last_check_at', ''),
    ('update_last_known_version', ''),
    ('update_pending_version', ''),
    ('update_previous_binary_path', ''),
    ('update_last_install_at', '');
