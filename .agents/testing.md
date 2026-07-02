# Testing

## Running tests

```bash
# All Rust tests (Linux only — use make check-daemon on macOS)
cd source/daemon && cargo test --workspace

# SDK checks
cd source/sdk/wardnet-js && yarn type-check && yarn format:check

# Frontend unit tests (Vitest) — per package, from source/
cd source && yarn turbo run test --filter=@wardnet/web
cd source && yarn turbo run test:coverage --filter=@wardnet/admin-site

# Frontend checks (type-check + lint + format, where the script exists)
cd source && yarn turbo run type-check lint format:check --filter=@wardnet/admin-site --filter=@wardnet/web

# Or run everything at once (unit tests + lint + format):
# On macOS, daemon checks automatically run inside a Linux container.
make check
```

## Frontend unit tests (Vitest)

The TS frontends — `@wardnet/web` (shared hooks/components/SDK wiring) and
the three apps `@wardnet/admin-site`, `@wardnet/admin-app`, `@wardnet/user-app`
— are tested with **Vitest** (jsdom + Testing Library), mirroring the
long-standing `marketing-site` setup. Each package exposes `test`,
`test:watch`, and `test:coverage` scripts wired into the Turbo pipeline.

Conventions:

- Test files live under `<pkg>/tests/`, mirroring `src/`, named `*.test.ts(x)`.
- Import test globals explicitly (`import { describe, it, expect, vi } from "vitest";`)
  even though `globals: true` is set — it keeps `tsc` happy since `tests/` is
  in the type-check include.
- jest-dom matchers are enabled via `<pkg>/tests/setup.ts`
  (`import "@testing-library/jest-dom"`).
- Mock the SDK singletons (`src/lib/sdk.ts`) with the hoisted pattern
  (`const { deviceService } = vi.hoisted(() => ({ deviceService: { list: vi.fn() } }));`
  then `vi.mock("../../src/lib/sdk", () => ({ deviceService }))`) — a plain
  outer const is uninitialised when the hoisted `vi.mock` factory runs.
- Render inside providers (`QueryClientProvider` + `MemoryRouter`); admin-site
  ships `renderWithProviders` + `makeDevice`/`makeTunnel` in `tests/test-utils.tsx`.
- Segmented MAC/IPv4 inputs render one `<input>` per octet inside a
  `data-testid` container — fill them per-segment.

CI: each build leaf (`build-admin-web`, `build-admin-app`, `build-user-app`)
runs `yarn turbo run test` as a required gate, and `coverage.yml`'s
`frontend-coverage` job runs `test:coverage` and feeds LCOV/JUnit into the
single Codecov upload. Coverage jobs are gated per change bucket so a
frontend-only PR skips daemon/site coverage.

## Test file layout — STRICT RULE

Tests **must** live in separate files. Never put `#[test]` or `#[tokio::test]` blocks inline in source files.

**In a module directory** (e.g., `src/device/`):
- Create `src/device/tests/mod.rs` listing sub-modules: `mod service; mod discovery;`
- Create one file per concern: `src/device/tests/service.rs`, `src/device/tests/discovery.rs`, etc.
- In `src/device/mod.rs`, add: `#[cfg(test)] mod tests;`

**In a crate root** (e.g., `src/lib.rs`):
- Create `src/tests/mod.rs` listing sub-modules
- In `src/lib.rs`, add: `#[cfg(test)] mod tests;`

**Repository tests** follow the same layout under `src/repository/tests/`. The shared `test_pool()` helper lives in `src/repository/tests/mod.rs`.

This rule is enforced by clippy and code review — no exceptions.

## Test patterns

### Service tests — mock repositories, test business logic

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

### Repository tests — real SQLite (in-memory), verify SQL correctness

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

### Infrastructure tests — real impl, temp resources

`FileSecretStore`, `AgeArchiver`, `SqliteDumper` are tested against real filesystem / real pool with tempfile-based isolation. Each test creates a unique directory under `std::env::temp_dir()` and cleans up on completion.

## Web UI end-to-end tests (Playwright)

The Playwright suite for the three web surfaces lives under
`source/end2end-tests/web-ui/` (run with `make e2e-ui`). Its setup,
topology, and the **authoritative selector convention** are documented in
that directory's [`README.md`](../source/end2end-tests/web-ui/README.md).

Selector convention in brief: **`data-testid` is the primary locator**
(located via `page.getByTestId(...)`), with a human-facing label/role/text
assertion added where meaningful. See the README for naming, placement,
and label-assertion rules; the rationale for reversing the earlier
role/label-first approach is in
[`docs/adr-e2e-selector-convention.md`](../docs/adr-e2e-selector-convention.md).
