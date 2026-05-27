-- Drop the legacy per-tunnel metrics tables.
--
-- Tunnel throughput is now stored in the generic stats subsystem
-- (`stats_intraday` / `stats_daily`) under the `tunnel.bytes.tx` and
-- `tunnel.bytes.rx` metrics, labelled by `tunnel_id`. The dedicated
-- `tunnel_metrics_*` tables and their `TunnelMetricsRepository` are
-- retired.

DROP TABLE IF EXISTS tunnel_metrics_intraday;
DROP TABLE IF EXISTS tunnel_metrics_daily;
