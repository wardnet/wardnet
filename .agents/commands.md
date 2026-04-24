# Commands

All builds are driven by the root **Makefile**. Use `make help` to see all targets.

## Makefile targets (preferred)

- **`make init`** — one-time dev setup: installs Rust cross-target, cross-linker, yarn deps
- **`make build`** — build web UI + daemon (host target)
- **`make build-web`** — build web UI only
- **`make build-daemon`** — build daemon for host target
- **`make build-pi`** — cross-compile daemon for Raspberry Pi (`aarch64-unknown-linux-gnu`)
- **`make check`** — run all checks (SDK + web + site + daemon: format, lint, tests)
- **`make check-sdk`** — SDK typecheck + format check
- **`make check-web`** — web UI typecheck + lint + format check (depends on SDK)
- **`make check-daemon`** — Rust format + clippy + tests. **Linux-only**: the daemon depends on Linux kernel interfaces (netlink, rtnetlink) and cannot compile on macOS. On non-Linux hosts this target auto-detects `podman` or `docker` and runs inside a `rust:1.95` container. Build artefacts are cached in `.target-linux/` (gitignored) and crate downloads in a named volume (`wardnet-cargo-cache`).
- **`make coverage-daemon`** — line-coverage summary via `cargo-llvm-cov`. Same platform auto-detection as `check-daemon` (container on macOS). Uses the same ignore regex as CI.
- **`make run-dev`** — mock daemon + web UI dev server. `RESUME=true` persists the DB at `.wardnet-local/wardnet.db`.
- **`make run-dev-daemon`** / **`make run-dev-web`** — run each piece independently.
- **`make run-pi PI_HOST=<ip> PI_USER=<user> PI_LAN_IF=<iface>`** — cross-compile, deploy via SSH, run with verbose logging. Cleans database by default; `RESUME=true` keeps existing data. `OTEL=true` enables OpenTelemetry export.
- **`make system-test`** — full E2E: build, deploy daemon + test-agent to Pi, run system tests, teardown
- **`make system-test-setup`** — deploy and start test infrastructure on Pi (leave running)
- **`make system-test-teardown`** — stop test environment on Pi
- **`make clean`** — clean all build artifacts

## Direct commands (when needed)

### Daemon (Rust)

All commands run from `source/daemon/`. **Linux only** — on macOS use `make check-daemon` which runs them inside a container.

- **Build**: `cargo build`
- **Test**: `cargo test --workspace`
- **Lint**: `cargo clippy --all-targets -- -D warnings`
- **Format**: `cargo fmt` (check: `cargo fmt --check`)
- **Single crate test**: `cargo test -p wardnetd`, `cargo test -p wardnet-common`, `cargo test -p wardnetd-services`

### SDK (`@wardnet/js`)

All commands run from `source/sdk/wardnet-js/`. Uses **Yarn 4** (via Corepack).

- **Install**: `yarn install`
- **Type check**: `yarn type-check`
- **Format**: `yarn format` (check: `yarn format:check`)

### Web UI

All commands run from `source/web-ui/`. Uses **Yarn 4** (via Corepack).

- **Install**: `yarn install`
- **Dev server**: `yarn dev` (port 7412, proxies `/api` to daemon on 7411)
- **Build**: `yarn build`
- **Type check**: `yarn type-check`
- **Lint**: `yarn lint`
- **Format**: `yarn format` (check: `yarn format:check`)
