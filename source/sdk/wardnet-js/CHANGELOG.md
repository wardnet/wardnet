# @wardnet/js

## 0.7.0

### Minor Changes

- 84d8d98: **Breaking:** `ListQueryLogResponse.total` is replaced by `has_more: boolean`.

  `GET /api/dns/log` no longer returns a total entry count. Counting matching rows required an unbounded `COUNT(*)` over the query log — a full table scan measured at ~300 ms per page load, against ~1 ms for the page of rows it accompanied — because both text filters use a leading-wildcard `LIKE`, which SQLite cannot seek on.

  `has_more` is exact, not an estimate: the daemon fetches one row beyond the requested limit and reports whether it materialised.

  To migrate, drive "next page" from `has_more` rather than comparing an offset against `total`, and render a row range from the offset and `entries.length` instead of a count.

## 0.6.0

### Minor Changes

- Add `AccessRequestService` and `AnomalyService`, plus device and DNS-filter type updates.

  **Breaking:** `RuleRequestService` is removed, along with `RuleRequestKind`,
  `RuleRequestStatus`, `DeviceRuleRequest`, `CreateRuleRequestRequest` and
  `DecideRuleRequestRequest`. The rule-request inbox was replaced by a single
  access-request inbox; migrate to `AccessRequestService` and the corresponding
  `AccessRequest*` types.

  `SystemDiagnostic` and `RecentErrorsResponse` are also no longer exported from
  the package root.

## 0.5.0

### Minor Changes

- 7369d74: Add a `PrivateDnsService` for the encrypted-DNS (Private DNS) feature: read status and enable prerequisites, enable/disable, grant and revoke devices, the device-keyed `me` state, and a `profileUrl()` for the signed iOS `.mobileconfig`.
