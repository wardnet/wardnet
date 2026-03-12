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
- **`make check`** — run all checks (SDK + web + daemon: format, lint, tests)
- **`make check-sdk`** — SDK typecheck + format check
- **`make check-web`** — web UI typecheck + lint + format check (depends on SDK)
- **`make check-daemon`** — Rust format + clippy + tests
- **`make run-pi PI_HOST=<ip> PI_USER=<user> PI_LAN_IF=<iface>`** — cross-compile, deploy via SSH, run with verbose logging. Cleans database by default; `RESUME=true` keeps existing data. `OTEL=true` enables OpenTelemetry export.
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

#### SDK (`@wardnet/js`)

All commands run from `source/sdk/wardnet-js/`. Uses **Yarn 4** (via Corepack).

- **Install**: `yarn install`
- **Type check**: `yarn type-check`
- **Format**: `yarn format` (check: `yarn format:check`)

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
│       ├── wardnet-types/           # Shared types: Device, Tunnel, RoutingTarget, VPN Provider types, Events, API DTOs
│       ├── wardnetd/                # Daemon binary
│       │   ├── build.rs             # Build script (version, OUI database generation)
│       │   ├── data/oui.csv         # IEEE MA-L OUI database (~39K entries)
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
│       │       ├── oui.rs             # MAC OUI prefix lookup (full IEEE MA-L database, ~39K entries)
│       │       ├── bootstrap.rs      # (Legacy) Admin bootstrap — no longer called from main.rs
│       │       ├── web.rs           # rust-embed static file serving
│       │       ├── vpn_provider.rs  # VpnProvider async trait (validate, list servers, generate config)
│       │       ├── vpn_provider_registry.rs  # VpnProviderRegistry (config-driven, self-registering)
│       │       ├── vpn_provider_nordvpn.rs   # NordVPN impl + NordVpnApi trait (async reqwest)
│       │       ├── repository/      # Data access layer (traits in root, SQLite impls in sqlite/)
│       │       ├── service/         # Business logic layer (traits + impls, includes ProviderService)
│       │       └── api/             # HTTP handlers (thin, delegate to services)
│       └── wctl/                    # CLI tool (clap)
├── sdk/
│   └── wardnet-js/                  # @wardnet/js — TypeScript SDK (browser + Node)
│       └── src/
│           ├── client.ts            # WardnetClient base HTTP client
│           ├── services/            # AuthService, DeviceService, TunnelService, SystemService, SetupService, InfoService
│           └── types/               # TypeScript type definitions (mirrors daemon API)
└── web-ui/                          # React + TypeScript frontend
    └── src/
        ├── components/
        │   ├── core/ui/             # shadcn/ui components (Button, Card, Sheet, etc.)
        │   ├── compound/            # Compositions (Sidebar, MobileMenu, PageHeader, DeviceIcon, ConnectionStatus, Logo)
        │   ├── features/            # Use-case components (DeviceList, TunnelList, LoginForm)
        │   └── layouts/             # Page shells (AppLayout, AuthLayout)
        ├── hooks/                   # React hooks bridging SDK ↔ React (useAuth, useTheme, useDevices, useTunnels, useSystemStatus, useDaemonStatus, useSetup)
        ├── stores/                  # Zustand stores (authStore)
        ├── pages/                   # Route pages (Dashboard, Devices, Tunnels, Settings, Login, Setup, MyDevice)
        └── lib/                     # SDK instance (sdk.ts), utilities (cn, formatBytes, formatUptime, timeAgo)
```

## Technical Stack

### Daemon
- Rust 1.94, edition 2024 (pinned in `rust-toolchain.toml`)
- axum 0.8, tokio, tower-http
- SQLite via sqlx 0.8 (runtime queries with `.bind()`, not compile-time macros)
- argon2 for password/API key hashing (Argon2id), SHA-256 for session tokens
- sysinfo for host CPU/memory monitoring
- rust-embed to serve web UI from the binary
- async-trait for trait object interfaces

### SDK (`@wardnet/js`)
- TypeScript 5.9, zero runtime dependencies
- Uses native `fetch` (works in browser and Node 18+)
- No DOM types — minimal `fetch.d.ts` for cross-environment support
- Linked to web-ui via Yarn `portal:` protocol (`"@wardnet/js": "portal:../sdk/wardnet-js"`)
- Yarn 4 with `nodeLinker: node-modules`

### Web UI
- React 19, TypeScript 5.9, Vite 7
- Tailwind CSS 4 (CSS-first config: `@import "tailwindcss"` + `@tailwindcss/vite` plugin)
- shadcn/ui (Radix UI primitives + Tailwind styling) — components in `src/components/core/ui/`
- TanStack Query 5, React Router 7, Zustand 5
- ESLint 10 + Prettier
- Yarn 4 with `nodeLinker: node-modules`
- Path alias: `@/` → `src/` (Vite + tsconfig)

## Architecture

### Layered design with dependency injection

```
main.rs  →  builds concrete implementations, injects into AppState
              │
API layer     │  Thin HTTP handlers, extract request, call service, return response
              ↓
Service layer │  Business logic via traits (AuthService, DeviceService, TunnelService, SystemService, ProviderService)
              ↓
Repository    │  Data access via traits (AdminRepository, DeviceRepository, TunnelRepository, etc.)
              ↓
SQLite        │  Parameterized queries only (`.bind()`), never string interpolation
```

- **Traits define ALL boundaries** — every layer depends on trait interfaces, not concrete types. This includes infrastructure: `WireGuardOps`, `KeyStore`, `EventPublisher`, `NordVpnApi` (provider-specific HTTP abstraction)
- **`main.rs`** uses `wardnetd::` paths (separate binary crate); all other files use `crate::` paths
- **`AppState`** holds `Arc<dyn Service>` trait objects, no pool exposed to handlers
- **API handlers never touch the database** — they call services, services call repositories

### Auth model
- **Setup wizard**: On first run, no admin exists. `GET /api/setup/status` returns `setup_completed: false`. Web UI redirects to setup page. `POST /api/setup` creates the first admin (Argon2id hashed). Returns 409 if already completed.
- **Unauthenticated endpoints**: `GET /api/info` (version + uptime), `GET /api/setup/status`, `POST /api/setup`, `GET /api/devices/me`, `PUT /api/devices/me/rule`
- Unauthenticated self-service for users (auto-detected by source IP via `ConnectInfo<SocketAddr>`)
- Admin login required for privileged operations (session cookie or API key)

### Authentication context in services (HARD REQUIREMENT)

Every service method **must** validate the authentication context as its first operation using `auth_context::require_admin()?;` or `auth_context::require_authenticated()?;`. Services never trust their caller -- they always check. This is a defense-in-depth measure: even if a handler bug exposes a service method, the auth guard inside the service rejects unauthorized calls.

**HTTP request path (automatic):** The `AuthContextLayer` middleware resolves the caller identity (from session cookie or API key) and sets a task-local `AuthContext` before the request reaches handlers. Service methods read it via `auth_context::require_admin()`.

**Background tasks calling services:** Background processes (e.g. `IdleTunnelWatcher` tearing down idle tunnels, DHCP lease expiry) run outside the HTTP middleware, so no `AuthContext` is set by default. They **must** wrap service calls in `auth_context::with_context()` to establish an admin identity:

```rust
use wardnet_types::auth::AuthContext;

// Background task calling a service method:
let admin_ctx = AuthContext::Admin { admin_id: Uuid::nil() };
auth_context::with_context(admin_ctx, tunnel_service.tear_down(id, "idle timeout")).await?;
```

Use `Uuid::nil()` (all zeros) as the `admin_id` for system-initiated operations -- this clearly distinguishes background/system actions from real admin sessions in audit logs.

**Tests:** Use the same `auth_context::with_context()` pattern to set the auth context before calling service methods:

```rust
let admin_ctx = AuthContext::Admin { admin_id: Uuid::new_v4() };
let result = auth_context::with_context(admin_ctx, svc.get_config()).await;
```

**Rules:**
1. Every service trait method implementation must call `auth_context::require_admin()?;` or `auth_context::require_authenticated()?;` as its first line
2. The only exception is startup/restore methods that run before the system is ready (e.g. `restore_tunnels`) -- these should be documented with a comment explaining why the guard is skipped
3. Background tasks wrap service calls in `auth_context::with_context(AuthContext::Admin { admin_id: Uuid::nil() }, ...)`
4. Tests wrap service calls in `auth_context::with_context(admin_ctx, ...)` to simulate the caller identity
5. Anonymous callers get `Err(AppError::Forbidden)` -- never silently succeed

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

**OUI Database:**
- Full IEEE MA-L database (~39K entries) in `crates/wardnetd/data/oui.csv`
- Parsed at build time by `crates/wardnetd/build.rs` → generates `oui_data.rs` in `OUT_DIR`
- Locally administered MACs (bit 1 of first byte set) detected as "Randomized MAC" (typically phones using MAC randomization)
- `cargo::rerun-if-changed=data/oui.csv` — only regenerates when CSV changes

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

### SDK (`@wardnet/js`)
- Pure TypeScript — no React, no DOM dependencies
- Service classes (AuthService, DeviceService, etc.) accept a `WardnetClient` instance
- Types mirror daemon API DTOs — keep in sync when API changes

### Web UI
- Prettier for formatting (configured in `.prettierrc`)
- ESLint with Prettier integration
- React Router 7 imports from `react-router` (not `react-router-dom`)
- **Component layers** (strict separation):
  - `core/ui/` — shadcn components, no business logic, do not modify directly (re-pull via shadcn CLI)
  - `compound/` — compositions of core components, data via props only, no API calls
  - `features/` — use-case views, data via props + callbacks, no direct API/service calls
  - `layouts/` — page shells, navigation/routing, no business logic
  - `pages/` — route-level, wire TanStack Query hooks → feature/compound components
- **All business logic in `@wardnet/js`** — components are pure presentation
- **Hooks** bridge SDK and React: wrap SDK service calls in TanStack Query for caching/loading/error
- **Dark/light mode**: System preference via `prefers-color-scheme`, toggles `.dark` class on `<html>`

### Dependencies
- Always add a comment with the crates.io or npmjs URL before each dependency in `Cargo.toml` / `package.json`

## Testing

### Running tests
```bash
# All Rust tests
cd source/daemon && cargo test --workspace

# SDK checks
cd source/sdk/wardnet-js && yarn type-check && yarn format:check

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

## Pre-push checklist (MANDATORY)

**You MUST run checks locally and fix ALL issues BEFORE every `git push`.** CI will reject the push otherwise. This is a hard gate — never push without passing checks.

```bash
# For Rust changes:
cd source/daemon && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test --workspace

# For web UI changes:
cd source/web-ui && yarn format && yarn lint && yarn type-check

# Or run everything at once:
make check
```

**Code coverage (MANDATORY for Rust changes):**
We use `cargo-llvm-cov` for code coverage. Before starting work, compute the current coverage baseline on `main` (or during planning). After implementation, run it again on your branch and verify coverage **does not decrease**. New code must have tests — coverage should stay the same or increase. It must never go down.

```sh
cd source/daemon
cargo llvm-cov --package wardnetd --summary-only \
  --ignore-filename-regex '(main\.rs|noop_.*\.rs|db\.rs|web\.rs|api/mod\.rs|auth_context\.rs)'
```

The `--ignore-filename-regex` excludes files that are not unit-testable (binary entrypoint, no-op/stub implementations prefixed with `noop_`, database pool setup, static file serving, Tower middleware boilerplate, and auth context thread-locals). CI uses the same exclusions — see `.github/workflows/ci.yml`.

## Boundaries

### Always do
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
