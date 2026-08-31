# CI token permissions

Every `write` scope granted to a workflow in this repository, why it exists,
and whether it can go away.

Scorecard's `Token-Permissions` check scores **0 for any write**, so a green
score is not the goal and is not reachable while the pipeline signs releases,
caches coverage baselines and uploads SARIF. The goal is that every write is
**job-scoped, necessary, and explained at the point it is granted** — so a
reviewer can tell a required grant from an accidental one without running the
pipeline. This file is the index; the reasoning lives next to each grant.

Last audited: 2026-08-31, against `main` at `310c12b9`.

## Granted and required

| workflow | scope | why |
|---|---|---|
| `ci-build.yml` | `security-events: write` | `codeql-action/upload-sarif` publishes clippy findings to Code Scanning. Silently stops publishing without it — it does not fail. |
| `ci-test.yml` | `contents: write`, `pull-requests: write` | Ceiling for `coverage.yml`'s bulwark job below. |
| `coverage.yml` | `contents: write`, `pull-requests: write` | bulwark caches coverage baselines on the `bulwark-state` branch and posts a sticky PR summary. |
| `cubit.yml` | `contents: write` | Pushes the `cubit-state` baseline branch on a push to `main`. |
| `release-edge.yml` | `contents: write` (×3) | Pushes the `edge-v*` tag, publishes the release, prunes superseded ones. |
| `release-edge.yml` | `id-token: write` | SLSA provenance attestation. |
| `prune-edge.yml` | `contents: write` | Deletes superseded edge releases and their tags. |
| `update-visual-snapshots.yml` | `contents: write` | Commits regenerated Playwright baselines back to the branch. |

All of the above are **job-scoped**, not workflow-level, and each carries an
inline comment. None is reducible without losing the function it exists for.

## Granted by gt, not editable here

`ci-orchestration.yml`, `gt-sync.yml` and `dependabot-auto-merge.yml` are
rendered by [gt](https://github.com/pedromvgomes/gt) from `.gt-repo.yaml`;
editing them is drift that the next `gt repo sync` reverts.

| grant | why |
|---|---|
| `statuses: write` | `reusable-attest` records the validated-tree commit status. |
| `contents: write` + `pull-requests: write` on the bulwark stage | Same baseline-caching and PR-comment needs as above. |
| `security-events: write` on the build stage | This repo's own `stage_permissions.build`, for the SARIF upload. |
| `contents: write` (gt-sync, dependabot-auto-merge) | Committing a re-render, and merging a Dependabot PR. |

Narrowing any of these is a gt change, not a wardnet one.

## Removed

- **`packages: write` on `tests-e2e.yml`** — was workflow-level, so both e2e
  jobs held it, and it existed only to export a GHCR layer cache. The cache was
  never shown to pay for itself (baseline e2e runs ranged 14–30 min with it,
  which is wider than any effect it could have had), so it was removed and the
  write went with it. That also cleared the grant from `ci-end2end.yml` and the
  `stage_permissions.end2end` entry in `.gt-repo.yaml` — three findings from
  one deletion.

## Open questions, deliberately not changed

Both need a live run to settle, and neither is exercised by pull-request CI.

- **`release-edge.yml`'s `security-events: write`.** `run-checks: false` means
  `check-daemon` never runs, so nothing uploads SARIF — but the comment says
  the grant is required anyway because a called workflow may not exceed its
  caller and the leaf declares the scope. If GitHub validates that lazily, the
  grant is dead and can go. Removing it blind would break the edge release
  path, which no PR run would catch.
- **`cubit.yml`'s `pull-requests: write`.** cubit runs on push-to-`main` and
  `workflow_dispatch` only; neither carries pull-request context, so the sticky
  comment path looks unreachable. Retained until a dispatch run confirms it,
  because cubit is advisory and never fails the build — a silently missing
  comment is exactly the kind of thing nobody would notice.

## Not token findings

Scorecard also reports, on the same page:

- `PinnedDependenciesID` — first-party actions on moving majors
  (`wardnet/bulwark@v2`, `wardnet/cubit@v0`, `pedromvgomes/gt@v1`). Deliberate:
  fixes reach every repo without a bump. Third-party actions **are** SHA-pinned.
- `CodeReviewID` — `required_approvals: 0`, a solo-maintainer decision recorded
  in `.gt-repo.yaml`.
- `BranchProtectionID` — partly `require_up_to_date: false`, which
  `.gt-repo.yaml` explains: the attestation records the tree a run validated,
  so an up-to-date requirement forces re-runs that prove nothing new.
- `VulnerabilitiesID` — 5 advisories including `RUSTSEC-2026-0221`. Real
  dependency work, unrelated to workflow permissions, tracked separately.
