-- Per-tunnel speed test results (issue #239). Each row is one completed run
-- that measured throughput, latency and jitter twice: once over the direct
-- (unbound/WAN) path and once through the tunnel. Storing both legs in one row
-- keeps the comparison apples-to-apples (measured seconds apart). Kept in a
-- separate table — not extra columns on the hot `tunnels` row — to avoid
-- nullable columns there and keep history append-only.
CREATE TABLE IF NOT EXISTS tunnel_speed_test_results (
    id                       TEXT PRIMARY KEY NOT NULL,
    tunnel_id                TEXT NOT NULL REFERENCES tunnels(id) ON DELETE CASCADE,
    direct_throughput_mbps   REAL NOT NULL,
    tunnel_throughput_mbps   REAL NOT NULL,
    direct_latency_ms        REAL NOT NULL,
    tunnel_latency_ms        REAL NOT NULL,
    direct_jitter_ms         REAL NOT NULL,
    tunnel_jitter_ms         REAL NOT NULL,
    tested_at                TEXT NOT NULL
);

-- History queries fetch the most-recent N for one tunnel: ORDER BY tested_at DESC.
CREATE INDEX IF NOT EXISTS idx_speed_tests_tunnel_tested
    ON tunnel_speed_test_results(tunnel_id, tested_at DESC);
