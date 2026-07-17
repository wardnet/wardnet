---
name: release-prep
description: |
  Use this skill when the user asks to prepare or cut a new wardnet
  release (e.g. "let's release 2026.05.01", "cut a release", "prep
  the next release", "release prep"). Drives the full versioned-
  release flow: release-notes doc, version bumps with propagation,
  OpenAPI regeneration, PR. After the PR is merged, push the matching
  signed git tag — that is the only step the human gates after review.
---

# Cut a new wardnet release

End-to-end checklist for tagging a new versioned release. Two human
gates: PR review (required before merge), then tag push (required
after merge). Everything else is mechanical.

## Versioning model — three independent tracks

- **`./CALVER`** — user-facing version, shape `YYYY.MM.NN`. Becomes
  the git tag (`v<CALVER>`), the release tarball / image filenames,
  the `release_version` field in `/api/info`, the OpenAPI spec's
  `info.version`, and the version line in the web UI. `NN` is an
  **in-month counter starting at `00`**, not day-of-month — multiple
  releases the same day just bump `NN`, and a single release later
  in the month doesn't skip ahead to that day's number. CalVer must
  match `^[0-9]{4}\.[0-9]{2}\.[0-9]{2}([-.+].+)?$` — `make
  check-version` enforces the regex but not the counter semantics.
- **`./VERSION`** — daemon Cargo workspace SemVer. Required by
  Cargo's strict parser; not surfaced to users. `make sync-version`
  propagates it to `source/daemon/Cargo.toml`,
  `source/web-ui/package.json`, and `source/site/package.json`.
- **`./source/sdk/wardnet-js/VERSION`** — independent SemVer for the
  npm package. Decoupled cadence — bump only when the SDK has
  shipped real changes since its last release. Skip in a typical
  daemon-only point release.

For a typical point release: bump `CALVER` to the next free counter
slot in the current month (e.g. `2026.05.00` → `2026.05.01`) and
patch-bump `VERSION` (`0.1.0` → `0.1.1`). Feature-heavy releases
warrant a SemVer minor bump (`0.1.x` → `0.2.0`) — the daemon
SemVer is invisible to users but signals API/build-graph changes
to consumers of the workspace. Evaluate the SDK separately:

```sh
git diff v<previous>..origin/main -- source/sdk/wardnet-js
```

If the diff is empty or limited to comments / formatting, leave the
SDK pinned. If the diff has user-visible changes, raise it with the
user — SDK releases are decoupled and may not align with this PR.

## Step 1 — Draft the release notes

Path: `docs/releases/v<CALVER>.md`. This file is **not just
documentation** — `release-daemon.yml` reads it via `gh release
create --notes-file` to populate the GitHub Release body, so anything
landing here ships to users.

Template, modelled on `v2026.05.00.md`:

- Lead paragraph framing the release in one or two sentences,
  linking to the previous release notes.
- If there is any **action required** for upgraders (operator-run
  install step, manual data migration, breaking API change) put it
  in a `> [!IMPORTANT]` admonition immediately after the lead. Do
  not bury it.
- `## Bug fixes` — one subsection per user-visible fix. Heading
  describes the *user-visible outcome*, not the code change. Body
  explains what users were seeing and what changed. Link the
  closing issue.
- `## Improvements` — features and UX work. Same shape.
- `## Upgrading` — auto-update flow as the default path; manual
  installer flow as the alternative. Repeat any `> [!IMPORTANT]`
  callouts here as concrete steps.
- `## Targets in this release` — **derive** the list from the build
  matrix in `.github/workflows/build-daemon.yml`. That matrix is what
  actually produces the assets, so it is the only thing that can be
  right. Do **not** copy the table from the previous release notes:
  that copies a copy, and an error in it survives every release that
  inherits it. Verify against what the last release really shipped —
  if the two disagree, the notes are wrong, not the assets:

  ```sh
  gh release view v<previous> --json assets --jq '.assets[].name'
  ```

  > This is not hypothetical. An `armv7-unknown-linux-gnueabihf` row
  > entered at `2026.06.00-beta.2`, rode the beta chain by copy, and
  > reached the `2026.07.00` stable notes — promising a 32-bit tarball
  > CI has never built, to exactly the Pi users most likely to need
  > one. `v2026.05.03` had it right the whole time. Nobody checked the
  > table against the matrix because the instruction never said to.

Sourcing the changelog:

```sh
git log v<previous>..origin/main --oneline
```

For each merged PR, decide: would an end user see or care? If yes →
in scope. If it's CI plumbing, agent docs, dependabot bumps,
internal tooling, or a refactor with no user-visible effect → out of
scope. Ask the user when uncertain rather than guessing.

## Step 2 — Bump versions

```sh
echo <NEW_CALVER>  > CALVER
echo <NEW_VERSION> > VERSION
make sync-version    # CALVER does not propagate — build.rs reads it
                     # directly. VERSION propagates into Cargo.toml +
                     # web-ui/package.json + site/package.json.
make check-version   # gate: every file agrees
```

`make sync-version` also touches `source/sdk/wardnet-js/package.json`
with the SDK VERSION file's value. If you intentionally did not bump
the SDK, that's still a no-op (same value in, same value out).

## Step 3 — Regenerate the OpenAPI spec

`CALVER` flows into `docs/openapi.json` via the daemon's `build.rs`,
not via `sync-version`:

```sh
make openapi
```

Expected diff: **only** the `info.version` field. Any other change
means `main` already drifted from the committed spec — investigate
before continuing (probably a missed `make openapi` in a prior PR).

## Step 4 — Rebuild embedded web apps

The daemon binary embeds three web apps at compile time via
`rust_embed`. The dist folders for `user-app` and `admin-app` are
committed to the repo (not CI-built); `admin-site/web/dist` is
CI-built. For every release, rebuild the committed dists so the
shipped binary contains the latest source:

```sh
cd source/user-app  && yarn install --immutable && yarn build
cd source/admin-app && yarn install --immutable && yarn build
```

Commit the resulting dist changes alongside the version files. Any
new content-hashed asset files (fonts, icons, JS/CSS bundles) must be
staged; deleted old hashes must be unstaged/removed.

**Do not rebuild `source/admin-site/web/dist/`** — that is managed by
the CI `build-web` job and only the placeholder `index.html` is
tracked.

## Step 5 — Verify

```sh
make check-version    # version pin agreement (also wired into CI)
make check-daemon     # fmt + clippy -D warnings + workspace tests (embeds rebuilt dists)
make check-openapi    # drift gate — fails if docs/openapi.json stale
make check            # full belt-and-braces (SDK + web + site + daemon)
```

`make check-daemon` will refresh `Cargo.lock` because the workspace
package version is part of the dependency graph; commit the lockfile
update with the rest.

On macOS, `check-daemon` runs in a Linux container. `make openapi`
runs natively even on macOS — the daemon's `build.rs` dependency
graph is portable enough for this one command.

## Step 6 — Commit, push, open PR

Worktree (per `.claude/rules/worktree-per-session.md`):

```sh
gt wt add chore/release-prep-<calver-with-dashes>
cd chore/release-prep-<calver-with-dashes>
```

Single commit, scope `chore(release):`:

```
chore(release): prep v<CALVER>

<one-paragraph summary mirroring the release-notes lead, plus any
action-required upgrade note repeated verbatim>
```

Files in the commit:

- `CALVER`, `VERSION`
- `docs/releases/v<CALVER>.md`
- `docs/openapi.json`
- `source/daemon/Cargo.toml`, `source/daemon/Cargo.lock`
- `source/web-ui/package.json`, `source/site/package.json`
- `source/user-app/dist/` — all files (new bundles + deleted old hashes)
- `source/admin-app/dist/` — all files (new bundles + deleted old hashes)
- (only if the SDK is being bumped) `source/sdk/wardnet-js/VERSION`,
  `source/sdk/wardnet-js/package.json`

Do not bundle unrelated changes.

PR body should:

- Categorise the merged PRs covered by this release (bug fixes,
  improvements) with links and closing issues, mirroring the
  release-notes doc.
- Surface any action-required upgrade note prominently, so reviewers
  don't miss it during review.
- List the test plan: `make check-version`, `make check-openapi`,
  `make check-daemon` all green.

## Step 7 — Tag (after merge)

**Stop here. Ask the user to confirm the PR is merged before
tagging.** Tag pushes are not reversible without coordinating across
the auto-update manifest and the release workflow, so this gate is
explicit.

After confirmation:

```sh
cd <main-worktree>
git fetch origin
git checkout origin/main
git tag -s v<CALVER> -m "v<CALVER>"   # falls back to -a if no GPG key
git push origin v<CALVER>
```

What the tag triggers (`.github/workflows/release.yml` watches
`v*.*.*`):

- Cross-platform build of `wardnetd` and the post-upgrade binaries.
- minisign-signed tarballs + sha256 digests + signatures uploaded as
  release assets.
- `release-daemon.yml` creates the GitHub Release using
  `docs/releases/v<CALVER>.md` as the body.
- `deploy-site.yml` regenerates `releases/stable.json` and
  `releases/beta.json` so auto-update clients pick up the new
  version on their next manifest poll.

Watch the workflow run; if any step fails, do not retry blindly — a
failed publish can land partial assets that auto-update will then
try to verify and reject.

## Common pitfalls

- **Forgetting `make openapi`.** `check-openapi` will catch it in
  CI, but only after a round trip. Run it locally and commit the
  diff.
- **Bumping `VERSION` but not running `make sync-version`.**
  Cargo.toml stays at the old value, the workspace builds against
  it, and `check-version` fails CI.
- **Editing `docs/openapi.json` by hand to bump the version.** The
  file is generated; hand-edits drift the moment any annotation
  changes. Always regenerate via `make openapi`.
- **Pushing the tag from a stale local `main`.** `git fetch` and
  check out `origin/main` explicitly before tagging, otherwise the
  tag may point at a commit that wasn't actually reviewed.
- **Auto-update assumes monotonic CalVer.** Don't reuse a CALVER for
  a re-tag; bump to the next free patch slot if you need to redo a
  release.
- **Trusting the targets table because it was already there.** Every
  other pitfall on this list is caught by a gate — `check-version`,
  `check-openapi`, CI. This one is caught by nothing. It is prose,
  sitting next to a generated asset list it is never diffed against,
  and it stayed wrong across six releases precisely because each one
  inherited it from the last. Derive it from `build-daemon.yml` and
  diff it against `gh release view v<previous> --json assets`.
