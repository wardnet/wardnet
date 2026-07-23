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
- **All shared hooks live in `@wardnet/web`** — do not put TanStack Query hooks that are used by more than one app surface in an app-local `hooks/` folder. Extract them to `source/web/src/hooks/` and re-export from `source/web/src/index.ts`.
- **Mutation hoisting** — when a mutation (e.g. `useRebuildTunnel`) is called from multiple list-item cards on the same page, hoist the single `useMutation` result to the page component and pass `mutate` + `isPending`/`variables` down as props. This keeps the in-flight indicator accurate across the whole list without each card owning an independent mutation.
- **Offline overlay pattern (admin-app)** — `OnlineStatusContext` exposes `showingLastKnownState: boolean`. Pages wrap their content in a `pointer-events-none opacity-40` div when this is true. The loading skeleton should be gated on **all** required queries being loaded (e.g. both `devicesLoading && policyLoading`), not just one, to avoid a premature flash.
- **Component layers** (strict separation):
  - `core/ui/` — shadcn components, no business logic, do not modify directly (re-pull via shadcn CLI)
  - `compound/` — compositions of core components, data via props only, no API calls
  - `features/` — use-case views, data via props + callbacks, no direct API/service calls
  - `layouts/` — page shells, navigation/routing, no business logic
    - **Carve-out — shell-wide status/auth.** The `AppLayout` shells may call
      `useDaemonStatus()` (admin-app, user-app) and `useAuth()` (admin-site)
      directly. These two hooks feed the shell chrome itself — the header
      version/connection pill and the admin-gated navigation — which live above
      every route and have no owning `page` to hoist them into. The carve-out is
      limited to these two read hooks; layouts still perform no mutations and
      wire no other queries.
  - `pages/` — route-level, wire TanStack Query hooks → feature/compound components
- **All business logic in `@wardnet/js`** — components are pure presentation.
- **Hooks** bridge SDK and React: wrap SDK service calls in TanStack Query for caching/loading/error.
- **Dark/light mode**: System preference via `prefers-color-scheme`, toggles `.dark` class on `<html>`.
- **Typography** — render text through the `<Text>` / `<Heading level>` primitive (from `@wardnet/ui`, re-exported by `@wardnet/web`); see `docs/adr/0012-typography-scale-and-roles.md`.
  - Pick a **`variant`** (`label`, `body`, `body-strong`, `caption`, `micro`, `metric`, `metric-unit`, `mono`, `h1`/`h2`/`h3`) — it bakes a size + weight + colour + element bundle. Override individual axes with `size` / `weight` / `color` / `as`. The prop is `variant`, NOT `role`: `role` passes straight through as the native ARIA attribute.
  - Do NOT write raw `text-*` / `font-*` size or weight utilities for new markup — they are tokenized into the variant/`t-size-*`/`t-weight-*` classes. Colour utilities (`text-ink-3`, `text-danger`, …) are still allowed and intentionally override the variant colour.
  - **Exception — display/hero numerals.** A one-off decorative glyph that needs a size above the `size` scale's `4xl` (32px) cap — e.g. the oversized `404` on the not-found page — has no scale token to reach for. Keep it a raw `text-*` size and mark the line with a `ds-typography-allow: <reason>` comment so the intent is legible and the guard (below) skips it. Do not reach for this to dodge a size that *does* exist on the scale.
  - **Enforced in admin-site.** `source/admin-site/tests/typography-conventions.test.ts` scans `src/**/*.tsx` and fails CI if a raw `text-*`/`font-*` size or weight utility reappears. Vendored shadcn primitives under `components/core/ui` are exempt; a justified one-off opts out with a `ds-typography-allow:` comment on or just above the line.
  - **marketing-site** has no `@wardnet/ui` dependency: apply the `@wardnet/styles` helper classes directly (`t-body`, `t-h2`, `t-size-sm`, …) instead of the primitive.
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

- Every endpoint handler carries a `#[utoipa::path(...)]` attribute with `method`, `path`, `tag`, `description`, `request_body`, `responses`.
- Authentication is declared once, document-level: `ApiDoc`'s `security(("session_cookie" = []), ("bearer_auth" = []))` default applies to every operation. Handlers do **not** repeat it — only deliberately unauthenticated endpoints add `security(())` to opt out (a forgotten annotation therefore documents the endpoint as authenticated, the safe direction).
- Route modules expose `pub fn register(router: OpenApiRouter<AppState>) -> OpenApiRouter<AppState>` that attaches routes via `utoipa_axum::routes!`.
- DTOs in `wardnet-common::api` derive `utoipa::ToSchema`.
- `Ipv4Addr` / `IpAddr` fields need a manual `value_type` annotation — utoipa doesn't ship `ToSchema` impls for them. Match the annotation to the field's shape, or the published schema lies about nullability:
  - plain field → `#[schema(value_type = String)]`
  - `Option<...>` field → `#[schema(value_type = Option<String>)]`
  - `Option<...>` **response** field (always serialized, as `null` when unset) → add `required`: `#[schema(required, value_type = Option<String>)]`

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
- Interpolating a `const` column list into an otherwise-fixed query is **still a violation**, even though it is not an injection risk: if every component is constant, the whole statement is constant — hoist it to a `const` instead of paying a `format!()` allocation on every call.

## Dependencies

- Always add a comment with the crates.io URL before each dependency in `Cargo.toml`.
  (`package.json` is standard JSON and cannot carry inline comments — no equivalent
  convention applies to the JS/TS packages.)
