# Normalising the DNS query log onto lookup ids

`dns_query_log` stored every column as repeated text. Across 1.79M rows there are
3,792 distinct domains, 29 device ids held as 36-byte UUID strings, 24 client IPs
and 8 results — 591 MB for the table and its indexes. Moving the seven repeated
columns onto `(id INTEGER PRIMARY KEY, v TEXT UNIQUE)` lookup tables and storing
`timestamp` as an epoch integer takes the subsystem to **146 MB (−75%)** and the
whole database from 1.07 GB to **~620 MB**, measured on a snapshot of the live box.

The normalisation alone reaches 109 MB. The remaining 37 MB is two integer
indexes — `domain_id` and `result_id` — that the measurement did not include and
that the daemon cannot run correctly without; see the consequences below. Quoting
the 109 MB figure describes a database that would stall its writer for minutes a
day and scan the whole log for a result filter.

**It is a space change, not a speed change.** The joins cost a fraction of a
millisecond on already-trivial queries and win on the expensive ones. Removing the
pagination `COUNT` is what fixed the 300 ms page load; this is orthogonal.

## What was rejected, and why

**A cache of lookup ids.** Ids are resolved per batch — one `SELECT … WHERE v IN
(…)` for misses, `INSERT … ON CONFLICT(v) DO NOTHING` for new values — with no LRU
in front. Batching already collapses 1.79M lookups a week into two statements per
one-second flush against a `UNIQUE` index, on a write pool that is idle in between.
A cache would protect a cost the batching already removed, and it is the sole
reason the orphan prune would need to be co-located with the writer: a prune that
deletes a domain whose id is still cached writes rows pointing at a row that no
longer exists. No cache, no hazard, no constraint on where the prune runs.

**An FK from `device_id` into `devices`.** It saves nothing — `devices.id` is
`TEXT PRIMARY KEY`, so referencing it stores the same 36 bytes. Worse, the device
retention runner deletes unmanaged devices after 30 days, so an FK forces a choice
between cascading (shredding log history as devices age out) and blocking (a
retention delete that fails against a 7-day log). Today log rows outlive their
device by design; a lookup table preserves that exactly. Note also that FKing
against the implicit `rowid` to get an integer for free is unsafe: `VACUUM` may
renumber rowids for any table whose primary key is not `INTEGER PRIMARY KEY`, and
this daemon runs incremental vacuum on a schedule.

**Integer enums for the closed columns.** `result` and `protocol` are closed sets,
so they could store a discriminant with no lookup table and no join. They don't.
The saving is identical either way — the row holds an integer regardless — so the
only thing at stake is a sub-millisecond join. Against that: `DnsQueryResult::slot`
exists as a compile-time exhaustiveness device over a freely-editable `ALL` array,
and persisting it would silently couple every historical row to that array's
*order*, with no test able to catch a reorder. Seven uniform lookup tables also
keep the database self-describing for a box with no `sqlite3` installed.

**FTS5 for the substring search.** A trigram index over the real domains costs
+231.8 MB and `MATCH` measured *slower* than `LIKE` (0.67 ms vs 0.10 ms). With only
~3,800 distinct domains, search resolves against the lookup table and feeds an
indexed integer `IN`, scanning thousands of rows instead of millions.

**Rewriting the rows in place.** A 2.4M-row copy was measured at ~49 s, taken with
no DNS and no DHCP while systemd's start timeout runs. The new table is therefore
created empty and the old one dropped: **existing query-log history is discarded.**
Accepted deliberately — the log is capped at 7 days, refills immediately, and the
box has a single operator who agreed to the loss. This must be called out in the
release notes for whichever version ships it.

## Consequences

- **Only `lk_dns_domain` is pruned.** It is the lookup that grows fast enough to
  matter — ~543 orphans/day, ~198k/year, against tens or hundreds a year for the
  rest — and its size is load-bearing: the substring scan is fast precisely
  because it reads thousands of rows. It is not the *only* load-bearing one:
  `lk_dns_client_ip` is scanned by a `LIKE` in the same way, it simply stays
  small enough that scanning it is free. The other six are **not** pruned, and two of them do grow without an
  upper bound: `lk_dns_device` gains an entry per device UUID, and MAC
  randomisation plus the 30-day device retention means a returning phone can
  mint a new one, while `lk_dns_client_ip` keeps every address ever seen and
  IPv6 privacy extensions rotate those roughly daily. That is a slow leak of
  kilobytes a year against a table measured in hundreds of megabytes, and it is
  left unreclaimed deliberately: pruning is not free — the expensive half is the
  `SELECT DISTINCT` scan of 1.79M rows, paid *per table*, on a
  single-connection write pool. Revisit it if either lookup ever grows large
  enough to slow the filter that scans it.
- **The prune lives inside `cleanup_query_log`, not behind its own trait method.**
  It is only meaningful immediately after the retention delete, has no independent
  cadence and no other caller. A separate method would put the word `lookup` in the
  service trait and the mock, and would move a mandatory ordering into a runner
  convention that no test protects.
- **`dns_query_log(domain_id)` and `(result_id)` are indexed, and that is not
  optional.** Together they cost ~37 MB, which is the whole difference between
  the 109 MB the normalisation reaches on its own and the 146 MB that ships.
  `result_id` restores what the old `result` index did — a reverse-ordered seek
  with early exit for the admin log's result dropdown, measured 79.6 ms versus
  0.3 ms for a rare result on a warm cache — and `domain_id` is what keeps the
  prune from stalling the writer, below.

  `domain_id` also makes the substring search **faster for narrow patterns and
  slower for broad ones**, which is worth stating plainly because it is easy to
  read the headline as a uniform win. Measured over 1.79M rows: a pattern
  matching one domain goes 141 ms → 0.3 ms; a pattern matching ~3,700 domains
  goes 1.8 ms → 17.9 ms, because the planner seeks per id and sorts into a temp
  b-tree for `ORDER BY id DESC` instead of scanning in rowid order and exiting
  early at `LIMIT`. Suppressing the index for that predicate (`+q.domain_id`)
  would buy back the 16 ms and give up the 140 ms; it is not worth it, and the
  losing case reads index pages where the winning case would read the table.
- **The prune is fast only if two separate things hold.** Use
  `DELETE FROM lk_dns_domain WHERE id NOT IN (SELECT DISTINCT domain_id FROM dns_query_log)`.
  The `NOT EXISTS` form reads more naturally, is a correlated subquery that
  rescans the log per lookup row, and measured 135 s — a two-minute writer stall.
  Separately, **`dns_query_log(domain_id)` must stay indexed**: the daemon runs
  with `PRAGMA foreign_keys=ON`, and SQLite proves a parent DELETE safe by
  scanning the child table once per deleted row unless the child key is indexed.
  Measured on 500k rows and 2,000 orphans: 0.061 s with foreign keys off, **33.5 s
  with them on and no index**, 0.016 s with the index. Any prune timing measured
  in the `sqlite3` CLI is measured with foreign keys *off*, because that is the
  CLI's default and not the daemon's — the two are not comparable. The index
  costs ~20 MB, and with `result_id` accounts for the 146 MB this lands at.
- **Timestamps are whole seconds, deliberately.** `QueryLogRow.timestamp` is a
  `DateTime<Utc>` and the producer truncates sub-second precision explicitly. That
  rule previously lived by accident inside a format string. Without the truncation
  the streamed event would carry nanoseconds the epoch-second column cannot return,
  so the same query would render differently live and on reload.
- **The wire is unchanged.** `QueryLogEvent.timestamp` becomes `DateTime<Utc>` for
  consistency with `DnsQueryLogEntry`, which was already typed that way. It is a
  WebSocket message and not part of the OpenAPI surface, and chrono renders a
  whole-second UTC value as `…:56Z` — the bytes it already emitted. No schema diff,
  no SDK bump.
- **Nothing above `wardnetd-data` learns that lookup tables exist.** The repository
  joins them on read and resolves them on write, which is what keeps the API
  contract untouched.
- **`created_at` is dropped.** It recorded the batch-flush time, always within 11 s
  of `timestamp`, and nothing ever read it.
