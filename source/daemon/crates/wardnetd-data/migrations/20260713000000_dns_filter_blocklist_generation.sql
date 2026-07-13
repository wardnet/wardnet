-- Generation-based blocklist imports.
--
-- `replace_blocklist_domains` used to DELETE every row for a blocklist and
-- re-INSERT the new set inside a single transaction. That kept the daemon's
-- *sole* write connection (WRITE_MAX_CONNECTIONS = 1) busy for the whole
-- import. A threat-intel feed such as HaGeZi TIF is ~1.9M domains, so the
-- writer was monopolised for minutes and every other writer in the process
-- (stats flush, DHCP leases, DNS query log, heartbeat) failed with
-- "pool timed out while waiting for an open connection". Health then went
-- DOWN, the soft watchdog withheld sd_notify(WATCHDOG=1), and systemd
-- SIGABRT'd the daemon mid-import.
--
-- The fix streams the new domains in small, independently-committed batches so
-- the writer is released between them. To keep the swap atomic despite the
-- batching, rows carry a `generation` and each blocklist names the generation
-- readers should see. An import writes generation N+1 in the background, flips
-- `active_generation` in one tiny transaction, then reclaims the superseded
-- rows in batches. Readers only ever see the fully written generation.
--
-- `generation` MUST be part of the primary key: a refresh re-imports a list
-- that overlaps almost entirely with its previous contents, so the new
-- generation is written while the old one is still present. Under the old
-- `PRIMARY KEY (domain, blocklist_id)` every real refresh would die with
-- "UNIQUE constraint failed".
--
-- The table is therefore recreated rather than altered (SQLite cannot change a
-- primary key in place) — and it is recreated EMPTY.
--
-- Copying the existing rows across would work, but it costs ~49s of blocked
-- startup on a Pi holding 2M domains (measured), and migrations run before
-- READY=1, so that is 49s during which the whole gateway serves no DNS or DHCP
-- and systemd's start timeout is ticking. It also scales with the user's lists,
-- so a heavier setup fares worse.
--
-- `dns_filter_blocked_domains` is a *cache* of remote lists, not user data:
-- every row is reconstructible by re-downloading the source URL. So the rows
-- are dropped and each blocklist is marked never-imported (`last_updated =
-- NULL`, `entry_count = 0`), which the DNS filter runner treats as due, and it
-- re-imports them in the background through the new non-starving batched path.
-- Startup stays fast; the lists refill without blocking anything.
--
-- The cost is honest and must stay visible: for a few minutes after this
-- upgrade the blocklists are empty and therefore not enforced. The daemon
-- reports `filters_ready = false` while any enabled list is still unimported,
-- and the admin UI shows a warning banner until it flips.

ALTER TABLE dns_filter_blocklists
    ADD COLUMN active_generation INTEGER NOT NULL DEFAULT 0;

DROP TABLE dns_filter_blocked_domains;

CREATE TABLE dns_filter_blocked_domains (
    domain       TEXT NOT NULL,
    blocklist_id TEXT NOT NULL REFERENCES dns_filter_blocklists(id) ON DELETE CASCADE,
    generation   INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (domain, blocklist_id, generation)
);

-- Only the covering index the read path needs. A standalone index on (domain)
-- used to exist; it is redundant now that `domain` is the leading column of the
-- primary key's index (SQLite serves `WHERE domain = ?` from
-- sqlite_autoindex_..._1 just as well), and no query filters on domain alone —
-- domain matching happens in the in-memory DnsFilter, not in SQL. Recreating it
-- would cost a third B-tree write on every one of the ~1.9M rows of each
-- import, for nothing.
CREATE INDEX IF NOT EXISTS idx_dns_filter_blocked_domains_blocklist_gen_domain
    ON dns_filter_blocked_domains(blocklist_id, generation, domain);

-- Mark every list never-imported so the runner's cron check treats it as due
-- (`last_updated: None => true`) and refills it on the next tick.
UPDATE dns_filter_blocklists
   SET entry_count = 0,
       last_updated = NULL,
       active_generation = 0;
