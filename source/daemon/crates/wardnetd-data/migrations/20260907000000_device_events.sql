-- Observational per-device event log, backing the device connectivity
-- timeline and the address-churn detector (issue #1338).
--
-- Presence transitions, address changes and zone/routing rebinds are published
-- as domain events and otherwise dropped; conntrack flushes only ever reached a
-- log line. Without a home for them the timeline cannot answer "did it depart?",
-- which is the question a connectivity investigation turns on.
--
-- Shaped after dhcp_lease_log: `mac` is denormalised onto every row and there is
-- deliberately NO foreign key to `devices`. `devices.id` is TEXT so an FK buys
-- nothing, and device retention deletes rows this log must outlive — the same
-- reasoning ADR 0034 records for dns_query_log.
--
-- `created_at` is epoch seconds rather than ISO text (ADR 0034): the column is
-- only ever range-scanned and compared, never displayed raw.
CREATE TABLE IF NOT EXISTS device_events (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    device_id  TEXT    NOT NULL,
    mac        TEXT    NOT NULL,
    kind       TEXT    NOT NULL,
    details    TEXT,
    created_at INTEGER NOT NULL
);

-- Single-column, for the reason ADR 0035 records: under an equality constraint
-- SQLite walks the index in rowid order, and rowid order is insertion order,
-- which is `created_at` order. A trailing `created_at` would widen every entry
-- to buy an ordering the walk already provides.
CREATE INDEX IF NOT EXISTS idx_device_events_device_id
    ON device_events(device_id);

-- The age prune scans by time across all devices, which the device index cannot
-- serve.
CREATE INDEX IF NOT EXISTS idx_device_events_created_at
    ON device_events(created_at);
