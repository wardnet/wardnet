-- Generic pre-aggregating stats subsystem.
--
-- Two-tier schema mirrors the tunnel_metrics pattern:
-- * stats_intraday — one row per (metric, labels, minute-bucket). 48h retention.
-- * stats_daily    — one row per (metric, labels, day). 13-month retention.
--
-- Counter upserts accumulate: ON CONFLICT DO UPDATE SET value = value + excluded.value
-- Gauge upserts overwrite:    ON CONFLICT DO UPDATE SET value = excluded.value
--
-- Expression indexes on common label dimensions enable json_extract() filters
-- and top-N queries to use index range scans rather than full table scans.
-- Requires SQLite 3.9+ (expression indexes); Raspberry Pi OS Bookworm ships 3.40+.

CREATE TABLE IF NOT EXISTS stats_intraday (
    metric    TEXT    NOT NULL,
    labels    TEXT    NOT NULL DEFAULT '{}',
    bucket_ts INTEGER NOT NULL,
    value     REAL    NOT NULL DEFAULT 0,
    kind      TEXT    NOT NULL DEFAULT 'counter',
    PRIMARY KEY (metric, labels, bucket_ts)
);

CREATE INDEX IF NOT EXISTS stats_intraday_outcome
    ON stats_intraday (metric, json_extract(labels, '$.outcome'), bucket_ts);
CREATE INDEX IF NOT EXISTS stats_intraday_domain
    ON stats_intraday (metric, json_extract(labels, '$.domain'),  bucket_ts);
CREATE INDEX IF NOT EXISTS stats_intraday_client
    ON stats_intraday (metric, json_extract(labels, '$.client'),  bucket_ts);

CREATE TABLE IF NOT EXISTS stats_daily (
    metric TEXT NOT NULL,
    labels TEXT NOT NULL DEFAULT '{}',
    day    TEXT NOT NULL,
    value  REAL NOT NULL DEFAULT 0,
    kind   TEXT NOT NULL DEFAULT 'counter',
    PRIMARY KEY (metric, labels, day)
);

CREATE INDEX IF NOT EXISTS stats_daily_outcome
    ON stats_daily (metric, json_extract(labels, '$.outcome'), day);
CREATE INDEX IF NOT EXISTS stats_daily_domain
    ON stats_daily (metric, json_extract(labels, '$.domain'),  day);
CREATE INDEX IF NOT EXISTS stats_daily_client
    ON stats_daily (metric, json_extract(labels, '$.client'),  day);
