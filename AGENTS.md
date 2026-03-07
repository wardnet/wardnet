# Wardnet

Self-hosted network privacy gateway for Raspberry Pi. See [README.md](README.md) for full overview.

## Commands

### Daemon (Rust)

All commands run from `source/daemon/`.

- **Build**: `cargo build`
- **Test**: `cargo test --workspace`
- **Lint**: `cargo clippy --all-targets -- -D warnings`
- **Format**: `cargo fmt` (check: `cargo fmt --check`)
- **Run**: `cargo run -p wardnetd -- --verbose`
- **Single crate test**: `cargo test -p wardnetd`, `cargo test -p wardnet-types`

### Web UI

All commands run from `source/web-ui/`. Uses **Yarn 4** (via Corepack).

- **Install**: `yarn install`
- **Dev server**: `yarn dev` (port 7412, proxies `/api` to daemon on 7411)
- **Build**: `yarn build`
- **Type check**: `yarn type-check`
- **Lint**: `yarn lint`
- **Format**: `yarn format` (check: `yarn format:check`)

### Full build (web UI must be built before daemon for rust-embed)

```bash
cd source/web-ui && yarn install && yarn build
cd source/daemon && cargo build --release
```

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
│       │       ├── state.rs         # AppState (holds service trait objects)
│       │       ├── web.rs           # rust-embed static file serving
│       │       ├── repository/      # Data access layer (traits + SQLite impls)
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
Service layer │  Business logic via traits (AuthService, DeviceService, SystemService)
              ↓
Repository    │  Data access via traits (AdminRepository, DeviceRepository, etc.)
              ↓
SQLite        │  Parameterized queries only (`.bind()`), never string interpolation
```

- **Traits define boundaries** — every layer depends on trait interfaces, not concrete types
- **`main.rs`** uses `wardnetd::` paths (separate binary crate); all other files use `crate::` paths
- **`AppState`** holds `Arc<dyn Service>` trait objects, no pool exposed to handlers
- **API handlers never touch the database** — they call services, services call repositories

### Auth model
- Unauthenticated self-service for users (auto-detected by source IP via `ConnectInfo<SocketAddr>`)
- Admin login required for privileged operations (session cookie or API key)

## Code Conventions

### Rust
- Doc comments on every public trait, struct, and enum explaining its responsibility
- `#[must_use]` on pure accessor methods (enforced by clippy pedantic)
- Tests in `src/<layer>/tests/<module>.rs` pattern with `#[cfg(test)] mod tests;` in the layer's `mod.rs`
- Service tests use mock structs implementing repository traits
- Repository tests use in-memory SQLite with migrations applied
- Simple standalone modules (like `config.rs`) may keep inline `#[cfg(test)] mod tests`

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
- Run formatter and linter before suggesting code changes
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
