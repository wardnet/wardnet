# Wardnet Phase 1 (MVP) Implementation Plan

**Last updated:** April 12, 2026

---

## Context

Wardnet is a self-hosted network privacy gateway for Raspberry Pi that sits alongside an existing router. It encrypts traffic via WireGuard tunnels, provides per-device routing control, and prevents DNS leaks -- all managed from a single web UI. This plan covers Phase 1 (MVP): the foundational system that proves the core value proposition.

**Phase 1 scope:** Core daemon with WireGuard tunnel management, policy routing, device detection, DHCP server (gateway advertisement, static reservations, conflict detection), gateway resilience (hardware watchdog, GARP failover, graceful reboot/shutdown), VPN provider integration (pluggable architecture + NordVPN), built-in DNS server with network-wide ad blocking (replaces Pi-hole) and VPN DNS leak prevention, REST+WebSocket API with API key auth, web UI (wizard with DHCP onboarding + router MAC discovery, dashboard, device/tunnel management, guided provider setup, DNS/ad-blocking config, safe reboot/shutdown, DHCP panel), CLI tool (including reboot/shutdown commands), and installation packaging.

**Out of scope for Phase 1:** Temporary/scheduled routing, kill switch per-device configuration, mobile app.

### Progress Summary

| Milestone | Status | Description |
|-----------|--------|-------------|
| 1a: Scaffolding & Foundation | ✅ Done | Workspace, DB, auth, basic API |
| 1b: WireGuard Tunnel Management | ✅ Done | Tunnel CRUD, monitoring, lazy lifecycle |
| 1c: Device Detection | ✅ Done | ARP/IP sniffing, OUI lookup, departure tracking |
| 1d: Policy Routing Engine | ✅ Done | ip rule, nftables, event-driven routing, reconciliation |
| 1e: DHCP Server | ✅ Done | dhcproto server, leases, reservations, API |
| 1f: Gateway Resilience & Failover | Not started | GARP, watchdog, graceful shutdown |
| 1g: DNS Server & Ad Blocking | In progress | Built-in DNS, ad blocking (replaces Pi-hole), leak prevention — [detailed plan](milestone-1g-dns-server.md) |
| 1h: Web UI | In progress | Core layout done; detail views, DHCP panel, wizard remaining |
| 1i: CLI Tool (wctl) | In progress | Commands scaffolded; output formatting, reboot/shutdown remaining |
| 1j: Installation & Packaging | Not started | install.sh, systemd unit, release workflow |
| 1k: VPN Provider Integration | ✅ Done | NordVPN provider, pluggable architecture |
| 1l: Integration Testing | In progress | Test agent + 7 E2E suites done; failure/DHCP/GARP tests remaining |

**~695 unit/integration tests** across the workspace. 7 system test suites targeting real Pi deployment.

---

## 1. Project Structure

### Cargo Workspace

```
wardnet/
├── source/
│   ├── daemon/                       # Rust workspace root (Cargo.toml here)
│   │   └── crates/
│   │       ├── wardnetd/             # binary -- the daemon
│   │       │   ├── src/
│   │       │   │   ├── main.rs       # entry point (thin, wires dependencies)
│   │       │   │   ├── lib.rs        # crate root (re-exports modules)
│   │       │   │   ├── api/          # axum REST handlers + auth middleware
│   │       │   │   ├── service/      # business logic (traits + impls)
│   │       │   │   ├── repository/   # data access traits
│   │       │   │   │   └── sqlite/   # SQLite implementations
│   │       │   │   ├── dhcp/         # DHCP server (mod, runner, server + tests)
│   │       │   │   ├── config.rs     # TOML config loading
│   │       │   │   ├── db.rs         # SQLite pool + migrations
│   │       │   │   ├── error.rs      # AppError → IntoResponse
│   │       │   │   ├── state.rs      # AppState (service trait objects)
│   │       │   │   ├── event.rs      # EventPublisher trait + BroadcastEventBus
│   │       │   │   ├── keys.rs       # KeyStore trait + FileKeyStore
│   │       │   │   ├── tunnel_interface.rs         # TunnelInterface trait + types
│   │       │   │   ├── tunnel_interface_wireguard.rs  # WireGuard impl (Linux kernel + macOS userspace)
│   │       │   │   ├── tunnel_monitor.rs  # Background health/stats tasks
│   │       │   │   ├── tunnel_idle.rs     # Idle tunnel teardown watcher
│   │       │   │   ├── firewall.rs        # FirewallManager trait
│   │       │   │   ├── firewall_nftables.rs  # nftables impl (masquerade, DNS DNAT)
│   │       │   │   ├── policy_router.rs      # PolicyRouter trait
│   │       │   │   ├── policy_router_iproute.rs  # iproute2 impl (ip rule, ip route, sysctl)
│   │       │   │   ├── routing_listener.rs   # Background event→routing dispatcher
│   │       │   │   ├── device_detector.rs    # Background ARP/IP packet sniffer
│   │       │   │   ├── packet_capture.rs     # PacketCapture trait + types
│   │       │   │   ├── packet_capture_pnet.rs  # pnet impl (raw socket)
│   │       │   │   ├── hostname_resolver.rs  # HostnameResolver trait + impl
│   │       │   │   ├── oui.rs         # MAC OUI lookup (IEEE MA-L database)
│   │       │   │   ├── bootstrap.rs   # Daemon initialisation logic
│   │       │   │   ├── command.rs     # CommandExecutor trait (shell command abstraction)
│   │       │   │   ├── garp.rs        # GARP (Gratuitous ARP) failover sequences (future)
│   │       │   │   ├── watchdog.rs    # Hardware watchdog integration (future)
│   │       │   │   ├── shutdown.rs    # Graceful shutdown/reboot orchestration (future)
│   │       │   │   └── web.rs         # rust-embed static file serving
│   │       │   └── migrations/        # sqlx SQL migrations
│   │       ├── wctl/                  # binary -- CLI tool
│   │       │   └── src/main.rs        # clap subcommands (status, devices, tunnels)
│   │       ├── wardnet-types/         # library -- shared types
│   │       │   └── src/
│   │       │       ├── lib.rs
│   │       │       ├── device.rs
│   │       │       ├── tunnel.rs      # Tunnel, TunnelStatus, TunnelConfig
│   │       │       ├── routing.rs
│   │       │       ├── dhcp.rs        # DhcpConfig, DhcpLease, DhcpReservation
│   │       │       ├── api.rs         # request/response DTOs
│   │       │       ├── auth.rs        # Session, ApiKeyRecord, Role, AuthContext
│   │       │       ├── event.rs       # WardnetEvent enum
│   │       │       ├── vpn_provider.rs   # ProviderInfo, ServerInfo, credentials
│   │       │       └── wireguard_config.rs  # .conf parser + WgConfig types
│   │       └── wardnet-test-agent/    # binary -- Pi-side kernel state inspector for system tests
│   │           └── src/
│   │               ├── main.rs        # HTTP server exposing ip rule, nft, wg show, ip link
│   │               ├── models.rs      # IpRule, NftRulesResponse, WgShowResponse, etc.
│   │               ├── fixtures.rs    # Test fixture generation (WG configs, keys)
│   │               ├── container.rs   # Container exec abstraction
│   │               └── kernel/        # Kernel state query modules
│   ├── sdk/
│   │   └── wardnet-js/                  # @wardnet/js — TypeScript SDK (browser + Node)
│   │       └── src/
│   │           ├── client.ts            # WardnetClient base HTTP client
│   │           ├── services/            # AuthService, DeviceService, TunnelService, ProviderService, SystemService, SetupService, InfoService
│   │           └── types/               # TypeScript type definitions (mirrors daemon API)
│   ├── web-ui/                       # React + Vite project
│   │   └── src/
│   │       ├── components/
│   │       │   ├── core/ui/          # shadcn/ui components (Button, Card, Sheet, Dialog, Select, Tabs, Switch, etc.)
│   │       │   ├── compound/         # Compositions (Sidebar, MobileMenu, PageHeader, DeviceIcon, ConnectionStatus, Logo, CountryCombobox, RoutingSelector, ApiErrorAlert)
│   │       │   └── layouts/          # Page shells (AppLayout, AuthLayout)
│   │       ├── hooks/                # React hooks bridging SDK ↔ React (useAuth, useTheme, useDevices, useDevice, useMyDevice, useTunnels, useProviders, useSystemStatus, useDaemonStatus, useSetup, mutations)
│   │       ├── stores/               # Zustand stores (authStore)
│   │       ├── pages/                # Dashboard, Devices, Tunnels, Settings, Login, Setup, MyDevice
│   │       └── lib/                  # SDK instance, utilities (cn, formatBytes, formatUptime, timeAgo)
│   └── system-tests/                 # TypeScript E2E tests targeting real Pi deployment
│       └── src/
│           ├── helpers/              # client.ts, agent.ts (test-agent client), setup.ts
│           ├── runner.ts             # Test orchestrator
│           └── tests/                # 01-health, 02-tunnel-import, 03-device-detection, 04-device-routing, 05-traffic-routing, 06-multi-tunnel, 07-idle-teardown
├── .github/workflows/ci.yml
├── Makefile                          # Unified build: init, build, check, run-pi, system-test
├── implementation-docs/
├── AGENTS.md
└── README.md
```

> **Note:** Modules for dns/, garp/, watchdog/, and install/ will be added in later milestones. Routing, device detection, DHCP, tunnel management, event bus, and key storage are implemented.

### Crate Dependencies

- `wardnet-types`: serde, uuid, chrono, thiserror (no runtime deps -- pure data types)
- `wardnetd` depends on `wardnet-types` + tokio, axum, sqlx, wireguard-control, ipnetwork, tokio-util, pnet (device detection + GARP), dhcproto (DHCP server), reqwest (for VPN provider APIs), rust-embed, toml, tracing, argon2, async-trait
- `wctl` depends on `wardnet-types` + clap, reqwest, tabled, tokio
- `wardnet-test-agent`: axum, tokio, serde -- lightweight HTTP server exposing kernel state for system tests

### Web UI Stack

React 19 + TypeScript 5.9, Vite 7, Tailwind CSS 4, shadcn/ui (Radix), TanStack Query 5, Zustand 5, React Router 7, Lucide icons

### SDK Package

`@wardnet/js` — TypeScript SDK with WardnetClient, service classes (AuthService, DeviceService, TunnelService, ProviderService, SystemService, SetupService, InfoService), and API types. Zero runtime deps (native `fetch`). Linked via Yarn `portal:` protocol.

---

## 2. Daemon Architecture

### Startup Sequence

1. Parse CLI args, load config from `/etc/wardnet/wardnet.toml`
2. Initialize tracing/logging (hierarchical spans, optional OpenTelemetry export)
3. Detect shutdown type: check for graceful shutdown flag file — if absent, record unclean shutdown in DB
4. Open SQLite (WAL mode), run migrations
5. Create EventPublisher (`BroadcastEventBus` wrapping `tokio::broadcast`)
6. Create service instances with trait-based DI (AuthService, DeviceService, SystemService, TunnelService)
7. Start TunnelManager -- restore tunnels from DB (but do NOT bring interfaces up yet -- tunnels start on-demand)
8. Start DeviceDetector -- ARP/packet sniffing on LAN interface
9. Start RoutingListener -- subscribes to events, dispatches to RoutingService which reconciles DB state with kernel (ip rule, nftables)
10. Start DhcpServer -- begin serving DHCP leases on LAN interface (if enabled in config)
11. _(Future)_ Start DnsManager -- generate Unbound config, reload Unbound
12. _(Future)_ Broadcast GARP reclaiming gateway role (announce gateway IP at Pi's MAC)
13. _(Future)_ Start hardware watchdog petting loop
14. Start API server (axum) with shared AppState — serves REST API + embedded web UI
15. _(Future)_ Write graceful shutdown flag file on exit
16. _(Future)_ Signal readiness to systemd (`sd_notify`)

> **Note:** No admin account is bootstrapped on startup. The first admin is created via the web UI setup wizard (`POST /api/setup`). The daemon starts in "setup mode" until this is completed.

### Internal Event Bus

Components communicate via the `EventPublisher` trait (backed by `tokio::broadcast`):

```
TunnelManager  --> TunnelUp, TunnelDown, TunnelStatsUpdated
DeviceDetector --> DeviceDiscovered, DeviceIpChanged, DeviceGone
RoutingEngine  --> RoutingRuleChanged
DhcpServer     --> DhcpLeaseAssigned, DhcpLeaseRenewed, DhcpConflictDetected
GarpManager    --> GarpFailoverSent, GarpReclaimSent
```

Subscribers:
- **WebSocket handler**: pushes events to connected UI clients
- **RoutingListener**: dispatches to RoutingService on tunnel/device/routing events (DeviceDiscovered, DeviceIpChanged, DeviceGone, TunnelUp, TunnelDown, RoutingRuleChanged)
- **TunnelManager**: reacts to device events (lazy bring-up/teardown)
- **DnsManager**: _(future)_ reacts to routing changes
- **DeviceDetector**: correlates DHCP lease events with device registry (MAC → IP → device identity)
- **Dashboard/UI**: surfaces DHCP conflict alerts and unclean shutdown warnings

### Device Discovery -- Detailed Mechanism

The DeviceDetector passively discovers devices that are routing traffic through the Pi. It does NOT scan the network -- it only sees devices that have set the Pi as their gateway.

**Detection flow:**

1. **Raw socket listener** -- Uses `pnet` to open a raw socket on the LAN interface (eth0). Captures:
   - **ARP packets**: When a device sends an ARP request/reply, extract source MAC + source IP. ARP is the most reliable signal because every device on the LAN must ARP.
   - **IP packets**: For devices already in the ARP cache, inspect incoming IP packet source addresses to confirm the device is actively routing through the Pi.

2. **New device registration** -- When a MAC address is seen for the first time:
   - Insert into SQLite: MAC, source IP, first_seen timestamp, last_seen timestamp
   - Emit `DeviceDiscovered` event
   - Kick off async hostname resolution (runs in background, updates DB when complete):
     - Current: reverse DNS via `getent hosts` (uses NSS, covers DNS and local name resolution)
     - Future (Milestone 1e): DHCP lease table becomes primary source (option 12 hostname from client), with `getent hosts` as fallback
   - MAC OUI prefix lookup against an embedded manufacturer database (first 3 bytes of MAC -> vendor name). This gives a rough device type hint (e.g. "Apple" -> likely phone/laptop, "Samsung" -> could be TV or phone, "Espressif" -> IoT).

3. **Known device tracking** -- For already-registered devices:
   - Update `last_seen` timestamp (batched every 30 seconds, not per-packet)
   - If same MAC appears with a different IP (DHCP renewal), update `last_ip` in DB and emit `DeviceIpChanged` event. The RoutingEngine subscribes to this and updates `ip rule` entries.

4. **Device departure detection** -- When a known device stops sending traffic:
   - If `last_seen` exceeds a configurable timeout (default: 5 minutes), mark the device as "gone" and emit `DeviceGone` event.
   - The TunnelManager subscribes to `DeviceGone`: if no other active devices are using a particular tunnel, start a teardown countdown (configurable, default: 10 minutes). If no device reclaims the tunnel before timeout, tear down the WireGuard interface.
   - If the device reappears (same MAC, any IP), emit `DeviceDiscovered` again, and the TunnelManager brings the tunnel back up.

5. **Admin overrides** -- Admin can:
   - Assign a persistent friendly name to any device (overrides auto-detected hostname)
   - Set device type manually (tv, phone, laptop, tablet, unknown)
   - These are stored separately from auto-detected values and take precedence in the UI

### Lazy Tunnel Lifecycle

Tunnels are NOT kept up at all times. They are brought up on-demand and torn down when idle:

- **Tunnel starts DOWN** -- When daemon boots or a new tunnel config is added, the WireGuard interface is configured but not brought up.
- **Bring up on device need** -- When a routing rule assigns a device to a tunnel, and that device is active (recently seen), the TunnelManager brings the tunnel interface up. If the tunnel is already up (other devices using it), no action needed.
- **Tear down on idle** -- When the last active device using a tunnel goes away (`DeviceGone`), start an idle countdown (configurable, default: 10 minutes). If no device needs the tunnel before the countdown expires, tear down the WireGuard interface.
- **Immediate bring-up on return** -- If a device reappears and its rule points to a down tunnel, bring it up immediately. The device may experience a few seconds of no connectivity while the tunnel handshake completes.

### Authentication

Two-tier model: unauthenticated self-service for regular users, admin login for privileged operations.

- **Unauthenticated self-service** -- Any device on the LAN can access the web UI without logging in. The UI auto-detects the requesting device (by source IP via `ConnectInfo<SocketAddr>`) and shows a self-service "My Device" view: the user can see their own device info and routing status. No login required. This is the default experience for household members.
- **Setup wizard** -- On first run, the daemon has no admin account. The web UI detects this via `GET /api/setup/status` and redirects to a setup page where the user creates the first admin account (username + password, min 8 chars, Argon2id hashed). `POST /api/setup` creates the account and marks setup as completed. No default credentials are ever generated.
- **Admin login** -- After setup, admin logs in via `POST /api/auth/login` to access privileged features: tunnel management, all device management, system settings. Login returns a session cookie. Non-admin users see a "Sign in as admin" link in the sidebar.
- **Admin API key** -- Generated during wizard setup. Stored hashed (argon2) in SQLite. Used by CLI and scripts for admin-level API access.
- **CLI (wctl)** -- Authenticates with admin API key via `Authorization: Bearer <key>` header. Stored in `~/.config/wardnet/wctl.toml`.
- **Auth middleware** -- axum middleware with three access levels:
  - **Public (no auth):** `GET /api/devices/me` (returns the requesting device based on source IP), `PUT /api/devices/me/rule` (self-service routing change)
  - **Admin (session cookie or API key):** All other `/api/*` endpoints -- tunnel CRUD, all-devices list, system settings, user management
  - The `/api/auth/login` endpoint is always public
- **Admin override** -- If admin has locked a device's routing rule, the self-service endpoint returns a clear message ("your device's routing is managed by the admin") and rejects changes.

### Key Design Decisions

1. **Trait-based system abstractions** -- TunnelInterface, KeyStore, EventPublisher, FirewallManager, PolicyRouter, PacketCapture, DhcpSocket, CommandExecutor traits allow mocking for tests without root
2. **Event bus over direct coupling** -- `EventPublisher` trait with `BroadcastEventBus` impl; components are independently testable
3. **nftables DNAT for DNS leak prevention** -- more reliable than Unbound per-client config
4. **Single binary with embedded web UI** -- rust-embed, no separate web server
5. **Reconciliation on startup** -- daemon reads DB desired state and applies to kernel, handles crashes/reboots
6. **Private keys stored as files** -- `/etc/wardnet/keys/<tunnel-id>.key` (mode 600), never in SQLite or API responses
7. **Lazy tunnel lifecycle** -- tunnels brought up on-demand when devices need them, torn down after idle timeout
8. **HTTP only for MVP** -- Plain HTTP on LAN (like Pi-hole, Home Assistant). Optional HTTPS with user-provided cert in Phase 2.
9. **Dedicated wardnet user** -- No running as root. Daemon runs as `wardnet` user with Linux capabilities.
10. **Built-in DHCP server** -- Wardnet runs its own DHCP server on the LAN interface, making the Pi the default gateway and DNS for all devices automatically. No per-device manual configuration needed. Short lease time (10 min) for fast recovery after restarts.
11. **GARP failover** -- On graceful shutdown, broadcast GARP announcing router's MAC as gateway so devices fall back to the real router instantly. On startup, broadcast GARP reclaiming gateway role. Router MAC persisted to disk during setup so failover works even during crash recovery.
12. **Hardware watchdog** -- Linux watchdog configured at install time. If wardnetd hangs, kernel reboots the Pi within 15 seconds. Combined with systemd Restart=always (RestartSec=2s), provides defence in depth.
13. **Hierarchical tracing spans** -- Root span `wardnetd{version=...}` wraps the entire daemon. Each background component (`tunnel_monitor`, `device_detector`, `idle_watcher`, `api_server`) creates a child span. Per-HTTP-request spans include method, path, content-length, response status, and latency. All `tokio::spawn` tasks must be `.instrument(span)` since spawned tasks don't inherit parent spans. Version appears in every log entry (console and JSON).
14. **Git-based versioning** -- `build.rs` runs `git describe --tags` at compile time. Release builds from tags get clean SemVer (`0.1.0`). Dev builds get `0.1.1-dev.N+gabc1234`. Shared parsing logic in `build-support/version.rs` included by both `wardnetd` and `wctl` build scripts.
15. **Setup wizard (no default credentials)** -- No admin account is bootstrapped on startup. The web UI detects first-run via `GET /api/setup/status` and presents a setup wizard to create the first admin. Passwords are Argon2id hashed. `POST /api/setup` is a one-time endpoint that returns 409 if already completed.
16. **Unauthenticated info endpoint** -- `GET /api/info` returns version + uptime without auth. Used by the web UI connection status widget to show daemon reachability and version regardless of login state.
17. **Full IEEE OUI database** -- MAC manufacturer lookup uses the complete IEEE MA-L database (~39K entries) parsed from `data/oui.csv` at build time via `build.rs`. Locally administered MACs (randomized by Android/iOS) are detected by checking bit 1 of the first byte.
18. **Host resource monitoring** -- System status endpoint reports CPU usage, memory used/total via the `sysinfo` crate. A persistent `System` instance behind `tokio::sync::Mutex` provides accurate CPU readings across calls.
19. **Separate SDK package** -- `@wardnet/js` in `source/sdk/wardnet-js/` contains all API client code, services, and TypeScript types. Zero runtime dependencies (uses native `fetch`). The web UI is pure presentation — no API calls or business logic in components.

### Running Without Root

The daemon runs as a dedicated `wardnet` system user, never as root:

- **systemd capabilities:**
  ```ini
  User=wardnet
  Group=wardnet
  Restart=always
  RestartSec=2s
  AmbientCapabilities=CAP_NET_ADMIN CAP_NET_RAW CAP_NET_BIND_SERVICE
  CapabilityBoundingSet=CAP_NET_ADMIN CAP_NET_RAW CAP_NET_BIND_SERVICE
  ```
  - `CAP_NET_ADMIN` -- create/configure WireGuard interfaces, manipulate routing tables, manage nftables rules
  - `CAP_NET_RAW` -- raw socket for packet capture (device detection via pnet)
  - `CAP_NET_BIND_SERVICE` -- bind to port 80 (web UI)

- **File permissions:**
  - `/etc/wardnet/` owned by `wardnet:wardnet`, mode 750
  - `/etc/wardnet/keys/` mode 700 -- WireGuard private keys with mode 600
  - `/var/lib/wardnet/` owned by `wardnet:wardnet` -- SQLite database
  - `/etc/unbound/unbound.conf.d/wardnet.conf` -- writable by `wardnet` (add `wardnet` to `unbound` group, or use a sudoers entry for `unbound-control reload` only)

- **Install script** creates the `wardnet` user and sets up all permissions.

### Routing Implementation

Each tunnel gets a dedicated Linux routing table. For device routed through tunnel T:
1. `ip rule add from <device_ip> lookup <table_for_T>`
2. Table T: `default via <tunnel_gw> dev wg_wardN`
3. nftables: masquerade on tunnel interface for device source IP
4. nftables: DNAT port 53 to local Unbound (which forwards to tunnel DNS)

When device IP changes (DHCP renewal), remove old rules and apply new ones.

---

## 3. Implementation Milestones

### Milestone 1a: Project Scaffolding & Foundation ✅

**Goal:** Compilable workspace, database, basic API endpoint, auth skeleton.

- [x] Initialize Cargo workspace with 3 crates (`wardnetd`, `wardnet-types`, `wctl`)
- [x] Define shared types in `wardnet-types`: Device, Tunnel, RoutingRule, RoutingTarget, WardnetEvent, API DTOs, auth types (ApiKey, Role, Session)
- [x] SQLite setup: initial migration (devices, tunnels, routing_rules, api_keys, sessions, system_config tables), WAL mode
- [x] Daemon config loading from TOML (default `./wardnet.toml`, configurable via `--config`)
- [x] Basic `main.rs`: parse args (`--verbose`, `--config`, `--foreground`), load config, open DB, start axum on port 7411
- [x] Tracing setup (default `error` level, `--verbose` enables `debug`)
- [x] Auth middleware: three-tier access (public self-service, admin session, admin API key)
- [x] `GET /api/devices/me` -- public, returns requesting device by source IP
- [x] `PUT /api/devices/me/rule` -- public, self-service routing change (blocked if admin-locked)
- [x] `GET /api/system/status` -- admin-only endpoint
- [x] `POST /api/auth/login` -- admin login, returns session cookie
- [x] Scaffold web UI: Vite 7 + React 19 + Tailwind CSS 4 + React Router 7 + TanStack Query 5 + Yarn 4
- [x] `rust-embed` serving web UI dist from daemon
- [x] GitHub Actions CI: cargo fmt/clippy/test, yarn type-check/lint/format/build
- [x] Cross-compilation config for `aarch64-unknown-linux-gnu`, `x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`
- [x] Layered architecture with trait-based DI: repository → service → API handlers
- [x] 61 tests: wardnet-types serde round-trips (21), repository integration with in-memory SQLite (19), service unit tests with mocks (20), config defaults (1)
- [x] AGENTS.md, README.md, wctl CLI skeleton with clap subcommands

**Deliverable:** `cargo run` starts daemon, serves web UI, auth works, responds to `/api/system/status`.

### Milestone 1b: WireGuard Tunnel Management ✅

**Goal:** Create, destroy, and monitor WireGuard tunnels via API. Tunnels start down and are brought up on-demand.

- [x] WireGuard `.conf` file parser (`wardnet-types/src/wireguard_config.rs`)
- [x] TunnelInterface trait + WireGuardTunnelInterface (Linux kernel + macOS userspace) + noop for `--mock-network`
- [x] Lazy lifecycle: tunnels configured but down by default, brought up via explicit `bring_up(tunnel_id)` call
- [x] Tunnel persistence in SQLite (address, dns, peer_config JSON columns); restore configs on daemon start
- [x] Health monitoring: background TunnelMonitor polling `last_handshake` every 10s for active tunnels
- [x] Stats collection: TunnelMonitor polls byte counters every 5s for active tunnels, publishes events
- [x] Idle tunnel teardown: IdleTunnelWatcher scaffold (full implementation in Milestone 1c when DeviceGone events exist)
- [x] EventPublisher trait + BroadcastEventBus wired with tunnel events
- [x] KeyStore trait + FileKeyStore for private keys on disk (mode 0600, never in DB or API)
- [x] API (admin only): `GET /api/tunnels`, `POST /api/tunnels`, `DELETE /api/tunnels/:id`
- [x] Interface naming: `wg_ward0`, `wg_ward1`, ... (sequential from DB)
- [x] Repository refactored: traits in `repository/<name>.rs`, SQLite impls in `repository/sqlite/<name>.rs`
- [x] 97 tests at completion of 1b: .conf parser (10), event bus (3), key store (5), tunnel repository integration (11), tunnel service unit (7), plus all existing tests (61)

**Deliverable:** Add tunnel via API, bring it up on demand, monitor health, tear it down.

### Milestone 1c: Device Detection ✅

**Goal:** Passively detect devices routing through the Pi, track presence, detect departure.

> **Note:** VPN Provider Integration (Milestone 1k below) depends on tunnel management (1b) being complete but is independent of device detection. It can be implemented in parallel with 1c–1f or after them.

- [x] DeviceDetector using `pnet`: raw socket on LAN interface capturing ARP packets + IP traffic
- [x] PacketCapture trait + PnetCapture (real) + NoopPacketCapture (`--mock-network`)
- [x] New MAC -> insert to DB, emit `DeviceDiscovered`, start async hostname resolution
- [x] HostnameResolver trait + SystemHostnameResolver (getent hosts) + NoopHostnameResolver (`--mock-network`)
- [x] Known MAC with new IP -> update DB, emit `DeviceIpChanged`
- [x] `last_seen` batch updates every 30s
- [x] Device departure: configurable timeout (default 5min), emit `DeviceGone` when exceeded
- [x] Device reappearance: re-emit `DeviceDiscovered` if previously gone
- [x] MAC OUI prefix lookup for manufacturer/device type hinting (full IEEE MA-L database, ~39K entries, parsed at build time from `data/oui.csv`)
- [x] Locally administered MAC detection (randomized MACs from Android/iOS privacy features)
- [x] DeviceDiscoveryService trait + impl with full observation processing pipeline
- [x] API: `GET /api/devices` (admin: all, user: own), `GET /api/devices/:id`, `PUT /api/devices/:id` (admin: rename, set type)
- [x] Unified build system: Makefile with `make init`, `make build-pi`, `make run-pi`, `make check`, used by both local dev and CI
- [x] CI updated: removed `cross` tool dependency, uses native `cargo` + cross-linker (same approach locally and on CI)
- [x] Git-based version management: `build.rs` derives version from `git describe` (release: `0.1.0`, dev: `0.1.1-dev.N+gabc1234`). Shared logic in `build-support/version.rs`, used by both `wardnetd` and `wctl` via `include!()`
- [x] Structured observability: hierarchical tracing spans (`wardnetd{version}` → `tunnel_monitor` / `idle_watcher` / `device_detector` / `api_server` → `http_request{method, path}`). Version appears in every log entry. JSON format includes span context via `with_current_span` / `with_span_list`. All background `tokio::spawn` tasks instrumented with parent spans.
- [x] Optional OpenTelemetry export: `[otel]` config section (disabled by default). When enabled, exports traces (spans) and logs via OTLP gRPC to a configured endpoint. Uses `tracing-opentelemetry` to bridge existing tracing spans and `opentelemetry-appender-tracing` for log export. Providers gracefully shut down on daemon exit to flush buffered telemetry. `make run-pi OTEL=true` auto-detects local IP for the collector endpoint.

**Deliverable:** Devices routing through Pi appear automatically, departures detected, hostname resolved.

### Milestone 1d: Policy Routing Engine ✅

**Goal:** Route individual devices through specific tunnels or direct internet. Tunnels brought up on-demand.

- [x] PolicyRouter trait + IproutePolicyRouter impl: `ip rule add/del`, `ip route add/del`, `sysctl` IP forwarding via CommandExecutor abstraction
- [x] FirewallManager trait + NftablesFirewallManager impl: nftables masquerade chains, DNS DNAT rules, per-device NAT via CommandExecutor
- [x] RoutingService trait + impl: orchestrates PolicyRouter + FirewallManager + TunnelService for per-device routing (apply_rule, remove_device_routes, handle_ip_change, handle_tunnel_down, handle_tunnel_up, reconcile, devices_using_tunnel)
- [x] Routing table management: one Linux routing table per tunnel (table numbers 100+)
- [x] RoutingService.apply_rule(device_id, device_ip, target): brings up tunnel on-demand, adds `ip rule`, configures masquerade + DNS DNAT
- [x] RoutingService.remove_device_routes(device_id, device_ip): cleans up all kernel state
- [x] Rule persistence in DB; reconciliation on startup (enables IP forwarding, inits nftables, applies all stored rules, cleans orphans)
- [x] RoutingListener background task subscribes to event bus:
  - `RoutingRuleChanged`: apply new rule for device
  - `DeviceDiscovered`: if device has stored rule targeting a tunnel, apply routing
  - `DeviceIpChanged`: update rules when device IP changes
  - `TunnelDown`: remove routes for affected devices (soft fallback to direct)
  - `TunnelUp`: re-apply routing rules for devices targeting this tunnel
  - `DeviceGone`: remove kernel routing state for departed device
- [x] Global default policy enforcement via RoutingTarget::Default resolution
- [x] API: `PUT /api/devices/:id/rule` -- set routing rule (tunnel/direct/default), admin-only
- [x] Mutex-serialized kernel state modifications to prevent race conditions
- [x] In-memory tracking of applied rules (AppliedRule per device) for efficient updates
- [x] 77 new tests: PolicyRouter (17), FirewallManager (19), RoutingService (24), RoutingListener (17)

**Deliverable:** Assign device to tunnel, tunnel comes up automatically, traffic exits through VPN. Device leaves, tunnel tears down after timeout.

### Milestone 1e: DHCP Server ✅

**Goal:** Run a DHCP server on the LAN interface so all devices automatically use the Pi as their default gateway and DNS server — no per-device manual configuration.

- [x] Embedded DHCP server using `dhcproto` crate with DhcpSocket trait abstraction (UdpDhcpSocket for production, mock for tests)
- [x] DHCP packet processing: handles DISCOVER → OFFER → REQUEST → ACK flow
- [x] Advertises Pi as default gateway (option 3) and DNS server (option 6)
- [x] Configurable lease time (default 10 minutes), pool range, and gateway/DNS settings
- [x] Static DHCP reservations: bind MAC → fixed IP, persist in SQLite (`dhcp_reservations` table)
- [x] DHCP conflict detection: detect other DHCP servers on the network, emit `DhcpConflictDetected` event
- [x] Lease lifecycle: assignment, renewal, revocation, expiry tracking
- [x] Lease audit log: all assignments and renewals logged to `dhcp_lease_log` table
- [x] DhcpService trait + impl: config management, lease CRUD, reservation CRUD, status reporting
- [x] DhcpRunner: manages DHCP server lifecycle (start/stop, config reload)
- [x] DhcpRepository: lease persistence, reservation persistence, lease log, pool queries
- [x] DHCP types in `wardnet-types`: DhcpConfig, DhcpLease, DhcpLeaseStatus, DhcpReservation
- [x] DB migration: `dhcp_leases`, `dhcp_reservations`, `dhcp_lease_log` tables
- [x] API (admin-only):
  - `GET /api/dhcp/config` -- get current DHCP config
  - `PUT /api/dhcp/config` -- update pool/lease settings
  - `POST /api/dhcp/config/toggle` -- enable/disable DHCP server
  - `GET /api/dhcp/leases` -- list active leases
  - `DELETE /api/dhcp/leases/:id` -- revoke a lease
  - `GET /api/dhcp/reservations` -- list static reservations
  - `POST /api/dhcp/reservations` -- create reservation
  - `DELETE /api/dhcp/reservations/:id` -- delete reservation
  - `GET /api/dhcp/status` -- server running status and pool usage
- [x] 45 tests: DHCP server packet handling, runner lifecycle, service logic
- [ ] _(Remaining)_ Hostname resolution from DHCP lease tables (option 12 client hostname as primary source, reverse DNS as fallback)
- [ ] _(Remaining)_ Per-device static configuration fallback mode instructions

**Deliverable:** Devices joining the network automatically receive the Pi as their gateway. Admin can manage static reservations via API. Conflict detection alerts if another DHCP server is present.

### Milestone 1f: Gateway Resilience & Failover

**Goal:** Ensure managed devices maintain internet connectivity during Pi downtime, and restore Wardnet routing quickly after reboot.

- [ ] GARP (Gratuitous ARP) module: send raw ARP replies via `pnet` on the LAN interface
- [ ] Graceful shutdown sequence: broadcast GARP announcing gateway IP at router's MAC, wait 500ms, repeat once, then proceed with shutdown/reboot. Must complete within 1 second (hard requirement).
- [ ] Startup GARP: broadcast GARP reclaiming gateway role (gateway IP at Pi's MAC)
- [ ] Persist router MAC to disk during setup (`/etc/wardnet/router_mac`) — GARP must not depend on live network scan
- [ ] Shutdown type detection: write flag file on graceful shutdown, check on startup — if absent, record unclean shutdown in DB with timestamp
- [ ] Unclean shutdown warning: surface in dashboard as persistent banner until acknowledged
- [ ] Hardware watchdog integration: open `/dev/watchdog`, pet periodically from a dedicated tokio task. If wardnetd hangs, kernel reboots within 15 seconds.
- [ ] systemd unit: `Restart=always`, `RestartSec=2s`
- [ ] Boot sequence optimisation notes for install script (disable unnecessary services, target network-online.target early) — full restoration within 30 seconds
- [ ] API (admin-only): `POST /api/system/reboot` (GARP-first), `POST /api/system/shutdown` (GARP-first)
- [ ] DB migration: add `router_mac`, `last_shutdown`, `last_shutdown_at` to `system_config` table
- [ ] GarpOps trait + impl for testability; NoopGarp for `--mock-network` mode
- [ ] WatchdogOps trait + impl; NoopWatchdog for dev/test environments

**Deliverable:** Safe reboot/shutdown via API triggers GARP failover. Devices fall back to router within 1 second. On startup, GARP reclaims gateway role. Hardware watchdog reboots Pi if daemon hangs.

### Milestone 1g: DNS Server & Ad Blocking

**Goal:** Built-in DNS resolver with network-wide ad blocking (replaces Pi-hole) and VPN DNS leak prevention. The primary motivation for running a local DNS server is ad blocking — without it, Wardnet would simply forward to Google or Cloudflare DNS.

- [ ] Embedded DNS resolver (Unbound) managed by the daemon
- [ ] DNS-based ad blocking: filter list support (EasyList, Steven Black, etc.), configurable blocklists
- [ ] Per-device ad blocking toggle: admin can enable/disable ad blocking for individual devices
- [ ] Query log: track DNS queries per device for visibility and debugging
- [ ] Whitelist/blacklist: admin can override filter decisions per domain
- [ ] nftables DNAT rules: redirect port 53 (UDP+TCP) from all devices to local resolver
- [ ] VPN-routed devices: forward upstream DNS to tunnel DNS server (leak prevention)
- [ ] Direct devices: resolve via upstream DoH/DoT (Cloudflare, Google, or custom) with ad blocking applied
- [ ] Subscribe to RoutingRuleChanged and TunnelUp/TunnelDown to update DNS forwarding rules
- [ ] Reload DNS config on changes
- [ ] API (admin-only): DNS config, blocklist management, query log, per-device ad blocking toggle, whitelist/blacklist CRUD
- [ ] DnsManager trait + impl for testability; NoopDnsManager for `--mock-network`
- [ ] Web UI: DNS settings page, ad blocking configuration page, per-device toggle in device detail

**Deliverable:** All devices use Wardnet for DNS with ad blocking applied network-wide. Per-device ad blocking control. VPN-routed devices resolve through tunnel DNS (no leaks). Admin manages blocklists, overrides, and per-device settings via web UI.

### Milestone 1h: Web UI -- Core Layout & Pages (In Progress)

**Goal:** Functional web UI with branded design, responsive layout, and real API data.

#### Completed:
- [x] **SDK package (`@wardnet/js`)**: TypeScript SDK with WardnetClient, AuthService, DeviceService, TunnelService, ProviderService, SystemService, SetupService, InfoService. Zero runtime deps, browser + Node 18+ support via native `fetch`. Linked via Yarn `portal:` protocol.
- [x] **shadcn/ui integration**: Button, Card, Sheet, Dialog, Select, RadioGroup, Switch, Tabs, Textarea, Command (combobox), Input, InputGroup, Label, Badge, Popover components in `src/components/core/ui/`
- [x] **Brand design**: Deep indigo + green accent palette (oklch), custom CSS variables for light/dark mode, Geist font
- [x] **Dark/light mode**: System preference via `prefers-color-scheme`, toggles `.dark` class on `<html>`
- [x] **Responsive layout**: Desktop persistent sidebar + mobile hamburger menu (shadcn Sheet). Deep indigo sidebar in both modes, frosted glass mobile header.
- [x] **Logo**: PNG from `artwork/logo.png` embedded in sidebar and auth pages
- [x] **Auth flow**: Zustand store, session cookie, admin vs self-service route guards, "Sign in as admin" / "Sign out" in sidebar
- [x] **Setup wizard**: First-run detection via `GET /api/setup/status`, redirect to setup page, create admin account, auto-login after setup
- [x] **Connection status widget**: Traffic-light indicator + version in sidebar footer, uses unauthenticated `/api/info` endpoint
- [x] **TanStack Query hooks**: useDevices (10s poll), useDevice (single), useMyDevice, useTunnels (15s poll), useProviders, useSystemStatus (30s poll), useDaemonStatus (30s poll), useSetup, plus mutation hooks (useSetMyRule, useUpdateDevice, useCreateTunnel, useDeleteTunnel)
- [x] **Dashboard (admin)**: 6-card grid — Devices, Tunnels, Uptime, CPU, Memory, Database. Usage bars for CPU/memory with color thresholds.
- [x] **Devices page (admin)**: Responsive table with device type icon, name/MAC, IP, type badge, manufacturer, last seen. Full IEEE OUI manufacturer data.
- [x] **Tunnels page (admin)**: Card grid with status badges, traffic stats. CreateTunnelSheet for adding tunnels.
- [x] **Settings page (admin)**: System info display (version, uptime, device/tunnel counts, database size)
- [x] **My Device (self-service)**: Stacked layout with device icon, IP/MAC/manufacturer, routing status. Helpful hint when device not detected (SSH tunnel case).
- [x] **Login page**: Full-screen indigo gradient hero with logo, username/password form, error handling (401 vs network error)
- [x] **Device type icons**: Lucide icons mapped to device types (TV, Phone, Laptop, Tablet, Console, Set-top Box, IoT, Unknown)
- [x] **Compound components**: CountryCombobox (VPN provider country selector), RoutingSelector (routing target picker), ApiErrorAlert (error display), ConfirmDialog (destructive action confirmation), ConnectionBanner (daemon unreachable warning)
- [x] **Utility functions**: formatBytes, formatUptime, timeAgo
- [x] **Polling**: Devices 10s, tunnels 15s, system status 30s, daemon info 30s
- [x] **Component architecture refactor**: strict layering (core/ui → compound → features → pages). DataTable (shadcn Table + TanStack Table) shared across all list views. Pages are pure wiring — no raw HTML.
- [x] **DHCP panel (admin)**: Status card with toggle, config card with edit sheet, leases table with revoke + make-static, reservations table with add/delete. Three paths to create reservations: from device edit, from active lease, or manual.
- [x] **Dashboard widgets**: DHCP summary card, Recent Errors card (in-memory ring buffer of last 15 WARN/ERROR), live log viewer (WebSocket stream with level filter, pause/resume, clear, download)
- [x] **Live log streaming**: WebSocket endpoint `/api/system/logs/stream` with per-client filter commands. `BroadcastLayer` tracing subscriber sends structured entries (message, fields, span context) to all connected clients. `RecentErrorsLayer` captures WARN/ERROR into ring buffer for `/api/system/errors` endpoint.
- [x] **Toast notifications**: Sonner toasts on all mutations (success green, error red) — tunnels, devices, DHCP, providers
- [x] **Favicon**: 16px + 32px PNGs from logo
- [x] **Connection banner**: Light red banner on all pages when daemon is unreachable
- [x] **Confirm dialogs**: Tunnel delete, lease revoke, reservation delete
- [x] **Device list sorted by name**: Alphabetical sort by name/hostname/MAC
- [x] **404 page**: Clean full-page 404 outside the app layout
- [x] **DNS & Ad Blocking placeholder pages**: Sidebar links + "coming soon" content
- [x] **DhcpService in SDK**: Full TypeScript SDK service + types for all DHCP endpoints
- [x] **Log file always JSON**: File output uses JSON format regardless of console setting, enabling reliable API/UI parsing

#### Priority tasks for next session:
- [ ] **Logging level audit**: establish clear rules for when to use each level (error/warn/info/debug/trace) and review all existing log statements. Currently abusing INFO for routine operations that should be DEBUG (e.g. every packet observation, every IP change, ARP scans). Rule of thumb: INFO = operator-relevant state changes (server started, device first discovered, tunnel up/down), DEBUG = operational details (individual packets, routine refreshes, periodic scans). Document the rules in AGENTS.md and enforce in code review.

#### Remaining:
- [ ] Settings: user management (create API keys), global default policy
- [ ] Extended first-run wizard: static IP config, DHCP onboarding, router MAC discovery, add first tunnel, set default policy
- [ ] Shutdown/reboot controls: Safe Reboot button, Safe Shutdown button with confirmation (depends on Milestone 1f)
- [ ] Unclean shutdown warning banner (depends on Milestone 1f)

#### Remaining (backend needed):
- [ ] Device DHCP status: show whether each device is using a wardnet DHCP lease, reservation, or has an external/static IP. Requires joining devices with DHCP leases/reservations in the list API. Surfaces in the device table to track which devices are fully managed by wardnet.
- [ ] DHCP pool range change handling: when the admin changes the pool range via the web UI, what happens to existing leases outside the new range? Need to define behaviour — options: revoke out-of-range leases immediately (devices re-request), let them expire naturally, or warn the admin. Also need to hot-reload the DHCP server config without restart.

#### Nice-to-have (deferred):
- [ ] Per-entity error surfacing: tunnel card shows last error reason (e.g. DNS resolution failure, handshake timeout) — requires persisting last error on the tunnel model
- [ ] Per-entity error surfacing: devices page shows when a device has fallen back to direct routing due to tunnel failure (distinguish intentional "direct" from "fallback to direct")

#### Phase 2:
- [ ] Customizable dashboard: draggable/resizable widget grid (react-grid-layout), user layout saved to localStorage, default layout with reset option. Allows users to arrange widgets, hide/show sections, and resize to preference.
- [ ] Kernel keyring for WireGuard keys: store private keys in the Linux kernel keyring instead of files on disk. Keys live in kernel memory and are never written to the filesystem. Requires either patching/forking the `wireguard-control` crate or using a custom netlink interface for key management.
- [ ] WebSocket client hook for domain events: connect to `/api/ws`, dispatch events to Zustand stores for real-time updates (polling works well enough for MVP)

**Deliverable:** Full web UI for managing tunnels, devices, routing rules, DHCP, and system lifecycle with auth.

### Milestone 1i: CLI Tool (wctl) (In Progress)

**Goal:** Working CLI for power users and scripting.

#### Completed:
- [x] clap command structure: `status`, `devices list|show|set-rule`, `tunnels list|show|add|remove`
- [x] reqwest API client using `wardnet-types`, API key auth via `Authorization: Bearer <key>` header
- [x] Config at `~/.config/wardnet/wctl.toml` (daemon URL + API key)

#### Remaining:
- [ ] `wctl reboot` and `wctl shutdown` commands: invoke graceful GARP-first sequence via API (depends on Milestone 1f)
- [ ] Output: `tabled` for human mode, `serde_json` for `--json`

**Deliverable:** All MVP CLI commands working against running daemon, including safe reboot/shutdown.

### Milestone 1j: Installation & Packaging

**Goal:** One-command install on Raspberry Pi.

- [ ] `install.sh`: detect OS, install WireGuard + Unbound + nftables, download binary, create `wardnet` user/group, set up dirs + permissions, install systemd unit, enable IP forwarding, configure hardware watchdog, disable unnecessary boot services, optimise boot sequence for fast restoration, print URL
- [ ] `wardnetd.service`: User=wardnet, AmbientCapabilities=CAP_NET_ADMIN CAP_NET_RAW CAP_NET_BIND_SERVICE, Restart=always, RestartSec=2s, WatchdogSec=15
- [ ] mDNS advertisement as `wardnet.local`
- [ ] GitHub Actions release workflow: cross-compile aarch64 + x86_64, build web UI, embed, publish release
- [ ] Documentation: README quick start, manual install steps

**Deliverable:** `curl -sSL https://get.wardnet.dev | bash` installs working Wardnet with watchdog and boot optimisation.

### Milestone 1k: VPN Provider Integration

**Goal:** Pluggable VPN provider system with NordVPN as the first implementation. Allows adding tunnels via guided setup instead of manual .conf import.

- [x] `VpnProvider` trait defining the provider interface:
  - `info()` — returns `ProviderInfo` (id, name, auth_methods, icon_url, website_url)
  - `validate_credentials(credentials)` — verify auth against provider API
  - `list_servers(credentials, filters)` — fetch available servers (filterable by country, load)
  - `generate_config(credentials, server)` — produce a WireGuard .conf string for the selected server
- [x] Provider registry: `VpnProviderRegistry` with config-driven enable/disable via `[providers.enabled]` TOML section. Self-registers all built-in providers at construction.
- [x] `ProviderService` trait + impl: list providers, validate credentials, list servers, setup tunnel (validate → list servers → pick lowest load → generate config → import via TunnelService)
- [x] NordVPN provider implementation:
  - `NordVpnApi` trait for testability (provider-specific HTTP abstraction, not generic HttpClient)
  - Async `reqwest::Client` for non-blocking HTTP calls
  - Country code resolution: calls `/v1/servers/countries` to convert ISO codes to NordVPN numeric IDs
  - Fetch server list from NordVPN API filtered by WireGuard support + country
  - Generate WireGuard config from API data (public key extraction from technology metadata)
  - Server selection: by country code, with auto-select (lowest load) or explicit `server_id`
- [x] API endpoints (admin-only):
  - `GET /api/providers` — list registered providers with metadata
  - `POST /api/providers/{id}/validate` — validate credentials for a provider
  - `POST /api/providers/{id}/servers` — list available servers (POST because body contains credentials)
  - `POST /api/providers/{id}/setup` — full guided setup: validate + pick server + generate config + import tunnel
- [x] Provider types in `wardnet-types`: `ProviderInfo`, `ProviderAuthMethod`, `ProviderCredentials` (tagged enum), `ServerFilter`, `ServerInfo`, plus API request/response types
- [x] Tests (38 new tests): mock `NordVpnApi` for unit tests, `MockVpnProvider` + `MockTunnelService` for service tests, `MockProviderService` for API handler tests. Serde round-trips for all provider types.

**Deliverable:** Admin can add a NordVPN tunnel from the API by providing credentials and picking a country — no manual .conf file needed. Architecture ready for community to add Mullvad, ProtonVPN, etc.

### Milestone 1l: Integration Testing & Hardening (In Progress)

**Goal:** Confidence that the system works end-to-end.

#### Completed:
- [x] **wardnet-test-agent** crate: lightweight HTTP server (port 3001) deployed on Pi, exposes kernel networking state for test assertions:
  - `GET /ip-rules` -- parse and return `ip rule list`
  - `GET /nft-rules` -- parse and return `nft list ruleset`
  - `GET /wg/:interface` -- parse `wg show` (peers, handshakes, transfer stats)
  - `GET /link/:interface` -- parse `ip link show` (up/down, MTU)
  - `POST /container/exec` -- execute commands in test containers
  - `GET /fixtures/:name` -- serve generated WireGuard test configs/keys
  - Input validation against command injection
- [x] **System tests** (`source/system-tests/`): TypeScript E2E suite targeting real Pi deployment, uses @wardnet/js SDK + test-agent client:
  - `01-health.ts` -- daemon health check
  - `02-tunnel-import.ts` -- tunnel import and activation
  - `03-device-detection.ts` -- device discovery
  - `04-device-routing.ts` -- per-device routing rules (ip rule, nftables, WireGuard verification)
  - `05-traffic-routing.ts` -- traffic flow verification
  - `06-multi-tunnel.ts` -- multiple simultaneous tunnels
  - `07-idle-teardown.ts` -- idle tunnel cleanup
- [x] **Makefile integration**: `make system-test` (full build → deploy → test → teardown), `make system-test-setup`, `make system-test-teardown`

#### Remaining:
- [ ] Failure tests: tunnel down + fallback, daemon restart + state recovery, kill -9 + reconciliation, unclean shutdown detection
- [ ] DHCP tests: lease assignment, static reservation, conflict detection with second DHCP server, lease correlation with device registry
- [ ] GARP tests: graceful shutdown sends GARP with router MAC, startup sends GARP reclaim, sequence completes within 1 second, internet interruption under 2 seconds for graceful restarts
- [ ] Watchdog tests: verify daemon pets watchdog, verify reboot on simulated hang
- [ ] Auth tests: self-service auto-detection by IP, admin login, admin-locked device rejection, API key validation
- [ ] Cross-platform testing: Debian 11/12, Ubuntu 22.04/24.04
- [ ] Hardware testing: Pi 4, Pi 5

---

## 4. Technical Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Async runtime | Tokio | Industry standard, required by axum/sqlx |
| Event bus | `tokio::broadcast` | Fan-out to multiple subscribers, lightweight |
| Error handling | `thiserror` (domain) + `anyhow` (boundaries) | Structured errors where it matters, ergonomic elsewhere |
| DB migrations | `sqlx::migrate!()` embedded | Runs at startup, idempotent, compile-time query checking |
| API versioning | None for v1 | No external consumers yet, add `/api/v2/` when needed |
| WebSocket format | JSON with typed envelope `{type, timestamp, payload}` | Simple, debuggable in browser DevTools, serde already in stack. Event volume is low (few/sec max), so payload size is negligible. |
| Firewall | nftables native | Target Debian 12+; install script ensures nftables on Debian 11 |
| HTTP / TLS | HTTP only for MVP (port 80) | LAN-only appliance, like Pi-hole/Home Assistant. Optional HTTPS with user-provided cert in Phase 2. |
| DNS resolver | Unbound (managed by daemon) | Battle-tested resolver. Primary purpose is network-wide ad blocking (replaces Pi-hole) with per-device toggle. Also prevents DNS leaks for VPN-routed devices. |
| Auth | Unauthenticated self-service + admin login | Users configure own device without login (auto-detected by source IP). Admin login for privileged ops. API key for CLI. |
| Privileges | Dedicated `wardnet` user + Linux capabilities | No root. CAP_NET_ADMIN + CAP_NET_RAW + CAP_NET_BIND_SERVICE via systemd. |
| SQLite concurrency | WAL mode + sqlx pool | Concurrent reads, single writer, sufficient for expected load |
| Tunnel lifecycle | Lazy (on-demand up, idle teardown) | Don't waste resources on unused tunnels. Configurable idle timeout. |
| DHCP server | Embedded in daemon (Rust crate) | Eliminates external dependency; full control over lease management, conflict detection, and device registry correlation. Short 10-min leases for fast recovery. |
| GARP failover | Raw ARP via `pnet` | Instant gateway handoff on shutdown/startup. Router MAC persisted to disk so it works even during crash recovery. Must complete within 1 second. |
| Hardware watchdog | Linux `/dev/watchdog` | Defence in depth: if daemon hangs (not crashes), kernel reboots within 15 seconds. Combined with systemd Restart=always + RestartSec=2s. |
| Shutdown orchestration | Flag file + DB tracking | Simple graceful vs unclean detection. Flag file written on startup, removed on graceful shutdown. Absence on next boot = unclean. |
| Observability | `tracing` + optional OpenTelemetry | Hierarchical spans with version in every log entry. OTel OTLP export (traces + logs) opt-in via `[otel]` config. Bridges existing tracing spans via `tracing-opentelemetry`. |
| Versioning | `git describe` at compile time | SemVer from tags (`0.1.0`), dev builds auto-increment patch with commit info (`0.1.1-dev.N+gabc1234`). Shared `build-support/version.rs`. |
| Admin bootstrapping | Setup wizard (web UI) | No default credentials. First admin created via `POST /api/setup`. Passwords Argon2id hashed. One-time endpoint (409 after completion). |
| OUI database | IEEE MA-L CSV, build-time | Full ~39K-entry database parsed by `build.rs` from `data/oui.csv`. Locally administered MACs detected as "Randomized MAC". |
| Host monitoring | `sysinfo` crate | CPU usage, memory used/total reported in system status. Persistent `System` instance for accurate CPU readings. |
| SDK architecture | Separate `@wardnet/js` package | Pure TypeScript, zero deps, native `fetch`. Linked via Yarn `portal:`. Web UI components are pure presentation. |
| Component library | shadcn/ui (Radix + Tailwind) | Owned thin wrappers in `core/ui/`, not modified directly. Layered component architecture: core → compound → features → layouts → pages. |
| Data polling | TanStack Query `refetchInterval` | Devices 10s, tunnels 15s, system status 30s, daemon info 30s. No WebSocket needed for MVP. |
| Policy routing | `ip rule` + `ip route` via CommandExecutor | Source-based routing per device. One routing table per tunnel (100+). Mutex-serialized kernel modifications. |
| Firewall impl | nftables via CommandExecutor | wardnet_nat table with masquerade + DNS DNAT chains. Per-device rules keyed by source IP. |
| System tests | TypeScript + wardnet-test-agent | E2E tests run against real Pi deployment. Test agent exposes kernel state (ip rule, nft, wg show) via HTTP for assertions. |

---

## 5. Key Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Cross-compilation for ARM64 with C deps | Use `rustls` (not OpenSSL), bundled SQLite, minimize C deps. Native `cargo` + cross-linker (no Docker-based `cross` tool). |
| nftables vs iptables across Debian versions | Target nftables; install script sets up iptables-nft on older systems. Abstract behind `Firewall` trait. |
| Testing routing without root | Trait-based abstractions (PolicyRouter, TunnelInterface, FirewallManager, CommandExecutor) enable mocked unit tests (77 routing tests, 8 WireGuard tests, 19 firewall tests). Real integration tests use Pi deployment with test-agent. |
| WireGuard kernel module missing | Install script checks and falls back to wireguard-dkms or wireguard-go. |
| Non-root capabilities insufficient | Validate all required syscalls work with CAP_NET_ADMIN + CAP_NET_RAW early in Milestone 1b. If any operation requires root, find alternative (e.g. helper binary with setuid for that specific operation). |
| SQLite write contention | WAL mode + short transactions. Write frequency is low (config changes, not high throughput). |
| Daemon crash leaves stale kernel state | Reconciliation on startup: compare DB desired state with kernel actual state, fix drift. Graceful shutdown cleans up on SIGTERM. |
| Unbound config writes need permissions | Add `wardnet` user to `unbound` group, or use sudoers entry scoped to `unbound-control reload` only. |
| DHCP conflict with ISP router | Setup wizard detects existing DHCP servers and guides user to disable them. If router is locked (ISP device), falls back to per-device static config mode with generated instructions. Post-setup conflict detection alerts admin immediately. |
| GARP not supported by all devices | GARP is best-effort for instant failover. Devices that ignore GARP will still recover via ARP cache expiry (typically 1-5 minutes) or on next DHCP renewal (10 minutes). Short lease time is the safety net. |
| Hardware watchdog not available | Not all hardware has `/dev/watchdog`. Install script detects availability and skips if absent. Systemd Restart=always is the primary recovery mechanism; watchdog is defence in depth. |
| Raw ARP requires CAP_NET_RAW | Already required for device detection via `pnet`. No additional capability needed for GARP. |

---

## 6. Development Workflow

- **First-time setup:** `make init` -- installs Rust cross-compilation target + toolchain, yarn dependencies
- **Web UI dev:** `cd source/web-ui && yarn dev` -- Vite on :7412, proxies `/api/*` to daemon on :7411
- **Daemon dev:** `cd source/daemon && cargo run -p wardnetd -- --verbose --mock-network` -- reads web-ui/dist/ from filesystem in debug mode (rust-embed)
- **CLI dev:** `cd source/daemon && cargo run -p wctl -- <command>`
- **Build for Pi:** `make build-pi` -- cross-compiles with native `cargo` + aarch64-linux-gnu linker (no Docker)
- **Deploy to Pi:** `make run-pi PI_HOST=<ip> PI_USER=<user> PI_LAN_IF=<iface>` -- cross-compiles, deploys via SSH, starts with verbose logging. Deletes database by default; use `RESUME=true` to keep existing data. Optional: `OTEL=true` for OpenTelemetry export.
- **Run checks:** `make check` -- runs all linting, formatting, and tests (web + daemon)
- **Mock mode:** `wardnetd --mock-network` uses NoopWireGuard, NoopPacketCapture, NoopHostnameResolver for local development without real network interfaces
- **Real testing:** Deploy to the Pi — real WireGuard, packet capture, and device detection require Linux with the correct kernel modules and capabilities

---

## 7. Verification

### How to test end-to-end:

1. **Self-service:** Open web UI from a device (no login), verify it auto-detects the device and shows routing controls. Change routing rule, verify it applies. Verify admin-locked devices show a locked message.
2. **Admin auth:** Login as admin, verify full access to all devices/tunnels/settings. Use API key from wctl, verify admin access. Verify unauthenticated requests to admin endpoints are rejected.
3. **Tunnel management:** Add a WireGuard .conf via API/UI (tunnel stays down). Verify config persisted.
4. **Device detection:** Point a device's gateway to the Pi, verify it appears in device list within 10 seconds. Hostname resolved automatically.
5. **On-demand tunnel:** Assign active device to a tunnel, verify tunnel comes up automatically. Run `curl ifconfig.me` from device, verify exit IP matches tunnel country.
6. **DNS leak prevention:** Set device to VPN tunnel, run `nslookup example.com`, verify DNS resolved through tunnel DNS (not ISP).
7. **Device departure:** Power off the device, verify `DeviceGone` event after timeout. Verify tunnel tears down after idle timeout if no other devices use it.
8. **Device return:** Power device back on, verify it's re-detected, tunnel comes back up, routing restored.
9. **Fallback:** Kill the WireGuard tunnel externally, verify device falls back to direct, verify event appears in WebSocket/UI.
10. **Persistence:** Reboot the Pi, verify configs restored, tunnels stay down until devices are detected, routing rules re-applied correctly.
11. **Web UI:** Complete first-run wizard, manage tunnels and devices from dashboard, verify real-time updates via WebSocket.
12. **CLI:** Run `wctl status`, `wctl devices`, `wctl set <device> <tunnel>`, verify output matches UI state.
13. **DHCP:** Connect a new device to the network with DHCP, verify it receives Pi as gateway and DNS, verify it appears in device list and DHCP lease panel.
14. **Static reservations:** Add a MAC → IP reservation via API, verify device receives the reserved IP on next DHCP renewal.
15. **DHCP conflict:** Start a second DHCP server on the network, verify admin alert appears within seconds.
16. **Safe reboot:** Click Safe Reboot in UI (or `wctl reboot`), verify GARP is sent (tcpdump), verify devices fall back to router, verify Pi reboots and reclaims gateway via GARP on startup, verify internet interruption < 2 seconds.
17. **Safe shutdown:** Click Safe Shutdown in UI (or `wctl shutdown`), verify GARP failover, verify warning about internet unavailability was shown.
18. **Unclean shutdown:** Kill the Pi (pull power), verify on next boot the dashboard shows unclean shutdown warning with timestamp.
19. **Watchdog:** Simulate daemon hang (SIGSTOP), verify Pi reboots within 15 seconds.
