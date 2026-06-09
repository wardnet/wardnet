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

### Two-tier storage — mirrors the `tunnel_metrics` pattern

| Table | Granularity | Retention | Key |
|---|---|---|---|
| `stats_intraday` | 1-minute buckets (`bucket_ts` = Unix seconds truncated to minute) | 48 h | `(metric, labels, bucket_ts)` |
| `stats_daily` | 1-day rollup (`day` = `YYYY-MM-DD` UTC) | 13 months | `(metric, labels, day)` |

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

`StatsService::query` supports three granularities: `Minute` (raw intraday rows), `Hour` (server-side aggregation of intraday), and `Day` (daily rollup table).

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

### Resolution pipeline (per-query order)

| Step | What happens | Notes |
|---|---|---|
| **0 — upstream selection** | Client IP → `UpstreamId` from routing snapshot | Miss = `Default` (system-wide upstream) |
| **0.5 — authoritative** | `AuthoritativeView::lookup` — answers directly, sets AA bit | Bypasses cache and filter entirely; returns `DnsQueryResult::Authoritative` |
| **0.6 — conditional forwarding rule match** | `AuthoritativeView::match_forwarding_rule` — selects per-domain upstream | Captured before the cache check; forwarding fires at step 3 if filter passes |
| **1 — cache** | Per-`UpstreamId` response cache | Tunnel and LAN devices have separate cache namespaces |
| **2 — filter** | `DnsFilterService::check` — block / rewrite / pass | Applies even to conditionally-forwarded domains |
| **3 — forward** | Conditional upstream (if matched at 0.6 and filter passed), otherwise default or tunnel upstream | `forward_via_conditional` binds an undeviced socket; `forward_via_tunnel` uses `SO_BINDTODEVICE` |

Authoritative answers fully short-circuit the pipeline (no cache store, no filter). CNAME handling: for A/AAAA queries on a domain that has only a CNAME record in the view, the CNAME goes into the answer section and the target A/AAAA record (if also in the view) goes into the additional section.

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

Rust / Axum / PostgreSQL microservice for DDNS registration and ACME DNS-01 credential proxying. Deployed as a single binary behind a transparent L4 proxy (nginx + PROXY protocol v1); the bridge **terminates TLS for its own FQDN itself** via ACME HTTP-01 and passes tenant traffic through to the home-Pi tunnels — no Caddy (see `docs/adr-bridge-self-terminated-tls.md`). Each bridge instance owns one region, keyed by its inforge region slug (e.g. `use1`).

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
