-- Transport protocol per logged DNS query (issue #912).
--
-- 'udp' for the classic :53 path, 'dot' for queries arriving over the :853
-- DNS-over-TLS listener. Backfilling existing rows as 'udp' is accurate —
-- DoT did not exist before this migration.
ALTER TABLE dns_query_log ADD COLUMN protocol TEXT NOT NULL DEFAULT 'udp';

-- Filter column on a high-volume append-only table — indexed in the same
-- migration that introduces it, or reads degrade to full-table scans once the
-- log grows past a few thousand rows.
CREATE INDEX IF NOT EXISTS idx_dns_query_log_protocol ON dns_query_log (protocol);
