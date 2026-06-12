ALTER TABLE devices ADD COLUMN dns_capture_enabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE devices ADD COLUMN dns_capture_cap_count INTEGER NOT NULL DEFAULT 1000;
ALTER TABLE devices ADD COLUMN dns_capture_cap_days  INTEGER NOT NULL DEFAULT 7;

CREATE TABLE IF NOT EXISTS dns_events (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    device_id   TEXT    NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    domain      TEXT    NOT NULL,
    status      TEXT    NOT NULL,
    captured_at TEXT    NOT NULL,
    sync_state  TEXT    NOT NULL DEFAULT 'pending'
);

CREATE INDEX IF NOT EXISTS idx_dns_events_device_id   ON dns_events(device_id);
CREATE INDEX IF NOT EXISTS idx_dns_events_captured_at ON dns_events(captured_at);
