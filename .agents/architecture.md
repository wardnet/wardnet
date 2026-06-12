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

## Bridge service (issue #435)

Rust / Axum / SQLite microservice for DDNS registration and ACME DNS-01 credential proxying. Deployed as a single binary behind Caddy on the bridge VM. Each bridge instance owns one region (e.g. `us`, `eu`).

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
