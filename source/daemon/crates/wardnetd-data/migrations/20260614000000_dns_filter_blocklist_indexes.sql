-- Add missing indexes to fix slow DNS filter rebuild query.
--
-- The query:
--   SELECT DISTINCT bd.domain
--   FROM dns_filter_blocked_domains bd
--   JOIN dns_filter_blocklists bl ON bd.blocklist_id = bl.id
--   WHERE bl.profile_id = ? AND bl.enabled = 1
--
-- was taking 3+ seconds because:
--   1. No index on dns_filter_blocked_domains(blocklist_id) → full table
--      scan for every matched blocklist row.
--   2. dns_filter_blocklists only had UNIQUE(profile_id, url); the
--      (profile_id, enabled) filter had to scan all profile rows to
--      apply the enabled predicate.
--
-- The covering index on (blocklist_id, domain) lets SQLite satisfy both
-- the join and the SELECT projection entirely from the index, without
-- touching the main table pages. The composite index on
-- (profile_id, enabled) gives the planner a tight seek for the WHERE
-- clause on the blocklists side.

CREATE INDEX IF NOT EXISTS idx_dns_filter_blocked_domains_blocklist_domain
    ON dns_filter_blocked_domains(blocklist_id, domain);

CREATE INDEX IF NOT EXISTS idx_dns_filter_blocklists_profile_enabled
    ON dns_filter_blocklists(profile_id, enabled);
