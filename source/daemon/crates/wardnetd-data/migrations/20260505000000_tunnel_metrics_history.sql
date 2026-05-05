-- Per-tunnel time-series throughput history.
--
-- The 5-second tunnel polling loop overwrites point-in-time `bytes_tx` /
-- `bytes_rx` snapshots on `tunnels`. To render a throughput chart we keep
-- decimated history in two tables:
--
-- * `tunnel_metrics_intraday` — one row per ~5 minutes, storing the
--   *delta* between consecutive samples. 48h rolling retention.
-- * `tunnel_metrics_daily` — one row per tunnel per day, summing the day's
--   intraday deltas. 12-month retention. Populated by the rollup runner.
--
-- ON DELETE CASCADE on tunnel_id keeps history clean when a tunnel is
-- removed; no extra service code needed for that path.
CREATE TABLE IF NOT EXISTS tunnel_metrics_intraday (
    tunnel_id      TEXT    NOT NULL REFERENCES tunnels(id) ON DELETE CASCADE,
    ts             INTEGER NOT NULL,
    bytes_tx_delta INTEGER NOT NULL,
    bytes_rx_delta INTEGER NOT NULL,
    PRIMARY KEY (tunnel_id, ts)
);
CREATE INDEX IF NOT EXISTS idx_tunnel_metrics_intraday_ts
    ON tunnel_metrics_intraday(ts);

CREATE TABLE IF NOT EXISTS tunnel_metrics_daily (
    tunnel_id      TEXT    NOT NULL REFERENCES tunnels(id) ON DELETE CASCADE,
    day            TEXT    NOT NULL,
    bytes_tx_total INTEGER NOT NULL,
    bytes_rx_total INTEGER NOT NULL,
    PRIMARY KEY (tunnel_id, day)
);
