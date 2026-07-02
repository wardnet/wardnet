# Rust Engineer Agent Memory

## Project Structure
- Workspace root: `source/daemon/`
- Main crate: `crates/wardnetd/` (lib + bin)
- Types crate: `crates/wardnet-types/`
- CLI crate: `crates/wctl/`

## Auth Context Pattern
- `auth_context::with_context(AuthContext::Admin { admin_id: Uuid::nil() }, future)` wraps background task calls to services
- All service methods call `auth_context::require_admin()?;` or `require_authenticated()?;` as first line
- Tests use a helper `as_admin(future).await` for ergonomics

## Test Conventions
- Tests in separate files: `src/<layer>/tests/<module>.rs`
- Never inline `#[cfg(test)] mod tests {}` in source files
- Routing service tests: `src/service/tests/routing.rs`
- Integration tests: `src/tests/routing_listener.rs`, `src/tests/tunnel_idle.rs`

## Key Modules
- `service/routing.rs`: Policy routing engine, 3-phase apply_rule (check, tunnel ops, apply)
- `tunnel_idle.rs`: Background watcher for idle tunnel teardown
- `routing_listener.rs`: Background event dispatcher for routing changes
- `auth_context.rs`: Task-local auth context with `with_context` + `require_admin`

## Clippy Notes
- Project uses `-D warnings` for clippy
- Collapsible if statements caught by clippy — use `let` chains (`if let ... && ...`)
- Strict lints on: `must_use_candidate` (add `#[must_use]` to pure pub fns), `cast_possible_truncation` (use `u8::try_from(...)` not `as u8`), `map_unwrap_or` (use `.map_or(default, f)` / `.is_ok_and(f)`)

## Subnet Helpers (#737)
- `wardnetd-services/src/subnet.rs`: shared IPv4 helpers `gateway_for` (net+1), `pool_bounds` (net+10..=bcast-6, None if too small), `canonical_cidr` (host bits cleared). Used by both dhcp `resolve_scope` and zone_enforcement `reconcile_isolation` — don't hand-roll the arithmetic.

## DHCP notes
- `DhcpScope.subnet_prefix: Option<u8>` — Some(prefix) for zone scope, None for base. `assign_lease` skips a static reservation whose IP is outside the zone subnet.
- dhcproto 0.15 exposes `DhcpOption::ClasslessStaticRoute(Vec<(ipnet::Ipv4Net, Ipv4Addr)>)`; `ipnet` is a direct dep of wardnetd. Member-isolation /32 scopes advertise `0.0.0.0/0 -> gateway` (option 121).
- wardnetd/wardnetd-mock are Linux-only (can't build on macOS host); CI is their gate — edit by inspection.
