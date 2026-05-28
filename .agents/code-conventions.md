# Code Conventions

## Rust

- Doc comments on every public trait, struct, and enum explaining its responsibility.
- `#[must_use]` on pure accessor methods (enforced by clippy pedantic).
- **Tests MUST go in separate files** — `src/<layer>/tests/<module>.rs` with `#[cfg(test)] mod tests;` in the layer's `mod.rs`. For crate-level modules, use `src/tests/<module>.rs` with `#[cfg(test)] mod tests;` in `lib.rs`. NEVER put test blocks inline in source files.
- Service tests use mock structs implementing repository/infrastructure traits (manually defined, no mocking libraries).
- Repository tests use in-memory SQLite with migrations applied.
- Infrastructure tests (event bus, secret store) use dedicated test files under `src/tests/`.
- All traits (`TunnelInterface`, `SecretStore`, `EventPublisher`, `FirewallManager`, `PolicyRouter`, `CommandExecutor`, `PacketCapture`, `DhcpSocket`, `DatabaseDumper`, `BackupArchiver`, repositories) have test doubles for unit testing.

## SDK (`@wardnet/js`)

- Pure TypeScript — no React, no DOM dependencies.
- Service classes (`AuthService`, `DeviceService`, etc.) accept a `WardnetClient` instance.
- Types mirror daemon API DTOs — keep in sync when API changes.

## Web UI

- Prettier for formatting (configured in `.prettierrc`).
- ESLint with Prettier integration.
- React Router 7 imports from `react-router` (not `react-router-dom`).
- **Component layers** (strict separation):
  - `core/ui/` — shadcn components, no business logic, do not modify directly (re-pull via shadcn CLI)
  - `compound/` — compositions of core components, data via props only, no API calls
  - `features/` — use-case views, data via props + callbacks, no direct API/service calls
  - `layouts/` — page shells, navigation/routing, no business logic
  - `pages/` — route-level, wire TanStack Query hooks → feature/compound components
- **All business logic in `@wardnet/js`** — components are pure presentation.
- **Hooks** bridge SDK and React: wrap SDK service calls in TanStack Query for caching/loading/error.
- **Dark/light mode**: System preference via `prefers-color-scheme`, toggles `.dark` class on `<html>`.
- **Detail views are routed pages, not sheets.**
  - One detail concept per resource: `/<resource>/:id` (e.g. `/tunnels/:id`,
    `/devices/:id`). The list page links into the detail page; sheets are
    reserved for forms (create/edit), not for read-mostly detail screens.
  - Standard chrome on every detail page: page title plus a small
    breadcrumb directly below it (`<Resource list> / <item label>`).
    Build it with the shared `<DetailPageHeader>` compound component so
    breadcrumb + title + status pill + trailing meta stay consistent
    across resources.

## OpenAPI annotations (daemon)

- Every endpoint handler carries a `#[utoipa::path(...)]` attribute with `method`, `path`, `tag`, `description`, `request_body`, `responses`, `security`.
- Route modules expose `pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState>` that attaches routes via `utoipa_axum::routes!`.
- DTOs in `wardnet-common::api` derive `utoipa::ToSchema`.
- `#[schema(value_type = String)]` required for `Ipv4Addr` / `IpAddr` fields — utoipa 5.4 doesn't ship `ToSchema` impls for them.

## SQL query strings

- **Use `const` query strings** for fixed SQL statements — avoids a heap allocation per call that `format!()` would incur.
  ```rust
  // ✓ — allocated once at program start
  const FIND_BY_ID: &str = "SELECT … FROM installs WHERE id = ?";

  // ✗ — heap allocates on every call
  let q = format!("SELECT {SELECT_COLS} FROM installs WHERE id = ?");
  ```
- **Parameterised queries only** (`.bind()`) — never string-interpolate user input into SQL.
- `format!()` is acceptable for `PRAGMA` statements with **numeric constants** (e.g. `PRAGMA incremental_vacuum({N})`), not for WHERE clauses or column lists.

## Dependencies

- Always add a comment with the crates.io or npmjs URL before each dependency in `Cargo.toml` / `package.json`.
