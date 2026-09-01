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

-- Only two indexes survive. `timestamp` serves the retention DELETE, and
-- `device_id` serves the per-device seek (measured 0.4 ms -> 145.6 ms without
-- it). The old covering indexes on domain / client_ip / result existed solely
-- for a pagination COUNT that no longer runs, and cost 152 MB.
CREATE INDEX IF NOT EXISTS idx_dns_query_log_timestamp ON dns_query_log(timestamp);
CREATE INDEX IF NOT EXISTS idx_dns_query_log_device_id ON dns_query_log(device_id);
