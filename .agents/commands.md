# Commands

All builds are driven by the root **Makefile**. Use `make help` to see all targets.

## Makefile targets (preferred)

- **`make init`** — one-time dev setup: installs yarn deps for SDK, web UI, and site
- **`make build`** — build web UI + daemon (host target)
- **`make build-web`** — build web UI only
- **`make build-daemon`** — build daemon for host target
- **`make check`** — run all checks (SDK + web + site + daemon: format, lint, tests)
- **`make check-sdk`** — SDK typecheck + format check
- **`make check-web`** — web UI typecheck + lint + format check (depends on SDK)
- **`make check-daemon`** — Rust format + clippy + tests. **Linux-only**: the daemon depends on Linux kernel interfaces (netlink, rtnetlink) and cannot compile on macOS. On non-Linux hosts this target auto-detects `podman` or `docker` and runs inside a `rust:1.96` container. Build artefacts are cached in `.target-linux/` (gitignored) and crate downloads in a named volume (`wardnet-cargo-cache`).
- **`make coverage-daemon`** — line-coverage summary via `cargo-llvm-cov`. Same platform auto-detection as `check-daemon` (container on macOS). Uses the same ignore regex as CI.
- **`make run-dev`** — mock daemon + web UI dev server. `RESUME=true` persists the DB at `.wardnet-local/wardnet.db`.
- **`make run-dev-daemon`** / **`make run-dev-web`** — run each piece independently.
- **`make clean`** — clean all build artifacts

## Direct commands (fast iteration only — NOT a substitute for Make before push)

> **IMPORTANT**: Direct `cargo`/`yarn` invocations are acceptable for fast inner-loop
> iteration (e.g. quickly re-running one failing test). They are **NOT** a replacement
> for the Make gate before `git push`. Specifically:
> - `cargo clippy -p <crate>` misses `--all-targets` — lib, test, and binary targets
>   each compile differently; the scoped invocation passes while `make check-daemon` fails.
> - On macOS, any direct `cargo` command builds for macOS, not Linux. CI runs on Linux.
>   Always use `make check-daemon` (which wraps the run in a container) for final verification.
> - Direct `yarn build` is permissive; `make check-web` adds lint and type-check that CI enforces.
>
> **Before every `git push`, always run the appropriate Make target.**

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
