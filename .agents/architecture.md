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
