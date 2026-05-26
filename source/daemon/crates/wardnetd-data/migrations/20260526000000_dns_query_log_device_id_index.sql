-- Fix: missing index on dns_query_log.device_id causes full table scans.
--
-- device_id is used in WHERE filters by query-log pagination and stats
-- roll-up queries. Without this index every filter on device_id scans
-- the entire table, and on high-volume installations the table can hold
-- millions of rows. The missing index is also the most likely cause of
-- slow-statement warnings (> 1s) seen on INSERT: SQLite with
-- foreign_keys=ON checks the parent table on every write for any column
-- that carries an implicit FK-like constraint, and a full scan on
-- millions of rows amplifies WAL write latency on Raspberry Pi SD cards.
--
-- Safe to add retroactively — CREATE INDEX IF NOT EXISTS is idempotent.
CREATE INDEX IF NOT EXISTS idx_dns_query_log_device_id
    ON dns_query_log(device_id);
