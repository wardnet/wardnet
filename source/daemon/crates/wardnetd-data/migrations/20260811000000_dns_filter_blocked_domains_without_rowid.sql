-- Collapse `dns_filter_blocked_domains` onto its primary key.
--
-- The table is nothing but its primary key: three columns, all three in the
-- key, no payload. With an implicit rowid it is stored twice — once as the
-- table btree keyed by rowid, once as `sqlite_autoindex_..._1` keyed by
-- (domain, blocklist_id, generation) and carrying the rowid back. Measured on
-- a live box holding 2.4M domains:
--
--   sqlite_autoindex_dns_filter_blocked_domains_1        320.2 MB
--   dns_filter_blocked_domains                           155.2 MB
--   idx_dns_filter_blocked_domains_blocklist_gen_domain  173.4 MB
--
-- `WITHOUT ROWID` makes the primary key the table, so the first two become one
-- structure instead of two. Reproduced on a synthetic 400k-row copy of this
-- exact schema: 83.8 MB against 52.9 MB, a 37% reduction, which is ~186 MB at
-- the row count above. No query plan changes — both indexes keep the same
-- leading columns, so the resolution read (PK, leads on `domain`) and the
-- delete-by-generation scan (secondary index, leads on `blocklist_id`) are
-- served exactly as before.
--
-- SQLite cannot convert a table in place, so it is recreated — and, following
-- the generation migration that established the pattern, recreated EMPTY.
-- Copying 2.4M rows across costs ~49s of blocked startup on a Pi (measured
-- then), and migrations run before READY=1: 49s of no DNS and no DHCP with
-- systemd's start timeout ticking. `dns_filter_blocked_domains` is a cache of
-- remote lists, not user data — every row is reconstructible from its source
-- URL — so the rows are dropped and each blocklist marked never-imported
-- (`last_updated = NULL`, `entry_count = 0`), which the DNS filter runner
-- treats as due and re-imports in the background through the batched path.
--
-- The cost is the same one that migration accepted and must stay visible: for
-- a few minutes after this upgrade the blocklists are empty and therefore not
-- enforced. The daemon reports `filters_ready = false` while any enabled list
-- is still unimported, and the admin UI shows a warning banner until it flips.
--
-- Note that dropping the rows hands ~650 MB to the freelist on a box the size
-- of the one measured. That space comes back through the daily incremental
-- vacuum rather than immediately.

DROP TABLE dns_filter_blocked_domains;

CREATE TABLE dns_filter_blocked_domains (
    domain       TEXT NOT NULL,
    blocklist_id TEXT NOT NULL REFERENCES dns_filter_blocklists(id) ON DELETE CASCADE,
    generation   INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (domain, blocklist_id, generation)
) WITHOUT ROWID;

-- Unchanged from the generation migration: the one covering index the
-- delete-by-generation path needs, leading on `blocklist_id` where the primary
-- key leads on `domain`.
CREATE INDEX IF NOT EXISTS idx_dns_filter_blocked_domains_blocklist_gen_domain
    ON dns_filter_blocked_domains(blocklist_id, generation, domain);

UPDATE dns_filter_blocklists
   SET entry_count = 0,
       last_updated = NULL,
       active_generation = 0;
