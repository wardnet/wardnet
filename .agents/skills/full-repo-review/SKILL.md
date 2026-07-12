---
name: full-repo-review
description: |
  Multi-agent code review of the ENTIRE Wardnet codebase (every app, not a diff)
  whose output is a batch of session-sized GitHub issues to fix later. Use when
  the user asks to "review the whole codebase", "do a full-repo review", "audit
  all the apps", "find issues across the repo and file them", or wants a
  standing/scheduled deep review rather than a pre-PR diff review. NOT for
  reviewing a branch diff before a PR — use the pre-pr-review skill for that.
  Expensive (~250 agents); explicit opt-in only. Runs two workflows in order
  (docs-accuracy, then full-repo-review) and drafts issues for human triage
  before anything is filed.
---

# Full-repo review → GitHub issues

A repeatable, multi-agent audit of **all** of Wardnet that lands its output as
**GitHub issues fixed in separate later sessions**. It reviews production code
across every app, sweeps repo-wide invariants a diff review cannot, verifies each
finding adversarially, clusters survivors into session-sized issues, and hands
them to a human to triage and file.

This is deliberately expensive (~250 subagents, tens of minutes). It is an
opt-in, scheduled pass — never something to run casually or on a diff.

## Two workflows, run in order

Both scripts are bundled next to this file under `workflows/`. Launch them with
the `Workflow` tool via `scriptPath` (the workflow registry is loaded at session
start, so a freshly-synced script is not resolvable by `name`).

### Run A — `workflows/docs-accuracy.js` (ALWAYS first)

The review builds a **house-rules digest** from `AGENTS.md` / `CONTEXT.md` /
`.agents/*.md` and injects it into every reviewer. If those docs are stale, every
reviewer inherits the drift and files bogus findings. So Run A verifies the docs
against the real tree **first** and reports drift — it changes nothing.

- One agent per doc; each classifies every checkable claim as **doc-stale** (fix
  the doc), **code-drifted** (the rule holds, the code broke it → hand to Run B),
  or **ambiguous** (needs a human call — the agent must not guess).
- A synthesis agent finds cross-doc contradictions and rates **digest risk**
  (`safe` / `fix-first` / `blocked`).
- Output: `<target>/.reviews/<date>/docs-report.md`.

**STOP after Run A.** Present the report. The human decides which doc fixes to
apply (some rules are deliberately kept strict — do not "fix" a doc to match
code without asking). Apply the agreed fixes, commit/merge them, then continue.
Carry the `handoff_to_run_b` code-drift findings into Run B as seeds (see the
`SEEDS` array in `full-repo-review.js`).

### Run B — `workflows/full-repo-review.js`

Runs against the **corrected** docs. Phases: house-rules digest → repo-wide
invariant sweeps (auth-guard, SQL, OpenAPI, panics, deps, web-layering, test
layout) → 4-axis panel per production unit (correctness, security, design,
conventions) → adversarial verify → cluster → draft issues → completeness critic
→ scribe. Output under `<target>/.reviews/<date>/`: `report.md`,
`issues/NNN-*.md`, `issues/manifest.json`.

Smoke-test the plumbing first with `mode: "smoke"` (one small unit, one axis,
full pipeline) before committing to a full run.

## Invocation

Always pass **absolute** paths. Review `main` (the shipping code), not a feature
worktree, so filed issues reference code that actually ships. Record the exact
sha so "the review of main @ <sha>" stays a true statement.

```
// Run A
Workflow({ scriptPath: "<skill>/workflows/docs-accuracy.js",
           args: { date: "<YYYY-MM-DD>", target: "<abs path to main worktree>" } })

// Run B — smoke first, then full
Workflow({ scriptPath: "<skill>/workflows/full-repo-review.js",
           args: { date: "<YYYY-MM-DD>", mode: "smoke", sha: "<sha>",
                   target: "<abs path to main>", docsFrom: "<abs path to corrected docs>" } })
Workflow({ scriptPath: "<skill>/workflows/full-repo-review.js",
           args: { date: "<YYYY-MM-DD>", mode: "full", sha: "<sha>", target: "<abs path to main>" } })
```

`<skill>` is this skill's synced base directory (`.claude/skills/full-repo-review`).

### args (full-repo-review.js)

| arg | meaning |
|---|---|
| `target` | Absolute path to the worktree under review (default `main`). |
| `date` | Run date — the `.reviews/<date>/` subdir. `Date.now()` is unavailable in workflow scripts, so it MUST be passed. |
| `sha` | The reviewed commit, for the report + issue footers. |
| `mode` | `full` or `smoke` (smoke = one unit/one axis, for plumbing checks). |
| `docsFrom` | Where to read the house-rules docs, if different from `target` (e.g. corrected docs not yet merged to main). Defaults to `target`. |
| `units` | Optional `[key]` — review only these units. Use to re-review a thin/failed unit. |
| `skipSweeps` / `skipSeeds` | Skip the repo-wide sweeps / docs-audit seeds — use on a focused `units` re-run so they aren't redone. |
| `extra` | Extra standing instruction injected into every reviewer prompt this run (e.g. "read these privileged files in full; don't grep-sample"). |

## Pre-flight

- Confirm the `main` worktree is on `main` and reset to `origin/main` (`git -C <main> fetch origin main && git -C <main> reset --hard origin/main`). Record the sha.
- Locally exclude the output dir so the review never dirties the tree:
  `printf '.reviews/\n' >> "$(git -C <main> rev-parse --git-path info/exclude)"`.

## Coverage gaps and re-runs

The completeness critic (in `report.md`) names what the pass missed — units not
covered, defect classes no axis could catch, suspiciously thin coverage, agents
that errored. When a unit is thin or its agents errored, re-run just that unit:
`{ units: [key], skipSweeps: true, skipSeeds: true, extra: "<deepen-coverage instruction>" }`
into a separate `-gaps` date, then merge its survivors into the main issue set.
This is how the netlink/watchdog backends get the depth they need — thin coverage
on the highest-privilege code is the most dangerous gap.

## Triage and filing (human-gated — nothing is auto-filed)

The workflows draft; they never call `gh`. Filing is a separate, explicit step.

1. **Severity → priority is P1/P2/P3 only.** The review NEVER self-assigns
   `priority:P0`. A finding with a verified security / data-loss / gateway-
   availability scenario is *flagged* as a release-blocker candidate at the top
   of `report.md` and in the run summary — the human decides if it is a P0.
2. Present the drafts for triage. A rendered triage view (per-issue priority
   control incl. P0, file/cut selection, exportable decisions) works well; build
   it from `manifest.json` + each issue's `## Problem`.
3. Take the human's decisions (which to file, final priority each), then:
   - Create the `review` label if absent:
     `gh label create review --color 5319e7 --description "Filed by the full-repo code review workflow"`.
   - File each surviving issue as the repo's scoped gh user (read the `--user`
     from the bare-repo root `.envrc`; do not guess). Replace the manifest's
     `priority:` label with the human's choice; keep the `component:*`, `bug`/
     `enhancement`, and `review` labels.
   - `gh issue create --title <title> --body-file <abs path to issues/NNN-*.md> --label ...`
     — pass **absolute** `--body-file` paths (the shell cwd may differ from the
     review dir). Every label must already exist in the repo taxonomy.
4. `gh issue list --label review` retrieves the whole batch; a future run can read
   it to avoid re-filing, and it can be bulk-closed if a pass is a bust.

## Design invariants (why the pipeline is shaped this way)

- **Adversarial verify** — every RED gets 3 perspective-diverse skeptics
  (reachability / already-handled / exploitability) prompted to *refute*,
  defaulting to refuted when uncertain; RED survives on majority, AMBER on one.
  Carve-out: conventions/doc-drift findings are not refuted for lacking a runtime
  failure scenario (they have none by definition) — judged only on "does the rule
  exist and does the code violate it".
- **Sized by production LOC** — roughly half the daemon is in-crate test code, so
  units are sized by production lines, not raw LOC.
- **No standalone performance axis** — with no profiler, a perf pass produces
  speculative micro-optimizations; real hot-path defects are the correctness
  axis's job. The standing order in every prompt: *a finding without a concrete
  failure scenario is not a finding.*
- **Scribe writes the files** — workflow scripts have no filesystem access, so a
  final agent writes `report.md` / `issues/` and keeps the payload out of the
  coordinator's context.

## When NOT to use this

- Reviewing a branch diff before a PR → **pre-pr-review**.
- Resolving incoming PR review comments → **pr-review-resolver**.
- Fixing one known bug → **tdd-bugfix**.
