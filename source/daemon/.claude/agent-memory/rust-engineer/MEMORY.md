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
