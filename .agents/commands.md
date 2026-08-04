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
- **`make run-dev`** — mock daemon + all three Vite dev servers (admin-site :7412/admin/, user-app :7413, admin-app :7414/admin-app/). `RESUME=true` persists the DB at `.wardnet-local/wardnet.db`.
- **`make run-dev-daemon`** — run just `wardnetd-mock` on :7411.
- **`make run-dev-web`** — run just the admin-site Vite server on :7412.
- **`make run-dev-admin-app`** — run just the admin-app Vite server on :7414/admin-app/.
- **`make clean`** — clean all build artifacts

## Direct commands (fast iteration only — NOT a substitute for Make before push)

> **DO NOT PUSH WITHOUT RUNNING THE MAKE TARGET FIRST. NO EXCEPTIONS.**
>
> - `cargo clippy -p <crate>` misses `--all-targets` — scoped invocations pass while `make check-daemon` fails.
> - `cargo fmt` without `--check` auto-fixes locally but CI will still reject commits that were pushed without verifying.
> - On macOS, direct `cargo` compiles for macOS. CI runs Linux. USE `make check-daemon`.
> - `yarn build` skips lint. USE `make check-web`.
>
> **ALWAYS RUN `make check-daemon` (Rust) or `make check-web` (web) BEFORE EVERY `git push`.**

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

### SDK (`wardnet.network/go`)

All commands run from `source/sdk/wardnet-go/`.

- **Build / test**: `go build ./...`, `go test ./...`
- **Lint**: `go vet ./...`, `golangci-lint run ./...`
- **Regenerate the REST client**: `go generate ./...` from `internal/rest/` —
  reads `docs/openapi.json`. Commit the result.
- **Drift gate**: `make check-go-openapi` regenerates and fails on any diff.
  Compiling is not enough on its own: the hand-written mapping only breaks when
  the spec *removes or renames* something it uses, so an added field
  regenerates to different code that still builds and the drift goes unnoticed.
- **Versioning**: `VERSION` in this directory drives the published module
  version, independently of the daemon's CalVer. Bump it to cut an SDK release.
  Keep it on `0.x` (or `1.x`) — Go demands a `/vN` path suffix at major ≥ 2.

### CLI (`wctl`)

All commands run from `source/wctl/`.

- **Build / test**: `go build ./...`, `go test ./...`
- **Run**: `go run ./cmd/wctl <subcommand>`
- **Lint**: `go vet ./...`, `golangci-lint run ./...`

Consumes the Go SDK via `replace wardnet.network/go => ../sdk/wardnet-go`, so
local SDK edits apply without publishing. Release builds set the version with
`-ldflags "-X main.version=<version>"`; without it the binary reports `dev` and
`wctl self-update` refuses to run.

### Admin site (desktop)

All commands run from `source/admin-site/web/`. Uses **Yarn 4** (via Corepack).

- **Install**: `yarn install`
- **Dev server**: `yarn dev` (port 7412, proxies `/api` to daemon on 7411)
- **Build**: `yarn build`
- **Type check**: `yarn type-check`
- **Lint**: `yarn lint`
- **Format**: `yarn format` (check: `yarn format:check`)

### Admin mobile PWA

All commands run from `source/admin-app/`. Uses **Yarn 4** (via Corepack).

- **Install**: `yarn install`
- **Dev server**: `yarn dev` (port 7414, base path `/admin-app/`)
- **Build**: `yarn build`
- **Type check**: `yarn type-check`

### Shared React library (`web`)

All commands run from `source/web/`. Uses **Yarn 4** (via Corepack).

- **Install**: `yarn install`
- **Type check**: `yarn type-check`
- **Format**: `yarn format` (check: `yarn format:check`)
