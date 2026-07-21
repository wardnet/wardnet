-- Expression indexes for the DNS analytics label dimensions added alongside
-- top-trackers and per-device timeseries:
--   * `$.company`   — top trackers blocked (dns.blocked.by_tracker)
--   * `$.device_id` — most-active devices and per-device series
--
-- top-N now ranks over whichever tier matches the query window (intraday /
-- hourly / daily), so the grouping expressions need index coverage on all
-- three tables — not just intraday. `$.device_id` already exists on
-- stats_intraday (see 20260715000000); this backfills the hourly and daily
-- tiers and adds `$.company` everywhere. Mirrors the existing outcome/domain/
-- client expression-index pattern; requires SQLite 3.9+ (already relied on).

CREATE INDEX IF NOT EXISTS stats_intraday_company
    ON stats_intraday (metric, json_extract(labels, '$.company'), bucket_ts);
CREATE INDEX IF NOT EXISTS stats_hourly_company
    ON stats_hourly (metric, json_extract(labels, '$.company'), hour_ts);
CREATE INDEX IF NOT EXISTS stats_daily_company
    ON stats_daily (metric, json_extract(labels, '$.company'), day);

CREATE INDEX IF NOT EXISTS stats_hourly_device_id
    ON stats_hourly (metric, json_extract(labels, '$.device_id'), hour_ts);
CREATE INDEX IF NOT EXISTS stats_daily_device_id
    ON stats_daily (metric, json_extract(labels, '$.device_id'), day);
