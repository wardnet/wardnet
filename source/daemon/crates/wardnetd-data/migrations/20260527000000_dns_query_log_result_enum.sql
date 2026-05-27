-- Clear DNS query log rows written under the old enum mapping (Cached/Local
-- variants). The table is a 7-day rolling log; data loss is acceptable.
-- After this migration all result strings in the DB match the canonical
-- resolver strings: forwarded, cache_hit, blocked, blocked_skipped,
-- rewritten, recursive, upstream_error.
DELETE FROM dns_query_log;
