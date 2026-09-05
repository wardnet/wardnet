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
- **[DNS forwarding ladder](.agents/architecture.md#dns-forwarding-ladder-issue-1199)** —
  why the default forwarder walks its own ladder of single-server resolvers
  instead of one multi-server hickory resolver (which races
  `num_concurrent_reqs = 2` servers regardless of `ServerOrderingStrategy`, so
  "Failover (in order)" queried two providers at once and no honest
  `dns_query_log.upstream` was possible); `UpstreamPool`'s `all` vs `serving`
  split and how the latency prober's `reachable` flag became load-bearing;
  the explicit bounds (`upstream_timeout_ms` per rung, `forward_deadline_ms`
  overall, `attempts = 0`) that replaced hickory's inherited 20-30s worst case.
  Invariants: **a negative answer is terminal** (never fail over on
  NXDOMAIN/NODATA), an **unmeasured** upstream is not a down one, and an
  exhausted ladder blames **no** upstream.
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
- **[Private DNS](docs/adr/0029-private-dns-dot.md)** — why the `DoT`
  `:853` listener treats the TLS **SNI** as both authentication and
  attribution, and why the resolver is therefore closed (the apex slug is
  public via CT logs, so apex serving is not acceptable); the per-device
  secret hostname `<token>.<fqdn>` riding the existing wildcard SAN so no
  token ever reaches a CT log; the two paths one hostname takes
  (split-horizon to the Pi on the LAN, the **Tunneller**'s
  `FRAME_CONNECT dest_port=853` while roaming); DoT-only until #816
  unblocks `:443`; Premium gating with a *persisted* disable on
  entitlement loss; and the Android constraints that shape the listener —
  **no ALPN advertised**, publicly-trusted chain, fail-closed. Also the
  **Tunneller**-vs-**Private DNS** terminology split (relay infrastructure
  vs user-facing feature).
- **[Application hosting](docs/adr/0030-published-apps.md)** — why a
  **published app** is a name + a **reach ladder** (LAN always on; Remote
  peer and Public as widening opt-ins) + an **access policy**, rather than
  ADR-0022's mechanism × visibility matrix (whose §2 this supersedes); why
  **no DNAT primitive exists** — Wardnet *is* the router, so v1 raw-L4
  publishing is an authoritative DNS record plus a narrow
  `ZoneEnforcementService` exception; why the Public rung is
  **HTTPS/WebSockets only** (the edge demuxes by SNI, which raw L4 has
  none of) and public L4 is deferred with its cloud port allocator; and
  why the **app catalog is compiled into the binary** like `vendors.toml`.
  Invariant: a published app's reachability probe **never** feeds the
  watchdog's `HealthMonitor`.
- **[Household identity](docs/adr/0031-household-identity.md)** — why the
  user directory is **box-local** and wardnet-cloud may pre-fill a hint but
  **never vouch for a box login** (no trust edge from cloud into a home
  network, at the accepted cost of local-only account recovery); why
  **device affinity is attribution and never authentication** (device
  identity is source-IP-derived, so affinity-as-credential would collapse
  admin to IP spoofing) and password-free entry comes from a **device-held
  session**; the **Admin role** + **Local admin** break-glass pair; and why
  a forward-auth gate breaks native mobile clients while app-native OIDC
  does not.
- **[Anomaly subsystem](.agents/architecture.md#anomaly-subsystem-issues-1097--225)** —
  the hardcoded `AnomalyType` catalog, the `AnomalyDetector` registry (one
  detector per type), and `AnomaliesDetectionEngine`'s two modes: a
  **preventive** per-detector sweep schedule and a **reactive** event-bus
  listener. The invariant that makes alerting edge-triggered is a *partial
  unique index* on `(anomaly_type, COALESCE(subject_id, ''))
  WHERE resolved_at IS NULL` — at most one open anomaly per subject, enforced
  by the database rather than by a service remembering it already alerted.
  Also why **per-entity errors are anomalies, not columns**: an open anomaly
  whose `subject_id` is a tunnel *is* that tunnel's current error, so there is
  no `tunnels.last_error`.
- **[Managed devices + retention](.agents/architecture.md#managed-devices--retention-subsystem-issue-1181)** —
  `devices.managed` as an explicit **latching** column (never derived from
  `name`), promoted by any *admin* configuration act and cleared only by an
  explicit **release**; the `DeviceRetentionRunner` that deletes unmanaged
  devices absent over 30 days; why the release orchestration lives in the API
  handler (an `Arc` cycle with `InboundWgService` / `PrivateDnsService`); and
  why the prune must **delete then evict** the MAC from discovery's in-memory
  maps. Invariant: **`managed = 0` implies no admin artefacts exist**, which is
  what makes the prune safe. A new per-device table must decide whether it
  promotes. See [ADR 0032](docs/adr/0032-managed-devices-and-retention.md).
- **[Household access requests](docs/adr/0033-household-access-requests.md)** —
  the single **access-request** inbox (`device_access_requests`, one `kind`
  discriminator) that replaced the rule-request one, and why approval dispatches
  through an **approver registry** rather than a `match` — a kind with no
  registered approver is record-only *by construction*, which is exactly the
  state `allow`/`block` are in. Covers why `/api/requests` was rejected as a
  resource name, why reconciliation with out-of-band grants goes over the event
  bus (approving calls `PrivateDnsService`, so the reverse would be a cycle),
  and the two filter-model facts that pushed rule auto-apply into its own issue:
  profiles combine by **rank, not order**, and assigning any explicit
  `profile_ids` **drops the household defaults**. Invariant: **asking never
  promotes a device to managed** — only the approval's `grant_device` does.
- **[Query-log normalisation](docs/adr/0034-query-log-normalisation.md)** —
  why `dns_query_log` moved its seven repeated text columns onto
  `(id INTEGER PRIMARY KEY, v TEXT UNIQUE)` lookup tables and an epoch
  `timestamp` (591 MB → **146 MB**, measured — 109 MB for the normalisation plus
  ~37 MB of integer indexes it cannot run correctly without), and why it is
  **a space change, not a speed change**. Covers the rejections that a reader would otherwise re-propose:
  **no id cache** (per-batch resolution already removed the cost, and the cache
  was the only thing forcing the prune's placement), **no FK into `devices`**
  (`devices.id` is `TEXT`, so it saves nothing, and device retention deletes rows
  the log must outlive — plus `VACUUM` may renumber a non-`INTEGER` table's
  rowid), **no integer enums** for the closed columns (`DnsQueryResult::slot` is
  a compile-time exhaustiveness device, not a wire format), and **no FTS5**
  (+232 MB and slower than `LIKE`). Invariants: only **`lk_dns_domain`** is pruned —
  the others grow far more slowly and the `SELECT DISTINCT` scan is paid per
  table; the prune uses **`NOT IN (SELECT DISTINCT …)`**, never a correlated
  `NOT EXISTS` (135 s), and **`dns_query_log(domain_id)` must stay indexed** or
  `PRAGMA foreign_keys=ON` makes each orphan scan the whole log (33.5 s vs
  0.016 s on 500k rows — any timing taken in the `sqlite3` CLI has foreign keys
  *off* and does not apply); and **nothing above `wardnetd-data`
  knows lookup tables exist**, which is what keeps the API contract unchanged.
- **[Query-log read path](docs/adr/0035-query-log-read-path.md)** — why the
  admin log's client filter resolves its substring against `lk_dns_client_ip`
  **in Rust** before touching the log, and how the resolved cardinality picks
  the predicate: none → empty page, one → `=` against the single-column
  `idx_dns_query_log_client_ip_id`, a handful → `IN`, more than 64 → back to the
  pattern (a guard, not a path a household reaches — the measured box holds 24
  clients). The load-bearing fact: **indexing `client_ip_id` does nothing for
  the `IN (SELECT …)` form**, because `ORDER BY q.id DESC` lets SQLite prefer the
  backwards primary-key walk and decline the index — only the resolved scalar
  `=` seeks it, measured 0.3 ms against 19.7 ms at 1.37M rows for a client whose
  rows have aged. Also why pagination is a `before` cursor rather than an offset,
  and why `next_cursor` is one-directional. Invariants: the index is
  **single-column** — under an equality constraint SQLite already walks it in
  rowid order, so `ORDER BY id DESC LIMIT n` needs no sort and a trailing `id`
  would only widen every entry; and **the endpoint has exactly one pagination
  model** — a second, offset-based one would keep the slow path reachable and
  tested.
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
