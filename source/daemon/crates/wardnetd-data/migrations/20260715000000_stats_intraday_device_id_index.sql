-- Expression index for top-N queries grouped by the `device_id` label
-- (issue: Top clients must rank by write-time device attribution, not by
-- DHCP-recyclable client IP). Mirrors the existing `stats_intraday_client`
-- index; the COALESCE(device_id, client) fallback form still scans, which
-- is fine — stats_intraday is pre-aggregated and bounded by 25 h retention.
CREATE INDEX IF NOT EXISTS stats_intraday_device_id
    ON stats_intraday (metric, json_extract(labels, '$.device_id'), bucket_ts);
