-- Add the hourly stats tier.
--
-- Three-tier schema: stats_intraday (minute, 25 h) → stats_hourly (hour, 8 d)
-- → stats_daily (day, 13 mo). The hourly tier fills the gap so that
-- queries for ranges like "7 days" have real persisted data rather than
-- falling back to in-memory aggregation bounded by intraday retention.
--
-- Upsert semantics match stats_intraday:
--   counter: ON CONFLICT DO UPDATE SET value = value + excluded.value
--   gauge:   ON CONFLICT DO UPDATE SET value = excluded.value

CREATE TABLE IF NOT EXISTS stats_hourly (
    metric  TEXT    NOT NULL,
    labels  TEXT    NOT NULL DEFAULT '{}',
    hour_ts INTEGER NOT NULL,  -- Unix seconds, truncated to hour boundary
    value   REAL    NOT NULL DEFAULT 0,
    kind    TEXT    NOT NULL DEFAULT 'counter',
    PRIMARY KEY (metric, labels, hour_ts)
);

-- Expression indexes mirror stats_intraday so top-N queries and
-- label-dimension filters use index range scans on stats_hourly too.
CREATE INDEX IF NOT EXISTS stats_hourly_outcome
    ON stats_hourly (metric, json_extract(labels, '$.outcome'), hour_ts);
CREATE INDEX IF NOT EXISTS stats_hourly_domain
    ON stats_hourly (metric, json_extract(labels, '$.domain'),  hour_ts);
CREATE INDEX IF NOT EXISTS stats_hourly_client
    ON stats_hourly (metric, json_extract(labels, '$.client'),  hour_ts);
CREATE INDEX IF NOT EXISTS stats_hourly_tunnel_id
    ON stats_hourly (metric, json_extract(labels, '$.tunnel_id'), hour_ts);
