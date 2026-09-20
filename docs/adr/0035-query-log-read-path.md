---
status: accepted
date: 2026-09-05
issue: "#1200 (dns_query_log UI read is a full table scan)"
---

# ADR: The query-log read path seeks an index and pages on a cursor

The admin query log was served by `WHERE q.client_ip_id IN (SELECT id FROM
lk_dns_client_ip WHERE v LIKE ?) ORDER BY q.id DESC LIMIT ? OFFSET ?` over the
largest table on the box. `EXPLAIN QUERY PLAN` returned `SCAN dns_query_log`.

The ordering was never the problem: `id` is `INTEGER PRIMARY KEY`, so descending
id is the rowid walked backwards and costs nothing. The filter was. `client_ip_id`
had no index, so SQLite could only walk that rowid and test each row, and the
query survived on early termination — a client with recent traffic satisfies
`LIMIT` in the first few rows walked. The two cases where nothing terminates it
early are a client whose rows have aged towards the retention horizon and a deep
page, measured at 0.2–0.3 s standalone and 4.5–4.9 s in the field, where three
concurrent requests with `rows_returned=0` queued behind the query-log ingest
writer.

Two changes close it: the client filter reaches SQLite as something an index can
seek, and pagination is keyset rather than offset.

## The client filter

[ADR-0034](0034-query-log-normalisation.md) moved every repeated column onto
integer lookup ids and indexed `domain_id`, `result_id` and `device_id`.
`client_ip_id` is the one it left out, and the one the admin log's client filter
constrains on. It gets `idx_dns_query_log_client_ip_id`, single-column, for the
reason `device_id` is single-column: under an equality constraint SQLite walks a single-column
index's entries in rowid order, so `ORDER BY q.id DESC LIMIT n` is a backwards
read that stops at `n`, with no sort to pay for.

**The index alone is not enough, and this is the load-bearing measurement.**
Adding it does not fix the `IN (SELECT …)` form: with `ORDER BY q.id DESC` in
the query, SQLite prefers the backwards primary-key walk and declines the index
entirely. Measured on a synthetic table at the production shape — 1.37M rows, 24
distinct clients, all five indexes, `ANALYZE` run — the subquery form scans at
19.7 ms for a client with no recent rows while the resolved `q.client_ip_id = ?`
seeks at 0.3 ms, and the resolution that turns one into the other costs 20 µs.
Only the scalar equality unlocks the index, which is why the substring is
resolved before the log is queried rather than handed to SQLite as a subquery.

The equality is the part that takes work. The admin UI feeds this from a
free-text box, so a partial IP must narrow rather than match nothing — the filter
is a substring, and a substring is not an equality. The repository resolves it
against `lk_dns_client_ip` first and branches on how many clients it matched:

- **none** — return an empty page without querying the log at all;
- **one** — `q.client_ip_id = ?`, the seek above;
- **a handful** — `IN` over the resolved integers: still index seeks, but SQLite
  cannot carry one rowid order across several of them, so it sorts. The sort is
  bounded by `LIMIT` and by the matching rows;
- **more than 64** — hand the pattern back as a subquery. A substring matching
  that many clients also matches most of the log, where the backwards
  primary-key walk already exits early and a per-id seek only adds work.

The zero case is a second, smaller win on top of the seek. "No such client" is a
verdict `lk_dns_client_ip` can give from a few dozen rows; left to SQLite the
same verdict costs a pass over everything the other filters admit, because there
is no matching row for `LIMIT` to stop at — 19.5 ms against the same 1.37M rows.
It is worth being precise about which case that is, because the two are easy to
conflate. `lk_dns_client_ip` is never pruned, so a device that has gone quiet
still resolves to an id and still runs the log query. The short-circuit catches
only a substring matching no client the box has *ever* seen — a typo in the
filter box. The slow statement above is the aged-rows case, and the index is
what fixes it; the short-circuit would not have.

## Pagination

`OFFSET` makes SQLite walk and discard every row already read, so page cost grows
with depth by construction — 0.21 s at offset 200,000 against 0.00 s at the head.
The endpoint takes a `before` cursor instead: the page is the newest `limit` rows
with an id below it, which is the same seek at every depth. `next_cursor` is the
oldest id on the page, present exactly when `has_more` is true.

`DnsQueryLogEntry.id` already existed on the wire and was always sent as `0`;
it now carries the row's id, which is what the cursor addresses.

## What was rejected, and why

**`client_ip = ?` instead of a substring.** The issue proposes this, and it is
the wrong end of the trade. The filter is a free-text input where a partial IP
must narrow; equality would make "192.168.1" match nothing. Resolving the
substring against the lookup keeps the semantics and still produces an equality
for the log query — the scan the proposal was avoiding is gone either way.

**The index alone, keeping the `IN (SELECT …)` subquery.** The obvious minimal
change, and the one to reach for first: index the column, leave the query shape
as the normalisation left it, and let the planner do the rest. It does not work.
`ORDER BY q.id DESC` gives SQLite a second way to satisfy the query, and against
a subquery it takes it — the backwards primary-key walk — so the index sits
unused and the scan stays. The domain filter is not a counter-example: ADR-0034
measured a seek for a rare domain in a `WHERE`-only shape that has no ordering to
serve. This is why the resolution is worth a round trip, and it is the single
fact that most needs re-measuring before anyone simplifies this code.

**A composite `(client_ip_id, id)` index.** It looks more thorough and buys
nothing: the single-column index already serves the ordering under an equality
constraint. The trailing column would widen every entry across the largest table
on the box for no plan change.

**A bidirectional cursor (`after` as well as `before`).** Previous is answered
from the cursors the client has already used, so a second, ascending query would
double the endpoint's shapes to serve a button the client can already draw. The
cost is that Previous is only available for pages actually visited, which is what
a Previous button means.

**Keeping `offset` alongside `before`.** Two pagination models on one endpoint
means the slow one stays reachable and stays tested. The only consumers are the
JS SDK and the admin site, both in this repo.

**Pruning `lk_dns_client_ip`.** Still the open question ADR-0034 left, and this
change does not settle it. The lookup is now read on every filtered page rather
than only inside a subquery, which is an argument for keeping it small — but it
grows by tens of rows a year, and the `SELECT DISTINCT` scan a prune costs is
paid per table.

## Consequences

- The page footer reports what the page holds ("Showing 50 entries · page 3")
  rather than an absolute range. A cursor addresses a row, not a position, so
  there is no index to count from — and there was never a total to count toward.
- A filter matching most clients still costs a pass over the rows it matches.
  That is the pre-existing behaviour, not a regression: the walk it falls back
  to is the one that was there before, and it exits early at `LIMIT`.
- `dns_query_log` carries a fifth index. It is the narrowest of the five —
  integer ids against a lookup with a few dozen rows — and the table it sits on
  is the one that shrank 75% in ADR-0034.
