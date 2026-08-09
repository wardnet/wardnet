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
- **[Local-DNS subsystem](.agents/architecture.md#local-dns-subsystem-issue-217)** —
  `AuthoritativeView` (ArcSwap-backed, lock-free), resolution pipeline order
  (authoritative → cache → filter → conditional/tunnel upstream),
  event-driven rebuild on `DnsLocalChanged`, and why background runners
  (including `DnsRunner`) call `DnsLocalService` rather than holding
  `dns_local_repo` directly.
- **[DDNS subsystem](.agents/architecture.md#ddns-subsystem-issue-527--521-umbrella)** —
  `DnsProvider` trait (bridge + Cloudflare impls), `DdnsService` (auth-gated, stores config in
  `system_config` and secrets in `SecretStore`), `DdnsUpdateRunner` (idle-until-configured 5-min
  tick), region catalog with concurrent latency probing, and WAN IP discovery.
- **[Watchdog + health subsystem](.agents/architecture.md#watchdog--health-subsystem-issue-214)** —
  three-layer recovery: `HealthMonitor` (`HealthCheck` trait, `ArcSwap` snapshot,
  concurrent refresh with per-check timeout + Y-consecutive debounce) → health-gated
  **soft** watchdog (`sd_notify(WATCHDOG=1)` ⇒ systemd `WatchdogSec=15` service
  restart) → **ungated** hard watchdog (`/dev/watchdog` ⇒ kernel host reboot). Plus
  unauthenticated `GET /health`, `Type=notify` + `READY=1`, and the `WatchdogOps`
  trait. Invariant: the hardware pet is never health-gated.
- **[Network-Zone enforcement subsystem](.agents/architecture.md#network-zone-enforcement-subsystem-issue-736)** —
  per-zone nftables **egress gate** (forward-chain drop of `wg_ward*`/WAN egress a
  zone forbids) + **admin-UI gate** (input-chain TCP-reset of device→Pi :443/:7411
  when a zone is not admin-reachable; DNS/DHCP pass). A dedicated
  `ZoneEnforcementService` + `ZoneEnforcementListener`, separate from the routing
  listener, keyed by device IP via comment UDATA (restart-survivable), live-reloaded
  on zone/device events with conntrack flush, reconciled on startup. Closes the
  global-default-policy caveat via a new `DefaultPolicyChanged` event + a callback
  that unbinds forbidden tunnel bindings to direct. Honest limit: same-subnet peer
  traffic is unaffected (the AP's job).
- **[Device identification](docs/adr/0025-device-identification.md)** —
  why placeholder IEEE listings (`Private`, registry filler) are dropped so
  `lookup_manufacturer` returns `None` by construction; the `is_randomized`
  flag that replaced the `"Randomized MAC"` manufacturer sentinel; the single
  `vendors.toml` **vendor catalog** driving every signal kind (OUI override,
  TCP port, mDNS service, DHCP option 60); `manufacturer_source`
  (`ieee` | `catalog` | `signal`) and why a curated override always renders as
  a hedge; the ±4-over-48-bit neighbour search for the BLE-vs-Wi-Fi MAC trap.
  Invariant: **nothing probes a device without a direct admin action.**
- **[Uninstall + shutdown teardown](docs/adr/0028-shutdown-teardown-and-uninstall.md)** —
  why a stopped daemon now deletes the `inet wardnet` table and its
  `wg_ward*` interfaces, and why a **self-initiated restart deliberately
  does not** (the replacement process inherits correct kernel state, so
  tearing down on every six-hourly auto-update is pure churn). Critically:
  shutdown tears tunnels down **through `TunnelService`, not the raw
  interface**, so each tunnel is recorded `Down` and the next boot's
  routing reconcile brings it back on demand — deleting the interface
  behind the database's back is unrecoverable, because
  `handle_tunnel_down` strips the routing and nothing recreates the
  interface. Covers the
  `ShutdownCause` gate, `wardnetd uninstall` owning the implementation
  because ADR 0013 removed the `nft` CLI dependency, the generated
  `/usr/local/sbin/wardnet-uninstall` escape hatch, and the
  default/`--purge` tiers. Invariant: **teardown deletes only our named
  table, never a ruleset flush.**
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

## Domain glossary

[`CONTEXT.md`](CONTEXT.md) is the canonical glossary for domain terms used
across issues, design docs, and code comments. It covers the three app
surfaces (admin site, user PWA, admin mobile PWA), identity model
(device-keyed vs admin-session), infrastructure (DDNS service, daemon-owned
TLS termination, path-based routing), and planned features (route
verification, VAPID push).
Read it before working on any of the PWA initiative issues (#435–#441).
