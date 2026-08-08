# Workflow

## Git

- **Branch naming**: `feature/<description>`, `fix/<description>`.
- **Main branch**: `main`.
- **Commit messages**: Conventional commits (`feat:`, `fix:`, `chore:`, `refactor:`, `docs:`).
- **Do NOT add `Co-Authored-By: Claude ...` trailers (or any AI-agent attribution) to commit messages.** Commits land under the human author only; GitHub parses `Co-Authored-By` trailers and inflates the repo contributor graph with bot accounts.
- Run `cargo fmt && cargo clippy --all-targets` before committing Rust changes.
- Run `yarn format && yarn lint` before committing web UI changes.

## Pre-push checklist (MANDATORY)

**You MUST run checks locally and fix ALL issues BEFORE every `git push`.** CI mirrors these exact Make targets — if any of them fail locally, CI will fail and the push will be rejected. This is a hard gate; never push without passing checks.

**DO NOT OPEN A PR OR PUSH BEFORE ALL MAKE CHECKS PASS LOCALLY. NO EXCEPTIONS.**

The fastest, most complete signal is the root Makefile:

```bash
# One-shot: runs SDK + web UI + site + daemon checks (format, lint, type-check,
# clippy, tests). This is exactly what CI runs.
make check
```

If you only changed one area, the narrower targets are faster:

```bash
# Rust daemon — cargo fmt --check, cargo clippy --all-targets -- -D warnings,
# cargo test --workspace (must all pass)
make check-daemon

# Web UI — typecheck + eslint + prettier format:check
make check-web

# Public marketing site
make check-site

# Go SDK + wctl — build, vet, test -race, golangci-lint for both modules
make check-go
```

Direct tool invocation is also fine if you want tighter iteration:

```bash
cd source/daemon && cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test --workspace
cd source/web-ui  && yarn format && yarn lint && yarn type-check
```

> **Note:** Direct `cargo` commands only work on Linux. On macOS, always use `make check-daemon` which runs inside a container.

### Common mistakes to avoid

- Running only `cargo build` and assuming tests pass — the test compile target has its own stubs that can fall out of sync with service signatures; always run `cargo test --workspace` (or `make check-daemon`) before pushing.
- **NEVER use `cargo clippy -p <crate>` or `cargo fmt` as a push gate** — scoped clippy misses `--all-targets`; running `cargo fmt` without `--check` silently auto-fixes locally while the pushed commit was never verified. RUN `make check-daemon` EVERY TIME.
- Running `yarn build` but skipping `yarn lint` — Vite is permissive about lint warnings that ESLint elevates to errors in CI.
- Pushing a rebase without re-running checks locally — dependency bumps pulled in from `main` can change lint/type rules; treat every rebase as a fresh change.

## Code coverage (MANDATORY for Rust changes)

We use `cargo-llvm-cov` for code coverage. Before starting work, compute the current coverage baseline on `main` (or during planning). After implementation, run it again on your branch and verify coverage **does not decrease**. New code must have tests — coverage should stay the same or increase. It must never go down.

```bash
# One-shot: runs tests with instrumentation and prints a per-file summary.
# On macOS this runs inside a Linux container (same as check-daemon).
make coverage-daemon
```

The `--ignore-filename-regex` (defined once in the Makefile's `COV_IGNORE` variable) excludes files that are not unit-testable (binary entrypoint, no-op/stub implementations prefixed with `noop_`, database pool setup, static file serving, Tower middleware boilerplate, auth context thread-locals, and Linux-only kernel interface modules). CI calls the same Makefile target with `COV_FMT` overridden for LCOV output.

## Boundaries

### Always do

- Use parameterized `.bind()` queries for all SQL — never string-interpolate user input.
- Write tests for new functionality.
- Follow the layered architecture: handlers → services → repositories.
- Keep API handlers thin — business logic belongs in services.
- Use existing trait patterns for new features.
- Run `make openapi` and commit `docs/openapi.json` whenever you change an
  HTTP handler annotation, DTO shape, or anything else that affects the
  generated OpenAPI spec. CI's `check-openapi` job gates on the committed
  file matching `cargo run -p wardnetd-api --bin dump_openapi`.

### Ask first

- Adding new dependencies to `Cargo.toml` or `package.json`.
- Modifying database migrations.
- Changing public API contracts or response shapes.
- Deleting files or removing functionality.
- Modifying CI pipeline.

### Never do

- Commit secrets, API keys, database files, or `.env`.
- Put SQL queries in API handlers (bypass the repository layer).
- Use `unsafe` Rust without explicit approval.
- String-interpolate user input into SQL queries.
- Skip or delete failing tests.
- Run the daemon as root.
