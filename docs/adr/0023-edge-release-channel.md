# 23. An edge release channel for unvetted, on-demand builds

Date: 2026-07-14

## Status

Accepted

## Context

Getting a code change onto real hardware costs about an hour. Every test
iteration pays the full release ceremony: merge the fix, open a release-prep PR
(CalVer bump, `VERSION` bump, release-notes doc, regenerated OpenAPI), wait for
CI, merge it, push a signed tag, wait for the release workflow (which re-runs
`check-daemon` and the entire E2E suite before it publishes anything). All of
that exists to protect *users* from a bad release. None of it helps the
operator who just wants to know whether the fix works on the Pi.

The cost is not theoretical. On 2026-07-14 a single day of debugging remote
access burned five releases (beta.4 published then retracted, beta.5), three
release-notes PRs whose only purpose was to let a tag be re-cut, and several CI
reruns of a flaky E2E spec that had nothing to do with any of the fixes. Worse,
the ceremony actively distorts the work: two of those release-notes PRs existed
solely because a GitHub release body is built from the notes file *in the
tagged tree*, so a fix that arrived after the notes were written forced the
whole cycle to restart.

The daemon already has the machinery for this — an auto-update runner, signed
manifests, a channel setting — it just has nowhere to point at that isn't a
blessed release.

## Decision

Add a third release channel, **edge**, fed by a dispatchable workflow that
publishes a signed daemon build from **any branch** in one step.

**Trigger.** `workflow_dispatch` with a branch input. Nothing is triggered by a
tag; the branch is named at dispatch time. Unmerged branches are explicitly in
scope — testing a candidate before merging it is the primary use case.

**Version.** `<base-calver>-edge.<run-number>`, e.g. `2026.07.00-edge.147`. The
CalVer's own pre-release suffix is stripped and the workflow's monotonic run
number supplies ordering. No `CALVER` bump, no `VERSION` bump, no release-notes
doc, no OpenAPI regeneration — the version is stamped at build time.

**What runs.** Build, sign, publish. **No `check-daemon`, no E2E.** For a
branch that came from a PR, those gates already ran; for an experiment, the
whole point may be to test a build the gates would reject. Every gate we add
re-imports the latency we are deleting.

**Publication.** One GitHub *prerelease* per build, tagged `edge-v<version>`
(deliberately not `v*`, so it cannot trigger `release.yml`), carrying the same
minisign-signed tarballs, sha256 digests, and asset layout as a real release —
so `install.sh` and the daemon's downloader need no changes. The manifest
generator learns to exclude `-edge.` from `stable.json` and `beta.json` (it
currently defines beta as "highest release overall", which would otherwise
silently point every beta box at an unvetted build) and emits `edge.json`. Edge
releases are pruned to the newest five.

**Gate.** A deploy-time `[update] allow_edge_channel` flag in
`/etc/wardnet/wardnet.toml`, default `false`. The service layer rejects
`channel = edge` unless it is set, `/api/update/status` advertises whether edge
is available, and the channel selector renders the option only when it is.
Putting a box on edge therefore requires root **on that box** — an admin
session is not enough. If the flag is removed from a box already on edge, the
daemon logs a warning at startup and falls back to `beta`, writing the change
back so the stored state never contradicts the config.

## Consequences

Edge builds are signed with the production key, so the *channel* remains
authentic — only our CI can produce an installable edge build. What is
deliberately absent is any promise that the code is good. That is the trade we
are making, and the `allow_edge_channel` gate is what keeps it away from
anybody who has not opted into it with root.

The pre-release ordering is a **one-way ratchet**: `"edge" > "beta"`
lexicographically, and the updater never downgrades, so a box on
`2026.07.00-edge.147` will not move to `2026.07.00-beta.6` if you flip it back
to beta — it waits for the next base CalVer. Two escape hatches exist and both
already work: re-run `install.sh` with `CHANNEL=beta` (the installer performs
no version comparison, so it installs whatever the manifest names, older or
not), or remove the TOML flag and restart (the box falls back to beta on the
next release with a higher base). We rejected teaching the updater to downgrade
on channel switch: a downgrade path cannot undo database migrations, and it
would be a loaded gun aimed permanently at user data to save an operator one
SSH command.

The first edge-capable daemon must still ship through the normal ceremony —
the daemon has to *know* the channel before it can follow it. One last slow
cycle buys unlimited fast ones.

We accept a modest amount of GitHub Releases clutter (five prereleases,
pruned). We rejected a single rolling `edge` release whose assets are replaced
in place: this repo has **immutable releases** enabled, which freezes a
published release's assets and permanently burns its tag name — the same wall
that forced beta.4 to be abandoned in favour of beta.5.
