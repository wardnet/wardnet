-- Normalise `dns_query_log` onto integer lookup ids and an epoch timestamp.
-- See docs/adr/0034-query-log-normalisation.md.
--
-- The table is recreated EMPTY and the old one dropped: existing query-log
-- history is discarded. Rewriting 1.79M rows in place measured ~49 s, taken
-- with no DNS and no DHCP while systemd's start timeout runs. The log is
-- capped at 7 days by `dns_query_log_retention_days` and refills immediately.

DROP TABLE IF EXISTS dns_query_log;

-- One lookup per repeated column. `v` carries the text exactly as the old
-- column did, so the repository joins them back into the same wire shape.
CREATE TABLE IF NOT EXISTS lk_dns_domain     (id INTEGER PRIMARY KEY, v TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS lk_dns_client_ip  (id INTEGER PRIMARY KEY, v TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS lk_dns_device     (id INTEGER PRIMARY KEY, v TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS lk_dns_query_type (id INTEGER PRIMARY KEY, v TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS lk_dns_result     (id INTEGER PRIMARY KEY, v TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS lk_dns_upstream   (id INTEGER PRIMARY KEY, v TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS lk_dns_protocol   (id INTEGER PRIMARY KEY, v TEXT NOT NULL UNIQUE);

-- `timestamp` is whole-second Unix epoch. `latency_ms` stays REAL: integer
-- milliseconds would round every sub-millisecond cache hit to zero, which is
-- the range that distinguishes a cache hit from a forward.
--
-- No FK on `device_id`: `devices.id` is TEXT (so a reference saves nothing)
-- and the retention runner deletes unmanaged devices that log rows outlive.
--
-- No `created_at`: it recorded the batch-flush time, always within 11 s of
-- `timestamp`, and nothing read it.
CREATE TABLE IF NOT EXISTS dns_query_log (
    id            INTEGER PRIMARY KEY,
    timestamp     INTEGER NOT NULL,
    client_ip_id  INTEGER NOT NULL REFERENCES lk_dns_client_ip(id),
    domain_id     INTEGER NOT NULL REFERENCES lk_dns_domain(id),
    query_type_id INTEGER NOT NULL REFERENCES lk_dns_query_type(id),
    result_id     INTEGER NOT NULL REFERENCES lk_dns_result(id),
    upstream_id   INTEGER          REFERENCES lk_dns_upstream(id),
    latency_ms    REAL    NOT NULL DEFAULT 0,
    device_id     INTEGER          REFERENCES lk_dns_device(id),
    protocol_id   INTEGER NOT NULL REFERENCES lk_dns_protocol(id)
);

-- `timestamp` serves the retention DELETE; `device_id` serves the per-device
-- seek (measured 0.4 ms -> 145.6 ms without it).
--
-- `domain_id` is indexed because the daemon runs with `PRAGMA foreign_keys=ON`
-- (see `db.rs`). SQLite proves a parent DELETE safe by scanning the child table
-- once per deleted parent row, so pruning `lk_dns_domain` without this index
-- costs a full scan of this table per orphan: measured 33.5 s against 500k rows
-- and 2,000 orphans, versus 0.016 s with it. It also turns the domain substring
-- filter into a covering-index seek.
--
-- The old text indexes on `domain` and `client_ip` are gone for good: both
-- filters are leading-wildcard LIKE, which can never seek, so they only ever
-- covered a pagination COUNT that no longer runs.
--
-- `result_id` is indexed because the old `result` index was never dead weight:
-- it served `WHERE result = ? ORDER BY id DESC LIMIT n` as a reverse-ordered
-- seek with early exit, which is what the admin log's result dropdown does.
-- Without it that filter scans the table — measured 79.6 ms versus 0.3 ms for
-- a rare result, and that is a warm page cache, not an SD card.
CREATE INDEX IF NOT EXISTS idx_dns_query_log_timestamp ON dns_query_log(timestamp);
CREATE INDEX IF NOT EXISTS idx_dns_query_log_device_id ON dns_query_log(device_id);
CREATE INDEX IF NOT EXISTS idx_dns_query_log_domain_id ON dns_query_log(domain_id);
CREATE INDEX IF NOT EXISTS idx_dns_query_log_result_id ON dns_query_log(result_id);
