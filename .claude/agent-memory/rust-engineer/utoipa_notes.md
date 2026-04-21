---
name: utoipa ToSchema gotchas
description: Known missing built-in ToSchema impls in utoipa 5.x and minimal workarounds used in wardnet-common
type: project
---

# utoipa ToSchema — types needing `#[schema(value_type = String)]`

utoipa 5.4 (features `axum_extras`, `chrono`, `uuid`) does NOT provide built-in `ToSchema`/`PartialSchema` impls for `std::net::Ipv4Addr`, `std::net::Ipv6Addr`, or `std::net::IpAddr`.

**Why:** These types serialize as strings via serde but utoipa has no default mapping. Deriving `ToSchema` on a struct that contains them fails with `the trait bound 'Ipv4Addr: PartialSchema' is not satisfied`.

**How to apply:** On each field of type `Ipv4Addr`/`IpAddr`/`Vec<Ipv4Addr>`/`Option<Ipv4Addr>`, add `#[schema(value_type = String)]` (or `Vec<String>` / `Option<String>` matching the wrapper). Works for tagged-union enum variant fields too (see `FilterAction::Rewrite` in `wardnet-common/src/dns.rs`).

Applied in: `wardnet-common/src/dhcp.rs` (DhcpConfig, DhcpLease, DhcpReservation) and `wardnet-common/src/dns.rs` (FilterAction).

Do NOT need hints for: `chrono::DateTime<Utc>`, `uuid::Uuid`, `Option<T>`, `Vec<T>`, tagged-union enums with plain struct variants, `#[serde(flatten)]` structs — these all work with a plain derive.
