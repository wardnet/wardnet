---
name: bump-rust
description: |
  Use this skill when the user asks to bump the daemon's Rust toolchain
  (stable channel + MSRV + README badge + fuzzing nightly). Changes land
  atomically across source/daemon/Cargo.toml, the two rust-toolchain.toml
  files, CI workflows, Makefile, README badge, and the agent/dev docs.
  Also use this skill when the OSS-Fuzz builder reports
  `rustc X.Y.Z-nightly is not supported by the following packages:
  wardnet-common@0.2.0 requires rustc <MSRV>`.
---

# Bump Rust Toolchain

Single-pass checklist for moving the daemon forward one (or more) Rust
minor versions. Every pin below must move together — drifting pins
cause confusing failures (CI green but fuzz red, local green but CI
red, etc.).

## Target version

The daemon tracks **latest stable**. Confirm the current stable at
<https://www.rust-lang.org/> or `rustup check`. Let `$NEW` be the new
minor (e.g. `1.95`).

## Pins to update

One edit each. The final `grep -rn "1\.<OLD>" --include="*.md"
--include="*.yml" --include="*.toml" --include="Dockerfile*"
--include="Makefile"` must come back empty before committing.

| File                                       | Line pattern                              |
|--------------------------------------------|-------------------------------------------|
| `source/daemon/Cargo.toml`                 | `rust-version = "<NEW>"`                  |
| `source/daemon/rust-toolchain.toml`        | `channel = "<NEW>"`                       |
| `.github/actions/setup-rust/action.yml`    | `default: "<NEW>"`                        |
| `.github/workflows/coverage.yml`           | `toolchain: "<NEW>"`                      |
| `.github/workflows/security.yml`           | `toolchain: "<NEW>"`                      |
| `Makefile`                                 | `RUST_IMAGE := docker.io/library/rust:<NEW>` |
| `README.md`                                | `[![Rust](...rust-<NEW>-orange...)]`      |
| `.agents/technical-stack.md`               | `Rust <NEW> (pinned in ...)`              |
| `.agents/commands.md`                      | `rust:<NEW>` in the check-daemon entry    |
| `docs/DEVELOPMENT.md`                      | tech-stack table row + prerequisites line |

Do **not** re-introduce `edition 2024` alongside the version. Editions
live ~3 years, bumping them is a separate project-wide event, and
mentioning the edition on every version line just means we forget to
update it when the edition actually changes.

The production `source/daemon/Dockerfile` has no Rust pin — it
installs a pre-built release binary, so nothing to change there.

## Fuzzing nightly

Policy: the fuzz workspace uses the **last nightly of the matching
minor version**, not whatever nightly is current. Running the fuzzer
on two unreleased compiler versions ahead of production adds
nightly-only codegen risk for no benefit.

Rust's release cadence is strict 6 weeks. Given stable `<NEW>` shipped
on date `S`, the `<NEW>`-nightly window ran from `S - 12 weeks` to
`S - 6 weeks` (i.e. the cycle when `<NEW>` was in beta is *after*,
the cycle when it was nightly is *before*). Pick a date near the end
of that window — late fixes, no branch-cut risk.

To resolve a specific date to a rustc minor, run:

```bash
podman run --rm --platform linux/amd64 \
  gcr.io/oss-fuzz-base/base-builder-rust@sha256:<current-digest-from-.clusterfuzzlite/Dockerfile> \
  bash -c 'rustup toolchain install nightly-YYYY-MM-DD --profile minimal --no-self-update 2>&1 | grep installed'
```

Output looks like `nightly-YYYY-MM-DD-... installed - rustc X.Y.Z-nightly (...)`.

Update `source/daemon/fuzz/rust-toolchain.toml`:

- `channel = "nightly-YYYY-MM-DD"`
- Update the header comment's worked example (date window, MSRV
  reference) so the next bumper has the numbers in front of them.

Do **not** update `.clusterfuzzlite/build.sh` — it parses the channel
out of the toml at build time.

## Verify

1. `make check-daemon` — runs fmt, clippy (`-D warnings`), and the
   full workspace test matrix inside the `rust:<NEW>` container on
   macOS, natively on Linux. Every new minor typically ships a handful
   of pedantic clippy lints; fix them in the same commit (see *Common
   new lints* below).
2. Sanity-check the fuzz toolchain in the OSS-Fuzz base image:
   ```bash
   podman run --rm --platform linux/amd64 \
     gcr.io/oss-fuzz-base/base-builder-rust@sha256:<digest> \
     bash -c '
       mkdir -p /tmp/t && cd /tmp/t
       printf "[toolchain]\nchannel = \"nightly-YYYY-MM-DD\"\ncomponents = [\"rust-src\"]\n" > rust-toolchain.toml
       rustup toolchain install "$(awk -F \" "/^channel/ {print \$2}" rust-toolchain.toml)" \
           --profile minimal --component rust-src --no-self-update
       export RUSTUP_TOOLCHAIN="$(awk -F \" "/^channel/ {print \$2}" rust-toolchain.toml)"
       rustc --version
     '
   ```
   Expected: `rustc <NEW>.0-nightly (...)`. Anything else (e.g.
   `<NEW-1>` or `<NEW+1>`) means the date is outside the matching
   window — pick another date.

## Common new lints (reference, not exhaustive)

Every stable bump adds or tightens clippy pedantic lints. Recent
examples worth knowing:

- `duration_suboptimal_units` (1.95+) — `Duration::from_secs(60)` →
  `from_mins(1)`, `from_secs(3600)` → `from_hours(1)`, etc. Note:
  `Duration::from_days` / `from_weeks` are still unstable as of 1.95
  (tracking issue #120301) — use `from_hours(24 * N)` for day
  literals.
- `map_unwrap_or` — `.map(f).unwrap_or(a)` → `.map_or(a, f)`.
- `unnecessary_sort_by` — `sort_by(|a, b| b.k.cmp(&a.k))` →
  `sort_by_key(|x| std::cmp::Reverse(x.k))`.

Fix them in the same commit as the version bump; never `#[allow]`
just to unblock — if a new lint fires it usually caught something
worth fixing.

## Commit shape

One commit, scoped `chore(rust):`. Subject names the new version.
Body calls out:

- The new stable version and why it's being bumped (latest stable,
  or fixes an OSS-Fuzz MSRV error).
- The new fuzz nightly date + resolved rustc (e.g.
  `nightly-2026-02-20 → rustc 1.95.0-nightly (7f99507f5)`).
- Any clippy fixes piggy-backed on the bump.

See commit `566ba7e` for the canonical example (1.94 → 1.95 + fuzz
`nightly-2026-04-15` → `nightly-2026-02-20`).
