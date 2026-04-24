# Wardnet — Development guide

This guide covers everything you need to build, run, and contribute to Wardnet. For the project overview and user-facing install instructions, see the [README](../README.md).

## Project status

Wardnet is in active development (Phase 1 MVP). It is being daily-driven on a Raspberry Pi at home but is not yet a finished product — expect to read the source occasionally if you hit rough edges. Milestone progress lives in [`implementation-docs/plans/phase-1-mvp-implementation-plan.md`](../implementation-docs/plans/phase-1-mvp-implementation-plan.md); known bugs and feature ideas in [`implementation-docs/follow-ups.md`](../implementation-docs/follow-ups.md).

### What works today

- WireGuard tunnel CRUD with NordVPN provider integration
- Per-device routing policy (direct / specific tunnel / network default), applied via `ip rule` + nftables
- Built-in DHCP server with leases, static reservations, and conflict detection
- Built-in DNS server with network-wide ad blocking, allowlist, and custom rules
- Device detection (ARP, OUI lookup, departure tracking)
- DNS leak prevention on tunnel-routed devices (port 53 DNAT to the tunnel's DNS)
- Web UI with setup wizard, dashboard, devices, tunnels, DHCP, DNS, ad blocking
- REST + WebSocket API with session + API-key auth
- `wctl` CLI (scaffolded)
- OpenTelemetry trace/log/metric export and Pyroscope continuous profiling (opt-in)
- Signed releases via tag-driven CI, published on GitHub Releases and mirrored at [`wardnet.network/releases/`](https://wardnet.network)

### What's not done yet

- Daemon-side auto-update (pipeline is signed and published; the update runner inside `wardnetd` is next)
- Gateway resilience (keepalived failover, hardware watchdog, graceful shutdown)
- Mobile app, kill switch per device, scheduled routing

### Known caveats when in use

- After switching a device *off* a tunnel (VPN → direct, or between tunnels), the device's existing TCP sockets may stay stuck for ~30–60s while their stack times out. Routing on the Pi is correct immediately; toggling Wi-Fi on the device fixes it instantly. See [#77](https://github.com/wardnet/wardnet/issues/77).
- A NordVPN server selected at tunnel-creation time may be unhealthy when you actually try to use it; the daemon currently still marks such tunnels as "up" even when no WireGuard handshake has ever completed. See [#79](https://github.com/wardnet/wardnet/issues/79) and [#80](https://github.com/wardnet/wardnet/issues/80).

## Architecture

```
source/
├── daemon/                 # Rust workspace
│   └── crates/
│       ├── wardnet-common/    # Shared types (devices, tunnels, routing, DHCP, DNS, update, events, API DTOs, config)
│       ├── wardnetd-data/     # Data access layer (repository traits + SQLite implementations, migrations, keystore)
│       ├── wardnetd-services/ # Business logic layer (auth, device, tunnel, DHCP, DNS, routing, system, logging, VPN)
│       ├── wardnetd-api/      # HTTP API layer (axum handlers, middleware, state)
│       ├── wardnetd/          # Daemon binary (Linux-specific backends + startup)
│       ├── wardnetd-mock/     # Local dev binary (full API, no-op network backends, in-memory SQLite)
│       └── wctl/              # CLI tool
├── sdk/
│   └── wardnet-js/        # @wardnet/js — TypeScript SDK (browser + Node)
├── web-ui/                # React 19 + TypeScript frontend (embedded into daemon via rust-embed)
├── site/                  # Public marketing site (wardnet.network) + release manifests
└── system-tests/          # TypeScript E2E tests against a real Pi
```

### Daemon (`wardnetd`)

Layered architecture with dependency injection via traits. Every boundary is a trait, including infrastructure (`TunnelInterface`, `KeyStore`, `EventPublisher`, `FirewallManager`, `PolicyRouter`, `CommandExecutor`, `PacketCapture`, `DhcpSocket`, `NordVpnApi`, etc.) so unit tests can stub any dependency.

```
wardnetd (main.rs)   →  wires real Linux backends, calls init_services(), starts axum server
                              │
wardnetd-api         │  AppState + Axum router: thin handlers, extract request, call service
                              ↓
wardnetd-services    │  Service structs implementing traits (AuthService, DeviceService, ...)
                              ↓
wardnetd-data        │  RepositoryFactory + repository traits (AdminRepository, DeviceRepository, ...)
                              ↓
SQLite               │  Parameterised queries only (`.bind()`), never string interpolation

wardnet-common       ─  Shared types, config, events — referenced by all crates above
wardnetd-mock        ─  Dev binary: same wardnetd-api/services/data stack, no-op Linux backends
```

- **Handlers never touch the database** — they call services; services call repositories.
- **Every service method enforces authentication** as its first operation (`auth_context::require_admin()?` or `require_authenticated()?`).
- **Background tasks** (idle tunnel teardown, DHCP cleanup, DNS runner, update checker) wrap service calls in `auth_context::with_context(...)` to establish a system identity (`admin_id: Uuid::nil()`).

### Web UI

React 19 + Vite 8 + Tailwind CSS 4 + TanStack Query 5 + React Router 7. Component layering:

- `core/ui/` — shadcn/ui primitives; never modify directly, re-pull via shadcn CLI.
- `compound/` — compositions of core components; props only, no API calls.
- `features/` — use-case views; data via props + callbacks, no direct service calls.
- `layouts/` — page shells with navigation.
- `pages/` — route-level; wire TanStack Query hooks to feature components.

All business logic lives in `@wardnet/js`. Components are pure presentation. Hooks in `src/hooks/` bridge the SDK and React.

## Tech stack

| Component       | Technology                                                    |
|-----------------|---------------------------------------------------------------|
| Daemon          | Rust 1.95, axum 0.8, SQLite (sqlx 0.8)                        |
| Web UI          | React 19, TypeScript 5.9, Vite 8, Tailwind CSS 4              |
| SDK             | TypeScript 5.9, zero runtime dependencies, native `fetch`     |
| Package manager | Yarn 4 (via Corepack)                                         |
| Auth            | argon2id (passwords / API keys), SHA-256 (session tokens)     |
| Tunnels         | WireGuard (Linux kernel module)                               |
| DNS             | hickory-dns                                                   |
| DHCP            | dhcproto                                                      |
| Observability   | OpenTelemetry (traces/logs/metrics), Pyroscope (profiling)    |
| Targets         | `aarch64-unknown-linux-gnu`, `x86_64-unknown-linux-gnu`       |

## Getting started

### Prerequisites

- Rust 1.95+ (pinned via `rust-toolchain.toml`)
- Node.js 25+
- Yarn 4 (enabled via `corepack enable`)
- **Daemon checks on macOS**: Podman or Docker. The daemon uses Linux-only kernel interfaces (netlink, rtnetlink) and cannot compile natively on macOS — `make check-daemon` runs checks inside a Linux container automatically.

### First-time setup

```sh
make init
```

Installs the Rust cross-compilation target, the aarch64-linux-gnu linker, and yarn dependencies for the SDK, web UI, and marketing site.

### Build

```sh
make build           # web UI + daemon (host target)
make build-web       # web UI only
make build-daemon    # daemon only (host target)
make build-pi        # cross-compile daemon for Raspberry Pi (aarch64-unknown-linux-gnu)
make image           # build the production container image (downloads latest release)
make image-multiarch # build linux/amd64 + linux/arm64 production images via buildx
```

### Run locally (no Pi hardware)

```sh
make run-dev       # runs wardnetd-mock + web UI dev server, browser opens to localhost:7412
```

The mock daemon serves the full API with no-op network backends and an in-memory SQLite database seeded with demo devices and tunnels. Useful for UI work without touching real infrastructure.

### Deploy to a Raspberry Pi

```sh
# Dev deploy: cross-compile, scp, run with verbose logging
make run-pi PI_HOST=<ip> PI_USER=<user> PI_LAN_IF=<iface>

# Production deploy: runs install.sh + copies binary to /usr/local/bin
make deploy-prod PI_HOST=<ip> PI_USER=<user> PI_LAN_IF=<iface>
```

`run-pi` cleans the database by default; pass `RESUME=true` to keep existing data. `OTEL=true` enables OpenTelemetry export.

### Checks

```sh
make check          # all (SDK + web + site + daemon — format, lint, type-check, tests)
make check-web      # web UI typecheck + lint + format
make check-site     # public site typecheck + format + tests
make check-daemon   # daemon format + clippy + tests (auto: native on Linux, container on macOS)
make check-sdk      # SDK typecheck + format
make coverage-daemon  # line-coverage summary (same container auto-detection)
```

Run `make check` locally before every `git push`. CI runs the same targets.

### Direct commands

```sh
cd source/daemon && cargo build                         # daemon build (Linux only)
cd source/daemon && cargo test --workspace              # daemon tests
cd source/daemon && cargo clippy --all-targets -- -D warnings
cd source/web-ui && yarn dev                            # web UI dev server (port 7412)
cd source/web-ui && yarn build                          # web UI production build
cd source/sdk/wardnet-js && yarn type-check             # SDK check
cd source/site && yarn dev                              # marketing site
```

### `wctl` — CLI

```sh
cd source/daemon && cargo run -p wctl -- status
```

### Version management

Project version lives in the [`VERSION`](../VERSION) file at the repo root. Bumping it requires propagating the change into the daemon's Cargo workspace and all three `package.json` files:

```sh
# Edit VERSION, then:
make sync-version
cargo check   # refresh daemon Cargo.lock
(cd source/sdk/wardnet-js && yarn install)
(cd source/web-ui && yarn install)
(cd source/site && yarn install)

# `make check-version` is the CI gate — run it locally to verify everything agrees.
make check-version
```

The `WARDNET_VERSION` env var seen by the daemon at compile time is derived from `git describe --tags --always --dirty` in [`source/daemon/build-support/version.rs`](../source/daemon/build-support/version.rs). On a clean `v0.2.0` tag this produces `0.2.0`; mid-development it produces `0.2.1-dev.5+gabc1234`.

## Cutting a release

1. Bump `VERSION`, run `make sync-version`, commit.
2. Write release notes in `docs/releases/vX.Y.Z.md` (use a previous release as a template).
3. Annotated tag: `git tag -a vX.Y.Z -m "..."`.
4. Push: `git push && git push --tags`.
5. The [Release workflow](../.github/workflows/release.yml) builds the daemon for every matrix target, signs each tarball with minisign, and publishes a GitHub Release with the `tarball + .sha256 + .minisig` for each target. It also attaches the committed `docs/openapi.json` and its `.sha256` as release assets so external consumers can download the OpenAPI spec for a specific daemon version.
6. The [Release workflow](../.github/workflows/release.yml) also calls [`build-site.yml`](../.github/workflows/build-site.yml), which regenerates `public/releases/stable.json`, `public/releases/beta.json`, and `public/releases/openapi-versions.json` from the GitHub API, then hands the bundle to [`deploy-site.yml`](../.github/workflows/deploy-site.yml) for publication to GitHub Pages. The daemon's auto-update runner (v0.3.0+) reads the stable/beta manifests; the site's docs page uses `openapi-versions.json` to list distinct published specs (deduped by content hash).

SemVer pre-release tags (`vX.Y.Z-beta.N`, `vX.Y.Z-rc.N`) are automatically flagged as GitHub pre-releases and feed the `beta` channel. Clean `vX.Y.Z` tags feed `stable`.

For signing-key setup and rotation, see [`deploy/keys/README.md`](../deploy/keys/README.md).

## Conventions

### Rust

- **Tests live in separate files** — `src/<layer>/tests/<module>.rs` with `#[cfg(test)] mod tests;` in the layer's `mod.rs`. Never inline `#[cfg(test)]` blocks in source files.
- **Traits everywhere** — every layer boundary is a trait. Service tests use hand-written mock structs (no mocking libraries).
- **SQL** — parameterised queries only (`.bind()`), never string interpolation.
- **Errors** — `AppError` in `wardnetd-services` maps cleanly to HTTP. Use `thiserror` for error types.
- **Logging** — `tracing` macros only. When you include a structured field, also include its value in the message text so plain-text readers get the same information. See [CLAUDE.md](../CLAUDE.md#logging-guidelines) for the rule.
- **Doc comments** on every public trait, struct, and enum.
- **`#[must_use]`** on pure accessor methods (enforced by clippy pedantic).

### Web UI / SDK

- Prettier + ESLint enforced in CI.
- React Router 7 imports from `react-router`, not `react-router-dom`.
- Strict component layering — see [Architecture](#architecture) above.

### Git

- Branch naming: `feature/<description>`, `fix/<description>`, `chore/<description>`.
- Conventional commits (`feat:`, `fix:`, `chore:`, `refactor:`, `docs:`, `test:`).
- Run `make check` before every push.
- `make check-version` must pass — CI gate.

## Continuous integration

CI is split into thin orchestrators (one per event type) that compose
reusable `workflow_call` leaves. Every orchestrator starts with a
`preflight` job that runs the [detect-changes](../.github/actions/detect-changes/action.yml)
and [check-version](../.github/actions/check-version/action.yml)
composite actions; its outputs gate the heavy leaves.

[`.github/workflows/pr.yml`](../.github/workflows/pr.yml) runs on every PR to `main`:

1. `preflight` — detect-changes + check-version.
2. `build-daemon` — [reusable leaf](../.github/workflows/build-daemon.yml). Lints, runs `cargo test --workspace`, verifies OpenAPI drift, builds the embedded web UI, cross-compiles `wardnetd` (x86_64 + aarch64) and `wardnetd-mock` (x86_64), uploads tarballs as artifacts.
3. `build-site` — [reusable leaf](../.github/workflows/build-site.yml). Lints + type-checks, builds the marketing site, uploads `site-dist`.
4. `coverage` — [reusable leaf](../.github/workflows/coverage.yml). Generates daemon + site coverage in parallel and performs a single coordinated Codecov upload with the `daemon` / `site` flags.
5. `tests-e2e` — [reusable leaf](../.github/workflows/tests-e2e.yml). Stub today; consumes daemon + site artifacts.

[`.github/workflows/ci.yml`](../.github/workflows/ci.yml) runs on pushes to `main` and reuses the same `build-daemon` + `build-site` leaves but skips `coverage` and `tests-e2e` (Codecov patch-coverage is PR-keyed; e2e is a PR gate).

[`.github/workflows/release.yml`](../.github/workflows/release.yml) runs on `v*.*.*` tag pushes: `resolve` → the same build leaves → `tests-e2e` → [`deploy-site.yml`](../.github/workflows/deploy-site.yml) (publishes the `site-dist` bundle to GitHub Pages) → [`release-daemon.yml`](../.github/workflows/release-daemon.yml) (renames tarballs with the version, signs each with minisign, publishes the GitHub Release).

[`.github/workflows/deploy-site.yml`](../.github/workflows/deploy-site.yml) is a reusable `workflow_call` leaf: it downloads a pre-built `site-dist` artifact and publishes it to GitHub Pages. It is invoked from `release.yml` only.

Fuzzing is handled by two separate workflows driven by cron and the [ClusterFuzzLite](../.clusterfuzzlite/README.md) setup:

- [`.github/workflows/fuzzing-scheduled.yml`](../.github/workflows/fuzzing-scheduled.yml) runs every 12 hours in batch mode against the fuzz targets under `source/daemon/fuzz/`, files GitHub Issues on new crashes.
- [`.github/workflows/fuzzing-maintenance.yml`](../.github/workflows/fuzzing-maintenance.yml) runs weekly for coverage reports + corpus pruning.

[`.github/workflows/security.yml`](../.github/workflows/security.yml) runs `cargo audit` nightly and on dependency changes.

[`.github/workflows/codeql.yml`](../.github/workflows/codeql.yml) runs CodeQL analysis weekly + on PRs.

[`.github/workflows/scorecard.yml`](../.github/workflows/scorecard.yml) runs OpenSSF Scorecard weekly.

Shared setup steps live in composite actions under [`.github/actions/`](../.github/actions/).

## Agent collaboration

AI coding agents (Claude Code, Codex, Copilot) have detailed instructions in:

- [`CLAUDE.md`](../CLAUDE.md) — Claude-specific project instructions
- [`AGENTS.md`](../AGENTS.md) — universal conventions all agents follow

Keep both up to date when the project structure or conventions change.

## License

[MIT](../LICENSE)
