# Wardnet

Self-hosted network privacy gateway for Raspberry Pi. See [README.md](README.md) for full overview.

## Agent memory

Agent memory files live at the **repo root** under
`.claude/agent-memory/<agent-type>/MEMORY.md`. When saving or reading
agent memory, always use the repo root path, NOT a subdirectory like
`source/daemon/`.

## Documentation map

This file is an index. Detailed agent-facing conventions live in
focused documents under [`.agents/`](.agents/). Each file is
self-contained — read the one that matches the kind of change
you're about to make, rather than the whole set.

- **[Commands](.agents/commands.md)** — `make` targets (preferred)
  and the direct `cargo` / `yarn` equivalents, per area.
- **[Project structure](.agents/project-structure.md)** — the
  full source tree with a one-line purpose per module.
- **[Technical stack](.agents/technical-stack.md)** — versions
  and key dependencies for the daemon, SDK, web UI, and public site.
- **[Architecture](.agents/architecture.md)** — the layered
  design, trait-based boundaries, where each crate sits in the
  stack, and why database-provider concerns live next to the
  repositories rather than in the backup service.
- **[Backup subsystem](.agents/backup.md)** — how
  `BackupArchiver`, `DatabaseDumper`, and `SecretStore` compose
  into the export/import flow, plus the two-phase apply and the
  background cleanup runner.
- **[Stats subsystem](.agents/architecture.md#stats-subsystem-issue-409)** —
  generic pre-aggregating metrics pipeline: `StatsBuffer` → `StatsFlushRunner`
  → `stats_intraday` / `stats_daily`; `Meter` / `Counter` / `Gauge` instruments;
  `StatsService` with time-series and top-N queries; `/api/stats` and
  `/api/stats/top` endpoints. Also covers the DNS stats migration away from
  `DnsRepository`.
- **[Auth model](.agents/auth.md)** — setup wizard,
  unauthenticated vs admin endpoints, and the HARD REQUIREMENT
  that every service method opens with
  `auth_context::require_admin()?` or `require_authenticated()?`.
- **[Observability](.agents/observability.md)** — the tracing
  span hierarchy every background component must follow, plus
  OUI database and versioning notes.
- **[Logging guidelines](.agents/logging.md)** — how to write a
  log line that's queryable in Loki and readable in stderr.
- **[Code conventions](.agents/code-conventions.md)** — Rust,
  SDK, and web UI style rules; OpenAPI annotation pattern;
  dependency-documentation format.
- **[Testing](.agents/testing.md)** — running tests and the
  mock/real-resource patterns for service, repository, and
  infrastructure tests.
- **[Workflow](.agents/workflow.md)** — git conventions,
  mandatory pre-push checklist, coverage rules, and the
  always/ask/never boundaries.
