# Project Structure

```
source/
├── daemon/                          # Rust workspace (Cargo.toml at this level)
│   └── crates/
│       ├── wardnet-common/          # Shared types: Device, Tunnel, RoutingTarget, DHCP, VPN Provider types, Events, API DTOs, Config, Stats query/response types
│       ├── wardnetd-data/           # Data access layer
│       │   ├── src/
│       │   │   ├── repository/      # Trait definitions (AdminRepository, DeviceRepository, TunnelRepository, DhcpRepository, DnsRepository, SystemConfigRepository, StatsRepository, etc.)
│       │   │   │   └── sqlite/      # SQLite implementations of all repository traits (including SqliteStatsRepository)
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
│       │       ├── dns/             # DnsService, DnsRunner, cache, AuthoritativeView, log sink, blocklist downloader
│       │       ├── dns_filter/      # DnsFilterService + DnsFilterRunner: blocklist CRUD, rebuild pipeline
│       │       ├── dns_local/       # DnsLocalService: zones, custom records, forwarding rules CRUD; emits DnsLocalChanged
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
│       │       ├── stats/           # Generic pre-aggregating stats subsystem (StatsBuffer, Meter, StatsService, StatsFlushRunner)
│       │       └── version/         # Compile-time version info
│       ├── wardnetd-api/            # HTTP API layer (Axum)
│       │   └── src/
│       │       ├── api/             # Endpoint handlers (auth, devices, dhcp, dns, info, setup, stats, system, tunnels, providers, backup, update)
│       │       │   ├── stats.rs     # GET /api/stats (time-series query) + GET /api/stats/top (top-N)
│       │       │   ├── tunnels.rs   # includes POST /api/tunnels/{id}/rebuild → TunnelService::rebuild()
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
│           ├── services/            # AuthService, DeviceService, TunnelService (includes rebuild()), ProviderService,
│           │                        #   SystemService, SetupService, InfoService, BackupService, UpdateService
│           └── types/               # TypeScript type definitions (mirrors daemon API);
│                                    #   RebuildTunnelResponse added for POST /api/tunnels/{id}/rebuild
├── admin-site/                      # Full desktop admin UI (served at /admin/)
│   └── src/
│       ├── components/
│       │   ├── core/ui/             # shadcn/ui components (Button, Card, Sheet, Dialog, Select, Tabs, Switch, etc.)
│       │   ├── compound/            # Compositions (Sidebar, MobileMenu, PageHeader, DeviceIcon, ConnectionStatus, TunnelCard, TunnelDetail, etc.)
│       │   ├── features/            # Use-case components (DeviceList, TunnelList, BackupCard, RestartProgressDialog, UpdateCard)
│       │   └── layouts/             # Page shells (AppLayout, AuthLayout)
│       ├── pages/                   # Route pages (Dashboard, Devices, Tunnels, Settings, Login, Setup, MyDevice)
│       └── lib/                     # App-local utilities
├── user-app/                        # PLANNED (issue #438): user PWA — self-service for non-admin household members; served at /
├── admin-app/                       # Admin mobile PWA (issue #439) — daily operational tasks; served at /admin-app/
│   └── src/
│       ├── components/              # Mobile-specific components (Header, TabBar, BusyOverlay, ConfirmDialog, DeviceRoutingSheet, ConnectionBanner, etc.)
│       ├── context/                 # OnlineStatusContext — exposes `showingLastKnownState` flag for offline overlay
│       ├── features/                # Dashboard feature cards (DevicesCard, TunnelsCard, DnsCard, DaemonStrip, StatusCard)
│       ├── hooks/                   # App-local hooks (useBiometric)
│       ├── layouts/                 # AppLayout, AuthLayout
│       └── pages/                   # Route pages: Dashboard, Devices, Tunnels, Dns, System, Login
│                                    #   Tunnels page: summary header card (sparkline + combined throughput) + per-tunnel
│                                    #   cards with status pill, rebuild button, and live throughput; uses
│                                    #   useCombinedTunnelStats and useRebuildTunnel from @wardnet/web.
│                                    #   Devices page: showingLastKnownState offline overlay; loading skeleton
│                                    #   gated on both devices + policy loaded.
├── web/                             # @wardnet/web — shared React hooks, utilities, and UI primitives for all app surfaces
│   └── src/
│       ├── primitives/              # UI primitives (Button, Card, Modal, Combobox, Drawer, Select, Toggle, Sparkline, etc.)
│       ├── hooks/                   # All shared TanStack Query hooks: useAuth, useDevices, useTunnels, useStats,
│       │                            #   useTunnelStats, useCombinedTunnelStats, useProviders, useSystemStatus,
│       │                            #   useDns, useDhcp, useBackup, useUpdate, useSetup, useDaemonStatus, etc.
│       │                            #   useCombinedTunnelStats — aggregates tunnel.bytes.tx + tunnel.bytes.rx
│       │                            #   over 1-hour window (1-min buckets) for sparkline + combined throughput.
│       │                            #   useRebuildTunnel — mutation wrapping POST /api/tunnels/{id}/rebuild.
│       ├── components/              # LoginForm, JobProgressDescription
│       ├── stores/                  # Zustand stores (authStore)
│       └── lib/                     # SDK singletons (sdk.ts), utility functions, country helpers, logger
├── styles/                          # @wardnet/styles — CSS design tokens + typed token constants
│   ├── styles.css                   # Tailwind base + CSS custom properties (--accent, --ink-*, --radius-*, etc.)
│   └── src/tokens.ts                # Typed brand/status/radius/density/font token constants
└── site/                            # Public documentation + marketing site (Vite + React)
    ├── content/docs/                # Markdown articles served by DocsArticle.tsx
    ├── content/docs.yml             # Topic catalogue driving /docs
    └── public/docs/                 # Screenshots and other static assets referenced from markdown
```
