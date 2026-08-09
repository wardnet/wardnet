# Architecture

## Layered design with dependency injection

```
wardnetd (main.rs)   →  wires real Linux backends, calls init_services(), starts axum server
                              │
wardnetd-api          │  AppState + Axum router: thin handlers, extract request, call service
                              ↓
wardnetd-services     │  Services struct + init_services(): AuthService, BackupService,
                      │  DeviceService, TunnelService, RoutingService, DhcpService,
                      │  VpnProviderService, SystemService, LogService, UpdateService
                              ↓
wardnetd-data         │  RepositoryFactory: AdminRepository, DeviceRepository, TunnelRepository,
                      │  DhcpRepository, DnsRepository, SystemConfigRepository, DatabaseDumper,
                      │  SecretStore, …
                              ↓
SQLite                │  Parameterized queries only (`.bind()`), never string interpolation

wardnet-common        ─  Shared types, config, events — referenced by all crates above
wardnetd-mock         ─  Dev binary: same wardnetd-api/services/data stack, no-op Linux backends
```

- **Traits define ALL boundaries** — every layer depends on trait interfaces, not concrete types. This includes infrastructure: `TunnelInterface`, `SecretStore`, `EventPublisher`, `FirewallManager`, `PolicyRouter`, `CommandExecutor`, `PacketCapture`, `DhcpSocket`, `DatabaseDumper`, `BackupArchiver`, `NordVpnApi` (provider-specific HTTP abstraction).
- **`wardnetd-services`** exports a `Services` struct and `init_services()` function — the single wiring point for all service implementations.
- **`AppState`** (in `wardnetd-api`) holds `Arc<dyn Service>` trait objects; no pool exposed to handlers.
- **API handlers never touch the database** — they call services, services call repositories.
- **Cross-service access is always service-to-service** — if service A needs data or behaviour from domain B, it receives and calls `Arc<dyn BService>`. Service A must never hold `Arc<dyn BRepository>` from domain B. Injecting a sibling repository directly bypasses that domain's business rules and is the first step toward duplicating logic across services. Every operation a sibling needs must be exposed as a method on the owning service's trait.
- **Database-provider concerns live next to the repositories**: the `DatabaseDumper` trait + its SQLite impl live in `wardnetd-data/src/database_dumper.rs`, *not* in the backup module. A future non-SQLite provider ships its own dumper alongside its own repositories and the backup service picks it up through `RepositoryFactory::dumper()` with no service-layer changes.
- **Secret-store concerns are provider-owned too**: `SecretStore::backup_contents` / `restore_from_backup` live on the trait, so each provider (`FileSecretStore` today; `HashicorpVault`, `1Password`, etc. later) decides what travels with a bundle and what stays in the external service.
- **mDNS advertisement is a self-contained runner**: `wardnetd::mdns_advertiser::MdnsAdvertiser` (Linux production binary only) publishes `<hostname>.local.` so the setup wizard is reachable without a known IP. It is a runner, not a service — mirrors `HeartbeatRunner` / `RouteMonitor`, not a trait-backed service. The mock binary does not start it.

## Tunnel rebuild endpoint (issue #480)

`POST /api/tunnels/{id}/rebuild` — admin-only endpoint that invokes `TunnelService::rebuild(id)`. The service method calls `tear_down_core` then `bring_up_core` (the same path used by the internal watchdog). The handler returns 404 for unknown tunnel IDs and guards against issuing a rebuild while a test probe is already in flight (409 Conflict). The response is `RebuildTunnelResponse { ok: true }`.

Key conventions:
- `tear_down_core` / `bring_up_core` are the internal primitives shared between `test_tunnel`, the idle watchdog, and `rebuild`. Always call them — never reach into `TunnelInterface` directly from `rebuild`.
- Errors from `bring_up_core` are **logged** (not silently swallowed) before returning to the caller.
- The in-flight guard reuses the same `Arc<Mutex<HashSet<Uuid>>>` that `test_tunnel` already holds; this prevents a rebuild from racing a concurrent test.

## Stats subsystem (issue #409)

A generic pre-aggregating stats subsystem. All layers (data, services, API) are complete as of PR 2.

### Three-tier storage — mirrors the `tunnel_metrics` pattern

| Table | Granularity | Retention | Key |
|---|---|---|---|
| `stats_intraday` | 1-minute buckets (`bucket_ts` = Unix seconds truncated to minute) | 25 h (`INTRADAY_RETENTION`) | `(metric, labels, bucket_ts)` |
| `stats_hourly` | 1-hour rollup (`hour_ts`) | 8 days (`HOURLY_RETENTION`) | `(metric, labels, hour_ts)` |
| `stats_daily` | 1-day rollup (`day` = `YYYY-MM-DD` UTC) | 13 months / 397 days (`DAILY_RETENTION_DAYS`) | `(metric, labels, day)` |

The retention constants live in `wardnetd-services/src/stats/service.rs`; the
hourly tier was added by the `20260611000000_stats_hourly.sql` migration. The
`Hour` query granularity reads the `stats_hourly` table — it is **not** computed
on the fly from intraday rows.

### Instrument kinds

- **Counter** — upsert accumulates: `value = value + excluded.value`
- **Gauge** — upsert overwrites: `value = excluded.value`

The `kind` column (`"counter"` / `"gauge"`) governs which SQL branch fires in `SqliteStatsRepository::upsert_intraday`.

### Label design — avoid high-cardinality explosions

Labels are stored as a **sorted JSON object string** (e.g. `{"outcome":"blocked"}`). Expression indexes on `json_extract(labels, '$.outcome')`, `$.domain`, and `$.client` cover the common filter paths without requiring separate columns.

The metric set was deliberately split to bound worst-case intraday row counts:

| Metric | Labels | Notes |
|---|---|---|
| `dns.queries` | `{outcome}` | Counter per outcome; low cardinality |
| `dns.latency_ms` | `{outcome}` | Gauge per outcome |
| `dns.queries.by_domain` | `{domain}` | Counter; blocked queries only — keeps domain set bounded |
| `dns.queries.by_client` | `{client}` | Counter per client IP |

Do **not** use a single `dns.queries` metric with `{client, domain, outcome}` labels — that cross product explodes row counts.

### Shared types (`wardnet-common/src/stats.rs`)

`StatsQuery`, `StatsTopQuery`, `StatsBucket`, `StatsSeriesPoint`, `StatsQueryResponse`, `StatsTopEntry`, `StatsTopResponse` — used by both the service/repository layer and the API/SDK.

### Repository interface (`wardnetd-data/src/repository/stats.rs`)

`StatsRepository` trait methods: `upsert_intraday`, `rollup_daily`, `trim_intraday`, `trim_daily`, `query_intraday`, `query_daily`, `top_n`. Accessed via `RepositoryFactory::stats()`.

### In-process pipeline

```
DNS query (DnsLogSink::record)
    │
    ↓  Counter/Gauge instruments (Meter → StatsBuffer)
StatsBuffer  (in-memory HashMap, Mutex-guarded, ~10 s window)
    │
    ↓  StatsFlushRunner::perform_flush  (every 10 s)
StatsRepository::upsert_intraday  →  stats_intraday table
    │
    ↓  StatsFlushRunner::perform_maintenance  (every 1 h, also at startup)
StatsRepository::rollup_daily  →  stats_daily table
StatsRepository::trim_intraday / trim_daily  (retention enforcement)
```

### Service layer (`wardnetd-services/src/stats/`)

| Module | Purpose |
|---|---|
| `buffer.rs` | `StatsBuffer` — in-memory accumulator; counters sum, gauges overwrite |
| `meter.rs` | `Meter` factory + `Counter` / `Gauge` instruments (OTel-style API) |
| `service.rs` | `StatsService` trait + `StatsServiceImpl` (time-series and top-N queries; calls `require_admin`) |
| `flush_runner.rs` | `StatsFlushRunner` — background task: 10 s flush + 1 h maintenance; follows runner contract |

`StatsService::query` supports three granularities: `Minute` (raw intraday rows), `Hour` (the `stats_hourly` rollup table), and `Day` (the `stats_daily` rollup table).

### API endpoints (`wardnetd-api/src/api/stats.rs`)

| Method | Path | Body | Description |
|---|---|---|---|
| `GET` | `/api/stats` | `StatsQuery` JSON | Time-series query (minute/hour/day granularity) |
| `GET` | `/api/stats/top` | `StatsTopQuery` JSON | Top-N label values ranked by total |

Both endpoints require admin auth. `StatsService` enforces this via `auth_context::require_admin()`.

### DNS stats migration from `DnsRepository`

`DnsRepository` previously exposed `query_stats`, `top_domains`, `top_clients`, and `series_buckets`. These on-the-fly aggregation methods were removed in PR 2. DNS stats are now served entirely through the generic `StatsService` + `/api/stats` / `/api/stats/top` endpoints using the four `dns.*` metrics recorded by `DnsLogSink`.

## Local-DNS subsystem (issue #217)

Zones, custom records, and conditional forwarding rules managed via the admin UI. Three storage layers in `wardnetd-data`: `DnsZone`, `CustomDnsRecord`, `ConditionalForwardingRule` tables, all CRUD'd through `DnsLocalRepository`. The service layer is `DnsLocalServiceImpl` in `wardnetd-services/dns_local/`.

### AuthoritativeView — lock-free in-memory snapshot

`AuthoritativeView` (`wardnetd-services/dns/authoritative.rs`) is an immutable snapshot of all **enabled** records and forwarding rules, built from the repository at startup and replaced atomically on every `WardnetEvent::DnsLocalChanged`. It is held behind an `Arc<ArcSwap<AuthoritativeView>>` in `UdpDnsServer` so each query reads a consistent snapshot lock-free, with no contention on the write path.

Key design rule: records whose zone has `enabled = false` are excluded even if the record itself is enabled. Records with no zone are included when their own flag is set. Forwarding rules are sorted longest-domain-first so the first suffix match is always the most specific one.

The view also carries a **reverse (PTR) index**: every enabled A record whose value is a private/internal IPv4 (RFC 1918, link-local, RFC 6598 CGN) is inverted into an `IPv4 → records` map. This is the complement of the DHCP `.lan` forward integration — the same `{hostname}.lan → IP` records the `DhcpLanRunner` writes become the source of reverse answers, so the index rides the existing `DnsLocalChanged` rebuild with no extra data source or event. It lets the gateway answer `in-addr.arpa` PTR queries for private ranges locally (RFC 6303) rather than leaking them upstream.

### Resolution pipeline (per-query order)

| Step | What happens | Notes |
|---|---|---|
| **0 — upstream selection** | Client IP → `UpstreamId` from the applied-state routing snapshot; a device-authenticated (DoT) client keys on its device id through the persisted-rule `dns_device_upstream_snapshot` instead (#923) | Miss = `Default` (system-wide upstream). The device-keyed map is what lets a *roaming* device's queries follow its tunnel binding — its transport peer is the relay loopback, useless as a routing key. Entries exist only while the bound tunnel is not `Down` (down ⇒ soft fallback to `Default`, mirroring `handle_tunnel_down` on the LAN path; kill-switch #235 will gate this per device). Rebuilt by the routing service at every entry point that changes its inputs — rule change, default-policy change, tunnel up/down, DNS-override change, reconcile |
| **0.4 — reverse PTR** | Private-range `in-addr.arpa` PTR answered locally: hostname from the reverse index, else authoritative NXDOMAIN + synthetic SOA | RFC 6303; a conditional-forwarding rule on the reverse name wins. Returns `Authoritative` / `AuthoritativeNxdomain` |
| **0.5 — authoritative** | `AuthoritativeView::lookup` — answers directly, sets AA bit | Bypasses cache and filter entirely; returns `DnsQueryResult::Authoritative` |
| **0.6 — conditional forwarding rule match** | `AuthoritativeView::match_forwarding_rule` — selects per-domain upstream | Captured before the cache check; forwarding fires at step 3 if filter passes |
| **1 — cache** | Per-`UpstreamId` response cache | Tunnel and LAN devices have separate cache namespaces |
| **2 — filter** | `DnsFilterService::check` — block / rewrite / pass | Applies even to conditionally-forwarded domains |
| **2.5 — rate limit** | Per-client-IP token bucket, checked **only here** — right before a query leaves for an upstream | `rate_limit_per_second == 0` disables. Local answers (authoritative, reverse PTR, cache hit, filter block/rewrite) returned above and are exempt; the limiter exists to protect upstreams |
| **3 — forward** | Conditional upstream (if matched at 0.6 and filter passed), otherwise default or tunnel upstream | `forward_via_conditional` binds an undeviced socket; `forward_via_tunnel` uses `SO_BINDTODEVICE` |

Authoritative answers fully short-circuit the pipeline (no cache store, no filter). The rate limiter guards only the upstream-bound path (step 2.5): its purpose is to protect upstream resolvers, so a burst of locally-answerable queries — e.g. a mesh node's RFC1918 PTR storm — is answered every time and never REFUSED, which is what stops such a client's REFUSED-driven retry loop from self-amplifying. CNAME handling: for A/AAAA queries on a domain that has only a CNAME record in the view, the CNAME goes into the answer section and the target A/AAAA record (if also in the view) goes into the additional section.

### Event-driven rebuild

`DnsLocalServiceImpl` publishes `WardnetEvent::DnsLocalChanged` after every mutation:
- Zone mutations set `domain: None` — triggers a full view rebuild, no per-domain eviction.
- Record and forwarding-rule mutations set `domain: Some(domain)` — triggers a view rebuild **and** evicts that domain from the DNS response cache.

`DnsRunner` handles `DnsLocalChanged` by calling `DnsServer::update_authoritative_view` (atomic ArcSwap swap) and, if `domain` is `Some`, `DnsServer::invalidate_domain`.

### Background runners call auth-gated services, never repositories

Background runners (`DnsRunner`, `DnsFilterRunner`, `DnsQueryLogRunner`,
`DbMaintenanceRunner`, `DhcpLanRunner`) hold `Arc<dyn *Service>` trait
objects, **not** repository handles. Each runs its service calls under an
admin auth context:

```rust
let admin_ctx = AuthContext::Admin { admin_id: Uuid::nil() };
auth_context::with_context(admin_ctx.clone(), service.some_method()).await
```

This keeps the service layer the single auth-and-events chokepoint:
`DnsLocalService::upsert_record`, for example, owns `DnsLocalChanged`
emission, so the DHCP `.lan` runner can never write a record without
triggering the authoritative-view rebuild. The `Services` struct does **not**
expose `dns_local_repo`; `DbMaintenanceRunner` takes a thin
`MaintenanceService` rather than `MaintenanceRepository`. Reaching for a
`Arc<dyn *Repository>` inside a runner is a layering violation.

## DDNS subsystem (issue #527 / #521 umbrella)

Keeps the Pi's public A record current and (in later commits) handles ACME DNS-01 TXT records. Lives entirely in `wardnetd-services/src/ddns/`.

### Shape

```
DdnsUpdateRunner  ──(admin auth ctx)──▶  DdnsService  ──▶  DnsProvider
 (5-min tick)                            (auth-gated)       (bridge | cloudflare)
```

`DdnsUpdateRunner` holds `Arc<dyn DdnsService>` and calls it under an admin auth context — it never touches repositories or providers directly. `DdnsService` is the auth-and-persistence chokepoint: every method opens with `auth_context::require_admin()`.

### Provider abstraction (`provider.rs`)

`DnsProvider` trait with three methods: `upsert_a(ip)`, `set_txt(name, value)`, `delete_txt(name)`. Two impls:

| Impl | File | Auth |
|---|---|---|
| `BridgeProvider` | `bridge.rs` | Ed25519-signed requests (seed → `SigningKey`); bearer token in header; PoW-based registration via `register_install` |
| `CloudflareProvider` | `cloudflare.rs` | Per-request Bearer token; list-then-create/update against CF v4 API |

Providers are **rebuilt per call** from stored config + secrets (`build_provider()`). This is intentional: reads are cheap at the 5-minute cadence, and rebuilding means a provider switch takes effect without any cache-invalidation plumbing. All providers share one pooled `reqwest::Client`.

### Storage split

| Kind | Location | Keys |
|---|---|---|
| Non-secret config | `system_config` table | `ddns_provider`, `ddns_install_id`, `ddns_subdomain`, `ddns_region`, `ddns_bridge_base_url`, `ddns_last_public_ip`, `ddns_domain`, `ddns_cf_zone_id` |
| Secrets | `SecretStore` | `ddns/bridge/signing_key` (32-byte Ed25519 seed), `ddns/bridge/bearer_token`, `ddns/cloudflare/api_token` |

### Supporting modules

| Module | Purpose |
|---|---|
| `region.rs` | Built-in region catalog (`REGION_CATALOG`); `select_best` probes all regions concurrently and picks lowest latency |
| `public_ip.rs` | WAN public-IP discovery; rejects non-global IPv4 (RFC 1918, loopback, link-local); tries multiple echo endpoints in order |
| `runner.rs` | `DdnsUpdateRunner` — idle-until-configured 5-min tick; follows the runner contract (accepts `&tracing::Span`, instruments spawn) |

### Service methods

| Method | Notes |
|---|---|
| `register_with_bridge(name)` | Probes regions, calls `register_install` (PoW), persists secrets first then config, returns `DdnsRegistration{subdomain, region}` |
| `check_name_available(name)` | Probes best region, asks bridge; used by wizard |
| `refresh_public_ip()` | Discovers WAN IP, short-circuits if unchanged, calls `provider.upsert_a(ip)` |
| `status()` | Reads provider, FQDN, and last-published IP from config; returns `DdnsStatus` |

## Daemon-owned TLS (issue #528 / #521 umbrella)

`wardnetd` terminates TLS itself — no Caddy. ACME DNS-01 issuance reuses the
DDNS providers (via `DdnsService::set_acme_challenge` / `clear_acme_challenge`)
to publish `_acme-challenge` TXT records.

### Shape

```text
TlsRenewalRunner ─(admin ctx)─▶ TlsService ─▶ acme (instant-acme) ─▶ DdnsService.set_acme_challenge
 (12h tick)                     (auth-gated)  └▶ CertActivator.activate (hot-swap :443)
```

`TlsRenewalRunner` holds `Arc<dyn TlsService>` and calls it under an admin
context — never an ACME client, provider, or repository directly (runner
contract). `TlsService::ensure_certificate()` is one idempotent
issue-if-missing-or-renew-if-<30d method; inert (`TlsStatus::NotConfigured`)
when no FQDN is active, so the runner is idle until DDNS is configured. The
wizard (C9) and Settings (C10) call the same method. Cert + key (and the ACME
account credentials) are read/written **only** through the `SecretStore`
abstraction — never direct filesystem access.

### Always-bound `:443` / 503-until-provisioned serving

The `:443` listener is **always bound**, seeded at boot from the stored real
cert if present, else from a throwaway rcgen **placeholder** self-signed cert. A
shared `provisioned: Arc<AtomicBool>` (default `false` for the placeholder) gates
a **503 guard layer** on the `:443` app: every route returns
`503 "TLS not provisioned"` until a real cert loads, so the admin API is never
served under the untrusted placeholder. Pre-provisioning, the operator uses
`:7411` plain HTTP (unguarded). `:80` 308-redirects to HTTPS. The listener is
constant — no supervisor, no mid-run listener start.

### `CertActivator` abstraction boundary

The serving stack (`axum-server` + `RustlsConfig`) lives in `wardnetd`, not in
`wardnetd-services`. The `CertActivator` trait (defined in `wardnetd-services`,
implemented by `wardnetd::tls_server::CertActivatorImpl`) is the seam:
`activate(chain, key)` calls `RustlsConfig::reload_from_pem` (lock-free in-memory
swap) and flips the `provisioned` flag. It is injected via `Backends` so the
TLS service can swap the live cert without the services crate depending on the
serving stack. The aws-lc-rs crypto provider is installed once in `main` (both
ring + aws-lc-rs are in the tree → rustls can't auto-pick).

| Module (`wardnetd-services/src/tls/`) | Purpose |
|---|---|
| `mod.rs` | `TlsService` trait + impl, `CertActivator` trait, `TlsStatus`, `load_stored_cert` |
| `acme.rs` | instant-acme DNS-01 orchestration; CSR/leaf key via rcgen; `parse_not_after` (x509-parser) |
| `runner.rs` | `TlsRenewalRunner` — idle-until-configured 12h tick; follows the runner contract |

## Bridge service (issue #435)

Rust / Axum / PostgreSQL microservice for DDNS registration and ACME DNS-01 credential proxying. Deployed as a single binary behind a transparent L4 proxy (nginx + PROXY protocol v1); the bridge **terminates TLS for its own FQDN itself** via ACME HTTP-01 and passes tenant traffic through to the home-Pi tunnels — no Caddy (see `docs/adr/0007-bridge-self-terminated-tls.md`). Each bridge instance owns one region, keyed by its inforge region slug (e.g. `use1`).

### Security invariants

These rules are hard requirements — violating any of them opens a specific attack vector.

| Rule | What it prevents | Where it lives |
|---|---|---|
| **Default to loopback (`127.0.0.1:8080`)** | Exposes unauthenticated endpoints on every interface if set to `0.0.0.0` | `config.rs` `LISTEN_ADDR` default |
| **X-Forwarded-For only trusted from loopback peers** | IP spoofing in rate-limit and challenge-binding checks from direct connections | `api/challenge.rs` `client_ip()` |
| **Canonical payload includes `path_and_query`** | Attacker replaces query params without invalidating signature if only `path` is covered | `auth/middleware.rs` `auth_layer` |
| **Challenge IP binding** | A challenge solved by attacker on their IP cannot be replayed from victim's IP | `api/register.rs` |
| **Name uniqueness check BEFORE burning challenge** | Avoids wasting the caller's PoW work on a taken name | `api/register.rs` `register_install` |
| **In-memory replay cache** | Replays of valid signed requests within the ±60 s timestamp window | `replay_cache.rs`, wired in `auth_layer` |
| **Reject private/reserved IPv4 ranges** | SSRF via DNS A record pointing at RFC 1918 / loopback addresses | `api/ip.rs` `is_reserved_ipv4()` |
| **SHA-256(bearer_token) stored** | Bearer token never exposed at rest even if DB is compromised | `repository/install.rs` `token_hash` column |
| **`pub_key_bytes: [u8; 32]` on `Install`** | Avoids per-request base64 decode + allocation; key decoded once at DB row load | `repository/install.rs` `InstallRow::into_install` |

### Rate limits

| Limit | Scope | Endpoint |
|---|---|---|
| 3 registrations / IP / 24 h | IP address | `POST /v1/register` |
| 20 challenges / IP / hour | IP address | `GET /v1/register/challenge` |

### Auth model

Every request to `/v1/installs/*` must carry:
- `Authorization: Bearer <token>`
- `X-Wardnet-Timestamp: <unix_ts>` — rejected if `|now - ts| > 60 s`
- `X-Wardnet-Signature: <base64>` — Ed25519 over `"<METHOD>\n<path_and_query>\n<ts>\n<hex-sha256(body)>"`

Unauthenticated endpoints (`GET /v1/health`, `GET /v1/register/challenge`, `POST /v1/register`, `GET /v1/names/{name}/available`) never trigger a DB token lookup regardless of whether an `Authorization` header is present.

### Shared validation (`api/validation.rs`)

`RESERVED_NAMES`, `is_valid_name()`, `validate_name()`, and `validate_public_key()` live in one place. Do **not** duplicate them in handler modules — the name availability check and the registration handler must apply identical rules.

## Watchdog + health subsystem (issue #214)

A **three-layer** recovery model. `Restart=always` only catches a daemon that
*exits*; a livelocked or deadlocked daemon keeps systemd happy and never
recovers. The layers escalate from "report" to "restart the service" to "reboot
the host". See [0014-watchdog-and-health.md](../docs/adr/0014-watchdog-and-health.md).

### Layer 1 — `HealthMonitor` (`wardnetd-services/src/health/`)

`HealthCheck` is a `Send + Sync`, `#[async_trait]` trait (`name() -> &'static str`
+ `check() -> CheckOutcome`). `HealthMonitor` holds `Vec<Arc<dyn HealthCheck>>`
plus an `arc_swap::ArcSwap<HealthSnapshot>` (the same lock-free pattern as the
local-DNS `AuthoritativeView`). `refresh()` runs every check **concurrently**
(`futures::future::join_all`), each wrapped in a `tokio::time::timeout`
(`health.check_timeout_secs`, default 2 s) so a hung probe becomes
`Down { detail: "timeout" }` rather than stalling the cycle. A per-check
**consecutive-failure debounce** flips a component to DOWN only after
`failure_threshold` (default 3) straight failures and recovers it on the first
success; overall is DOWN if any component is DOWN. `HealthMonitorRunner`
(`wardnetd/src/health_runner.rs`, child span `health`) drives `refresh()` every
`health.refresh_interval_secs` (default 5 s). The four probes are registered in
`main.rs`: `DbHealthCheck` (`SELECT 1` via `MaintenanceRepository::ping`),
`LivenessHealthCheck`, `DnsServerHealthCheck`, `DhcpServerHealthCheck`. The
DNS/DHCP probes are **desired-vs-actual**, not raw `is_running()`: each reads
its configured `enabled` flag through the auth-gated service under a nil-admin
`auth_context` (like the runners) and reports DOWN only when
`enabled && !is_running()` — never for a deliberately toggled-off service,
which would otherwise make the soft watchdog restart-loop a healthy daemon. The
mock registers only liveness + DB (its noop DNS/DHCP servers never bind).
`GET /health` (`wardnetd-api/src/api/health.rs`,
unauthenticated, `security(())`) maps overall UP→200 / DOWN→503 with a
per-component body, reading the snapshot from `AppState::health_monitor`.

### Layer 2 — soft watchdog (`wardnetd/src/watchdog/soft.rs`, span `watchdog{layer=soft}`)

`SoftWatchdogRunner` ticks at `WATCHDOG_USEC/2` and sends `sd_notify(WATCHDOG=1)`
**only** when overall health is UP *and* the snapshot is fresh (younger than
`2 × refresh_interval`); otherwise it withholds the ping and systemd's
`WatchdogSec=15` restarts the *service*. The gating decision is the testable
`should_ping(&snapshot, staleness)` helper. `sd_notify` is behind a `Notifier`
trait (`SdNotifier` real impl; tests inject a fake) so the policy is unit-tested
without a real `NOTIFY_SOCKET`. `main.rs` also sends `READY=1` via the same
notifier once all listeners bind — which is why the unit is `Type=notify`.

### Layer 3 — hard watchdog (`wardnetd/src/watchdog/hard.rs`, span `watchdog{layer=hard}`)

`HardwareWatchdogRunner` pets `/dev/watchdog` every `watchdog.pet_interval_secs`
**ungated** — it never consults health. This is the backstop for a *total*
freeze where even the health loop can't run. The device is behind a
`WatchdogOps` trait (`wardnetd-services/src/system/watchdog_ops.rs`) wired onto
`Backends` like `SystemPowerOps`/`GarpOps`: `LinuxWatchdog`
(`wardnetd/src/system/linux_watchdog.rs`, `WDIOC_SETTIMEOUT` ioctl + write-a-byte
keep-alive + `'V'` magic-close disarm) in production, `NoopWatchdog` in the mock.
`shutdown()` disarms first so a clean stop never reboots. **Invariant: the
hardware pet is never health-gated** — that is the whole point of having a
third layer below the health-gated soft restart.

## Network-Zone enforcement subsystem (issue #736)

Phase 1 / CI-2 of epic #244. Turns a device's [`NetworkZone`] into nftables
rules so a zone bites on a flat shared subnet, live-reloaded with no restart.
Design rationale, verdict choices, and the default-policy caveat are in
[`docs/adr/0019-network-zone-enforcement.md`](../docs/adr/0019-network-zone-enforcement.md);
domain terms in [`CONTEXT.md`](../CONTEXT.md) ("Zone packet enforcement").

### Shape

`ZoneEnforcementService` (`wardnetd-services/src/zone_enforcement/`) is a
**separate** event-bus subscriber from the routing service — driven by
`ZoneEnforcementListener` (`wardnetd/src/zone_enforcement_listener.rs`, span
`zone_enforcement_listener{}`). It shares the `FirewallManager` + `PolicyRouter`
backends with the routing service (they cooperate on the one `wardnet` nftables
table + per-device conntrack) but owns its own rules. The two listeners are
independent: routing manages kernel policy routing, the enforcer manages packet
gating, and neither blocks the other.

### Two gates, keyed by device IP (comment UDATA, restart-survivable)

- **Egress gate** — forward-chain `drop` (`wardnet:zone:egress:<ip>`). Tunnel
  forbidden ⇒ drop `oifname wg_ward*` (a `meta oifname` + bitwise-mask + compare
  prefix match, so it matches any tunnel index without enumerating interfaces).
  Direct forbidden ⇒ drop `oifname <lan_interface>`. A packet function of the
  zone's `allowed_targets` alone — never the device's current routing target.
- **Admin-UI gate** — a new `input` base chain (accept policy) carries
  reject-with-tcp-reset rules (`wardnet:zone:adminui:<ip>`) for device→Pi :443
  and :7411 when `admin_ui_reachable = false`; DNS/DHCP pass untouched. "Connection
  refused" ⇒ TCP reset, not a silent drop. `init_wardnet_table` /
  `flush_wardnet_table` gained this `input` chain.

### Live reload + reconcile

The listener maps events to per-device recomputes, each followed by a conntrack
flush so open flows re-evaluate at once: `NetworkZoneChanged`→`apply_zone`,
`DeviceZoneChanged`/`DeviceDiscovered`→`apply_device`, `DeviceIpChanged`→
`handle_ip_change` (re-key old→new IP), `DeviceGone`→`remove_device`. Startup
`reconcile` re-applies every device's rules and drops orphaned rules for IPs no
longer backed by a device, and runs **after** `RoutingService::reconcile` (which
(re)creates + flushes the shared table).

### Closing the default-policy caveat (event + callback)

`RoutingService::set_default_policy` now emits
`WardnetEvent::DefaultPolicyChanged` (RoutingServiceImpl gained an
`EventPublisher` dep). The enforcer subscribes and, for each `Default`-ruled
device whose zone forbids the newly-resolved kind, calls back into
`RoutingService::apply_rule_for_device(id, Direct)` to unbind it — the one edge
the #735 write-time gate cannot catch. The routing engine stays zone-free; the
tradeoff (stored `Default` vs applied `Direct` divergence, re-derived each boot)
is recorded in the ADR.

**Honest limit:** same-subnet peer↔peer traffic is not affected — the daemon
never sees it on a flat L2 segment (the AP's job, or the isolate-members rung
#737).

### New-device quarantine (issue #738)

An off-by-default `quarantine_new_devices` `system_config` toggle (owned by
`NetworkZoneService`, exposed at `GET/PUT /api/network/quarantine-new-devices`).
It is **notification-only**: placement is unchanged — every new device already
lands in the `is_default_for_new` zone unconditionally (`DeviceDiscoveryServiceImpl::insert_new_device`),
and enforcement already reacts to `DeviceDiscovered`. When the toggle is on, the
**truly-first-ever** discovery path (only `insert_new_device`, never the reappear
path — so idempotent by construction) publishes a dedicated
`WardnetEvent::NewDeviceQuarantined`; `PushService` turns it into an admin
"approve this device" push. A real quarantine is achieved by pointing
`is_default_for_new` at a restrictive Guest zone (the #735 lever). Approve =
existing `PUT /api/devices/{id}/zone`. Note `DeviceDiscovered` is **not** a
valid first-ever signal (it also fires on every reconnect).

[`NetworkZone`]: ../source/daemon/crates/wardnet-common/src/network_zone.rs
