---
"@wardnet/js": minor
---

**Breaking:** `ListQueryLogResponse.total` is replaced by `has_more: boolean`.

`GET /api/dns/log` no longer returns a total entry count. Counting matching rows required an unbounded `COUNT(*)` over the query log — a full table scan measured at ~300 ms per page load, against ~1 ms for the page of rows it accompanied — because both text filters use a leading-wildcard `LIKE`, which SQLite cannot seek on.

`has_more` is exact, not an estimate: the daemon fetches one row beyond the requested limit and reports whether it materialised.

To migrate, drive "next page" from `has_more` rather than comparing an offset against `total`, and render a row range from the offset and `entries.length` instead of a count.
