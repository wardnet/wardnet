# @wardnet/js

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
