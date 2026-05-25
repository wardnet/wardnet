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
- **Database-provider concerns live next to the repositories**: the `DatabaseDumper` trait + its SQLite impl live in `wardnetd-data/src/database_dumper.rs`, *not* in the backup module. A future non-SQLite provider ships its own dumper alongside its own repositories and the backup service picks it up through `RepositoryFactory::dumper()` with no service-layer changes.
- **Secret-store concerns are provider-owned too**: `SecretStore::backup_contents` / `restore_from_backup` live on the trait, so each provider (`FileSecretStore` today; `HashicorpVault`, `1Password`, etc. later) decides what travels with a bundle and what stays in the external service.
- **mDNS advertisement is a self-contained runner**: `wardnetd::mdns_advertiser::MdnsAdvertiser` (Linux production binary only) publishes `<hostname>.local.` so the setup wizard is reachable without a known IP. It is a runner, not a service — mirrors `HeartbeatRunner` / `RouteMonitor`, not a trait-backed service. The mock binary does not start it.

## Stats subsystem (issue #409)

A generic pre-aggregating stats subsystem is being built in phases. The data layer (PR 1) is complete; the services layer and frontend are pending.

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
| `dns.queries/{outcome}` | `{outcome}` | Per-outcome total; low cardinality |
| `dns.latency_ms/{outcome}` | `{outcome}` | Gauge per outcome |
| `dns.queries.by_domain/{blocked,domain}` | `{domain}` | Top-blocked domains; filtered to blocked only |
| `dns.queries.by_client/{client}` | `{client}` | Per-client query count |

Do **not** use a single `dns.queries` metric with `{client, domain, outcome}` labels — that cross product explodes row counts.

### Shared types (`wardnet-common/src/stats.rs`)

`StatsQuery`, `StatsTopQuery`, `StatsBucket`, `StatsSeriesPoint`, `StatsQueryResponse`, `StatsTopEntry`, `StatsTopResponse` — used by both the service/repository layer and the API/SDK.

### Repository interface (`wardnetd-data/src/repository/stats.rs`)

`StatsRepository` trait methods: `upsert_intraday`, `rollup_daily`, `trim_intraday`, `trim_daily`, `query_intraday`, `query_daily`, `top_n`. Accessed via `RepositoryFactory::stats()`.
