# Project Structure

```
source/
├── daemon/                          # Rust workspace (Cargo.toml at this level)
│   └── crates/
│       ├── wardnet-common/          # Shared types: Device, Tunnel, RoutingTarget, DHCP, VPN Provider types, Events, API DTOs, Config
│       ├── wardnetd-data/           # Data access layer
│       │   ├── src/
│       │   │   ├── repository/      # Trait definitions (AdminRepository, DeviceRepository, TunnelRepository, DhcpRepository, DnsRepository, SystemConfigRepository, etc.)
│       │   │   │   └── sqlite/      # SQLite implementations of all repository traits
│       │   │   ├── database_dumper/ # DatabaseDumper trait + SqliteDumper (VACUUM INTO snapshot + atomic rename restore)
│       │   │   ├── bootstrap/       # Admin account initialization (first-run setup)
│       │   │   ├── db/              # SQLite pool init (WAL mode, migrations)
│       │   │   ├── secret_store/    # SecretStore trait + FileSecretStore + NullSecretStore (provider-backed vault)
│       │   │   └── oui/             # MAC OUI prefix lookup (full IEEE MA-L database, ~39K entries)
│       │   └── migrations/          # SQLite migration files (sqlx)
│       ├── wardnetd-services/       # Business logic layer
│       │   └── src/
│       │       ├── auth/            # AuthService: login, session management, API key auth
│       │       ├── device/          # DeviceService + DeviceDiscoveryService
│       │       │   └── discovery/   # Background ARP scan + observation loop
│       │       ├── dhcp/            # DhcpService + DhcpRunner lifecycle
│       │       ├── dns/             # DnsService + DNS filter + blocklist downloader
│       │       ├── tunnel/          # TunnelService: VPN tunnel lifecycle management (+ KeyStoreAdapter over SecretStore)
│       │       ├── routing/         # RoutingService: policy rules, per-device routing
│       │       ├── vpn/             # VpnProviderService: provider credentials, server list
│       │       ├── system/          # SystemService: host CPU/memory, uptime, daemon restart
│       │       ├── backup/          # BackupService + AgeArchiver + cleanup runner
│       │       ├── logging/         # LogService, log streaming, error notification
│       │       ├── event/           # BroadcastEventBus (EventPublisher implementation)
│       │       ├── auth_context/    # Task-local auth context (require_admin, with_context)
│       │       ├── request_context/ # Request-scoped context
│       │       ├── command/         # CommandExecutor trait (shell command abstraction)
│       │       └── version/         # Compile-time version info
│       ├── wardnetd-api/            # HTTP API layer (Axum)
│       │   └── src/
│       │       ├── api/             # Endpoint handlers (auth, devices, dhcp, dns, info, setup, system, tunnels, providers, backup, update)
│       │       │   └── logs_ws.rs   # WebSocket log streaming endpoint
│       │       ├── middleware.rs    # AuthContextLayer, RequestContextLayer, CORS, tracing
│       │       ├── state.rs         # AppState (holds Arc<dyn Service> trait objects + EventPublisher)
│       │       └── web.rs           # rust-embed static file serving (fallback to index.html)
│       ├── wardnetd/                # Daemon binary: Linux-specific backends + startup orchestration
│       │   ├── build.rs             # Build script (version, OUI database generation)
│       │   ├── data/oui.csv         # IEEE MA-L OUI database (~39K entries)
│       │   └── src/
│       │       ├── main.rs          # Entry point: wires real backends, calls init_services(), starts axum server
│       │       ├── tunnel_interface_wireguard.rs  # WireGuard impl (Linux kernel + macOS userspace)
│       │       ├── firewall_nftables.rs           # nftables impl via CommandExecutor
│       │       ├── policy_router_netlink.rs        # Netlink routing policies (ip rule, ip route)
│       │       ├── packet_capture_pnet.rs          # pnet raw socket packet capture
│       │       ├── hostname_resolver.rs            # System hostname resolution
│       │       ├── device_detector.rs              # DeviceDetector: spawns capture + observation loop
│       │       ├── tunnel_monitor.rs               # Background health check + stats collection
│       │       ├── tunnel_idle.rs                  # Idle tunnel teardown on DeviceGone
│       │       ├── routing_listener.rs             # Background event→routing dispatcher
│       │       ├── route_monitor.rs                # Kernel route table observation
│       │       ├── metrics_collector.rs            # OpenTelemetry metrics export
│       │       ├── profiling.rs                    # Pyroscope profiling integration
│       │       ├── dhcp/                           # DHCP server (dhcproto)
│       │       └── dns/                            # DNS server (hickory)
│       ├── wardnetd-mock/           # Local dev binary: full API with no-op Linux backends
│       │   └── src/
│       │       ├── main.rs          # Entry point: on-disk/in-memory SQLite + demo data seed + fake events
│       │       ├── backends/        # No-op impls (noop_tunnel, noop_routing, noop_dhcp, noop_dns, noop_device)
│       │       ├── seed.rs          # Demo data seeder (writes directly via repositories)
│       │       └── events.rs        # Periodic fake event emitter for UI testing
│       ├── wctl/                    # CLI tool (clap: status, devices, tunnels, update subcommands — placeholders)
│       └── wardnet-test-agent/      # Pi-side kernel state inspector for system tests
│           └── src/
│               ├── main.rs          # HTTP server (port 3001) exposing ip rule, nft, wg show, ip link
│               ├── models.rs        # IpRule, NftRulesResponse, WgShowResponse, LinkShowResponse
│               ├── fixtures.rs      # Test fixture generation (WireGuard configs, keys)
│               ├── container.rs     # Container exec abstraction
│               └── kernel/          # Kernel state query/parse modules
├── sdk/
│   └── wardnet-js/                  # @wardnet/js — TypeScript SDK (browser + Node)
│       └── src/
│           ├── client.ts            # WardnetClient base HTTP client
│           ├── services/            # AuthService, DeviceService, TunnelService, ProviderService, SystemService, SetupService, InfoService, BackupService, UpdateService
│           └── types/               # TypeScript type definitions (mirrors daemon API)
├── web-ui/                          # React + TypeScript frontend
│   └── src/
│       ├── components/
│       │   ├── core/ui/             # shadcn/ui components (Button, Card, Sheet, Dialog, Select, Tabs, Switch, etc.)
│       │   ├── compound/            # Compositions (Sidebar, MobileMenu, PageHeader, DeviceIcon, ConnectionStatus, Logo, CountryCombobox, RoutingSelector, ApiErrorAlert)
│       │   ├── features/            # Use-case components (DeviceList, TunnelList, LoginForm, BackupCard, RestartProgressDialog, UpdateCard)
│       │   └── layouts/             # Page shells (AppLayout, AuthLayout)
│       ├── hooks/                   # React hooks bridging SDK ↔ React (useAuth, useTheme, useDevices, useTunnels, useProviders, useSystemStatus, useBackup, useRestart, useUpdate, …)
│       ├── stores/                  # Zustand stores (authStore)
│       ├── pages/                   # Route pages (Dashboard, Devices, Tunnels, Settings, Login, Setup, MyDevice)
│       └── lib/                     # SDK instance (sdk.ts), utilities (cn, formatBytes, formatUptime, timeAgo)
└── site/                            # Public documentation + marketing site (Vite + React)
    ├── content/docs/                # Markdown articles served by DocsArticle.tsx
    ├── content/docs.yml             # Topic catalogue driving /docs
    └── public/docs/                 # Screenshots and other static assets referenced from markdown
```
