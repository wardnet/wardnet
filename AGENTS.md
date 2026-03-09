# Wardnet

Self-hosted network privacy gateway for Raspberry Pi. See [README.md](README.md) for full overview.

## Agent Memory

Agent memory files live at the **repo root** under `.claude/agent-memory/<agent-type>/MEMORY.md`. When saving or reading agent memory, always use the repo root path, NOT a subdirectory like `source/daemon/`.

## Commands

All builds are driven by the root **Makefile**. Use `make help` to see all targets.

### Makefile targets (preferred)

- **`make init`** — one-time dev setup: installs Rust cross-target, cross-linker, yarn deps
- **`make build`** — build web UI + daemon (host target)
- **`make build-web`** — build web UI only
- **`make build-daemon`** — build daemon for host target
- **`make build-pi`** — cross-compile daemon for Raspberry Pi (`aarch64-unknown-linux-gnu`)
- **`make check`** — run all checks (web + daemon: format, lint, tests)
- **`make check-web`** — web UI typecheck + lint + format check
- **`make check-daemon`** — Rust format + clippy + tests
- **`make deploy PI_HOST=<ip>`** — build for Pi and deploy via SSH
- **`make clean`** — clean all build artifacts

### Direct commands (when needed)

#### Daemon (Rust)

All commands run from `source/daemon/`.

- **Build**: `cargo build`
- **Test**: `cargo test --workspace`
- **Lint**: `cargo clippy --all-targets -- -D warnings`
- **Format**: `cargo fmt` (check: `cargo fmt --check`)
- **Run**: `cargo run -p wardnetd -- --verbose --mock-network`
- **Single crate test**: `cargo test -p wardnetd`, `cargo test -p wardnet-types`

#### Web UI

All commands run from `source/web-ui/`. Uses **Yarn 4** (via Corepack).

- **Install**: `yarn install`
- **Dev server**: `yarn dev` (port 7412, proxies `/api` to daemon on 7411)
- **Build**: `yarn build`
- **Type check**: `yarn type-check`
- **Lint**: `yarn lint`
- **Format**: `yarn format` (check: `yarn format:check`)

## Project Structure

```
source/
├── daemon/                          # Rust workspace (Cargo.toml at this level)
│   └── crates/
│       ├── wardnet-types/           # Shared types: Device, Tunnel, RoutingTarget, Events, API DTOs
│       ├── wardnetd/                # Daemon binary
│       │   ├── migrations/          # SQLite migrations (sqlx)
│       │   └── src/
│       │       ├── main.rs          # Entry point (thin — wires dependencies, starts server)
│       │       ├── lib.rs           # Crate root (re-exports modules for testability)
│       │       ├── config.rs        # TOML config loading with defaults
│       │       ├── db.rs            # SQLite pool init (WAL mode, migrations)
│       │       ├── error.rs         # AppError → axum IntoResponse
│       │       ├── state.rs         # AppState (holds service trait objects + event publisher)
│       │       ├── event.rs         # EventPublisher trait + BroadcastEventBus
│       │       ├── keys.rs          # KeyStore trait + FileKeyStore (private key files)
│       │       ├── wireguard.rs     # WireGuardOps trait + types
│       │       ├── wireguard_real.rs  # Real WireGuard impl (Linux + macOS)
│       │       ├── wireguard_noop.rs  # No-op impl (--mock-network)
│       │       ├── tunnel_monitor.rs  # Background health check + stats collection
│       │       ├── tunnel_idle.rs     # Idle tunnel teardown on DeviceGone
│       │       ├── device_detector.rs   # DeviceDetector: spawns capture + observation loop
│       │       ├── packet_capture.rs    # PacketCapture trait
│       │       ├── packet_capture_pnet.rs  # Real pnet impl
│       │       ├── packet_capture_noop.rs  # No-op impl (--mock-network)
│       │       ├── hostname_resolver.rs    # HostnameResolver trait + SystemHostnameResolver
│       │       ├── hostname_resolver_noop.rs  # No-op impl (--mock-network)
│       │       ├── oui.rs             # MAC OUI prefix lookup (manufacturer database)
│       │       ├── web.rs           # rust-embed static file serving
│       │       ├── repository/      # Data access layer (traits in root, SQLite impls in sqlite/)
│       │       ├── service/         # Business logic layer (traits + impls)
│       │       └── api/             # HTTP handlers (thin, delegate to services)
│       └── wctl/                    # CLI tool (clap)
└── web-ui/                          # React + TypeScript frontend
    └── src/
        ├── api/                     # Typed fetch client
        ├── components/              # Shared components
        ├── pages/                   # Route pages
        └── types/                   # TypeScript API type mirrors
```

## Technical Stack

### Daemon
- Rust 1.94, edition 2024 (pinned in `rust-toolchain.toml`)
- axum 0.8, tokio, tower-http
- SQLite via sqlx 0.8 (runtime queries with `.bind()`, not compile-time macros)
- argon2 for password/API key hashing, SHA-256 for session tokens
- rust-embed to serve web UI from the binary
- async-trait for trait object interfaces

### Web UI
- React 19, TypeScript 5.9, Vite 7
- Tailwind CSS 4 (CSS-first config: `@import "tailwindcss"` + `@tailwindcss/vite` plugin)
- TanStack Query 5, React Router 7, Zustand 5
- ESLint 10 + Prettier
- Yarn 4 with `nodeLinker: node-modules`

## Architecture

### Layered design with dependency injection

```
main.rs  →  builds concrete implementations, injects into AppState
              │
API layer     │  Thin HTTP handlers, extract request, call service, return response
              ↓
Service layer │  Business logic via traits (AuthService, DeviceService, TunnelService, SystemService)
              ↓
Repository    │  Data access via traits (AdminRepository, DeviceRepository, TunnelRepository, etc.)
              ↓
SQLite        │  Parameterized queries only (`.bind()`), never string interpolation
```

- **Traits define ALL boundaries** — every layer depends on trait interfaces, not concrete types. This includes infrastructure: `WireGuardOps`, `KeyStore`, `EventPublisher`
- **`main.rs`** uses `wardnetd::` paths (separate binary crate); all other files use `crate::` paths
- **`AppState`** holds `Arc<dyn Service>` trait objects, no pool exposed to handlers
- **API handlers never touch the database** — they call services, services call repositories

### Auth model
- Unauthenticated self-service for users (auto-detected by source IP via `ConnectInfo<SocketAddr>`)
- Admin login required for privileged operations (session cookie or API key)

### Observability — Tracing spans

Every log entry includes the daemon version via a hierarchical span tree. This is a **hard requirement** for all new components.

**Span hierarchy:**
```
wardnetd{version=0.1.1-dev.5+gabc1234}       # root span in main.rs
  ├── tunnel_monitor{}                         # background task
  ├── idle_watcher{}                           # background task
  ├── device_detector{}                        # background task
  └── api_server{}                             # axum serve
        └── http_request{method=GET, path=/api/devices}  # per-request (tower-http TraceLayer)
```

**Rules for new components:**
1. Every background component's `start()` method accepts a `parent: &tracing::Span` parameter
2. Inside `start()`, create a child span: `let span = tracing::info_span!(parent: parent, "component_name");`
3. Every `tokio::spawn(future)` must be `tokio::spawn(future.instrument(span.clone()))` — spawned tasks do NOT inherit parent spans
4. For inner spawns (e.g. hostname resolution inside device_detector), capture `tracing::Span::current()` and instrument the spawned future
5. `main.rs` captures `root_span = tracing::Span::current()` (which is the `wardnetd{version=...}` span) and passes it to all component `start()` calls

**Versioning:**
- Version is derived from git tags at compile time via `build.rs` → `WARDNET_VERSION` env var
- Shared version-parsing logic lives in `source/daemon/build-support/version.rs` (included by both `wardnetd/build.rs` and `wctl/build.rs` via `include!()`)
- Release: `v0.1.0` tag → `0.1.0`. Dev: N commits after tag → `0.1.1-dev.N+gabc1234`

## Logging Guidelines

When a log line includes structured fields, those key values **must** also appear in the message text. This ensures readability in both structured log aggregators (Loki, Grafana) and plain text output. Simple status messages without meaningful structured data (e.g. `"device detector shut down"`, `"using no-op network backends"`) are fine without structured fields.

**Pattern:**
```rust
// CORRECT — fields in both structured args AND message text (named params)
tracing::info!(mac = %obs.mac, ip = %obs.ip, "device detected: mac={mac}, ip={ip}", mac = obs.mac, ip = obs.ip);
tracing::warn!(error = %e, interface = %iface, "ARP scan failed on {iface}: {e}");
tracing::debug!(count, "flushed last_seen timestamps: count={count}");

// CORRECT — simple status message, no structured fields needed
tracing::info!("device detector shut down");

// WRONG — fields only in structured args (message is opaque in plain text)
tracing::info!(mac = %obs.mac, ip = %obs.ip, "device detected");

// WRONG — fields only in message text (not queryable in structured logs)
tracing::info!("device detected: mac={mac}, ip={ip}", mac = obs.mac, ip = obs.ip);
```

**Rules:**
1. Always use `tracing` macros (`tracing::info!`, `tracing::warn!`, etc.), never `log` or `println!`
2. Structured fields go first: `field = %value` or `field = value` (for Display vs Debug)
3. The message string repeats key values using tracing's `{variable}` interpolation syntax (resolved at the macro level, zero-cost when level is disabled)
4. `error` level — always include the error in the message: `"operation failed on {thing}: {e}"`
5. `warn` level — include enough context to diagnose: what failed, which entity, the error
6. `info` level — include the primary identifiers: MAC, IP, device_id, interface, etc.
7. `debug` level — include counts and operational details: `"flushed {count} timestamps"`
8. `trace` level — rarely used, for packet-level details during development

**Performance:** Tracing macros are zero-cost when the level is filtered out. The level check happens first — if disabled, no arguments are evaluated, no strings are formatted.

## Code Conventions

### Rust
- Doc comments on every public trait, struct, and enum explaining its responsibility
- `#[must_use]` on pure accessor methods (enforced by clippy pedantic)
- **Tests MUST go in separate files** — `src/<layer>/tests/<module>.rs` with `#[cfg(test)] mod tests;` in the layer's `mod.rs`. For crate-level modules, use `src/tests/<module>.rs` with `#[cfg(test)] mod tests;` in `lib.rs`. NEVER put test blocks inline in source files.
- Service tests use mock structs implementing repository/infrastructure traits (manually defined, no mocking libraries)
- Repository tests use in-memory SQLite with migrations applied
- Infrastructure tests (event bus, key store) use dedicated test files under `src/tests/`
- All traits (WireGuardOps, KeyStore, EventPublisher, repositories) have test doubles for unit testing

### Web UI
- Prettier for formatting (configured in `.prettierrc`)
- ESLint with Prettier integration
- React Router 7 imports from `react-router` (not `react-router-dom`)

### Dependencies
- Always add a comment with the crates.io or npmjs URL before each dependency in `Cargo.toml` / `package.json`

## Testing

### Running tests
```bash
# All Rust tests
cd source/daemon && cargo test --workspace

# Web UI checks
cd source/web-ui && yarn type-check && yarn lint && yarn format:check
```

### Test patterns

**Service tests** — mock repositories, test business logic:
```rust
struct MockDeviceRepo { device: Option<Device>, rule: Option<RoutingRule> }

#[async_trait]
impl DeviceRepository for MockDeviceRepo { /* return preconfigured data */ }

#[tokio::test]
async fn set_rule_admin_locked() {
    let svc = DeviceServiceImpl::new(Arc::new(MockDeviceRepo { /* ... */ }));
    let result = svc.set_rule_for_ip("192.168.1.10", RoutingTarget::Direct).await;
    assert!(result.is_err());
}
```

**Repository tests** — real SQLite (in-memory), verify SQL correctness:
```rust
async fn test_pool() -> SqlitePool { /* in-memory pool with migrations */ }

#[tokio::test]
async fn create_and_find_by_username() {
    let pool = test_pool().await;
    let repo = SqliteAdminRepository::new(pool);
    repo.create("id-1", "admin", "hash").await.unwrap();
    let result = repo.find_by_username("admin").await.unwrap();
    assert!(result.is_some());
}
```

## Git Workflow

- **Branch naming**: `feature/<description>`, `fix/<description>`
- **Main branch**: `main`
- **Commit messages**: Conventional commits (`feat:`, `fix:`, `chore:`, `refactor:`)
- Run `cargo fmt && cargo clippy --all-targets` before committing Rust changes
- Run `yarn format && yarn lint` before committing web UI changes

## Boundaries

### Always do
- **Run `make check` (or the relevant `check-daemon` / `check-web` target) and fix ALL issues before pushing to remote.** This includes formatting (`cargo fmt`, `yarn format`), linting (`cargo clippy`, `yarn lint`), type checking (`yarn type-check`), and tests. CI will reject the push if any check fails — fix locally first.
- Use parameterized `.bind()` queries for all SQL — never string-interpolate user input
- Write tests for new functionality
- Follow the layered architecture: handlers → services → repositories
- Keep API handlers thin — business logic belongs in services
- Use existing trait patterns for new features

### Ask first
- Adding new dependencies to `Cargo.toml` or `package.json`
- Modifying database migrations
- Changing public API contracts or response shapes
- Deleting files or removing functionality
- Modifying CI pipeline

### Never do
- Commit secrets, API keys, database files, or `.env`
- Put SQL queries in API handlers (bypass the repository layer)
- Use `unsafe` Rust without explicit approval
- String-interpolate user input into SQL queries
- Skip or delete failing tests
- Run the daemon as root
