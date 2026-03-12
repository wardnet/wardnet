# Wardnet Daemon - Rust Engineer Memory

## IMPORTANT: Memory File Location
This memory file lives at the **repo root**: `.claude/agent-memory/rust-engineer/MEMORY.md`
NOT inside `source/daemon/`. Always read and update memory at the repo root, regardless of working directory.

## Repository Module Structure (Refactored)
- **Traits** live in `src/repository/<name>.rs` (e.g. `device.rs`, `tunnel.rs`)
- **SQLite implementations** live in `src/repository/sqlite/<name>.rs`
- `src/repository/mod.rs` re-exports both traits and SQLite structs
- `src/repository/sqlite/mod.rs` re-exports all `Sqlite*Repository` types

## DeviceRepository Trait
- `DeviceRow` (insert struct) lives in `src/repository/device.rs`, re-exported from `repository/mod.rs`
- SQLite impl uses a private `DeviceRow` (sqlx::FromRow) for reading, aliased `InsertDeviceRow` for the public one
- `SELECT_COLS` constant used in SQLite impl to avoid repeating column lists
- Clippy pedantic requires backticks around identifiers like `last_seen` in doc comments

## OUI Module
- `src/oui.rs` with ~200 OUI prefix entries in a `LazyLock<HashMap<[u8; 3], &'static str>>`
- Tests in `src/tests/oui.rs`
- `lookup_manufacturer()` parses MAC "AA:BB:CC" prefix, `guess_device_type()` uses substring matching

## Auth Context in Services (HARD REQUIREMENT)
- Every service method MUST call `auth_context::require_admin()?;` or `auth_context::require_authenticated()?;` as its first line
- Private helper methods (e.g. `load_config`) are exempt -- they're only called from guarded public methods
- Background tasks wrap service calls in `auth_context::with_context(AuthContext::Admin { admin_id: Uuid::nil() }, ...)` to establish admin identity
- Tests wrap service calls in `auth_context::with_context(admin_ctx, svc.method())` to simulate caller identity
- Only exception: startup/restore methods that run before system is ready (e.g. `restore_tunnels`) -- document with comment

## Key Patterns
- `replace_all` on identifiers like `WireGuard` is dangerous -- it replaces in code identifiers too, not just doc comments. Only use targeted edits for doc comment fixes.
- sqlx for SQLite maps INTEGER columns to `i64`. When the domain type is `u16` (e.g. listen_port), use `u16::try_from()` at the DB boundary. For insert, sqlx `.bind()` accepts `Option<u16>` directly.
- `TunnelRow.listen_port` is `Option<u16>` (not `i64`) so the service can pass values from parsed config without casting.
- Clippy requires backticks around `WireGuard` in doc comments (`///`) but not in regular comments (`//`).

## Test Conventions
- Tests go in separate files: `src/repository/tests/<name>.rs`, `src/service/tests/<name>.rs`
- Repository tests use `super::test_pool()` (in-memory SQLite with migrations)
- Service tests use hand-written mock structs implementing repository traits (no mocking library)
- Drop `MutexGuard`s before `.await` points in tests to avoid `clippy::await_holding_lock`
