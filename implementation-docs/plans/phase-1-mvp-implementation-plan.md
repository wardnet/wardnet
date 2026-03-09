# Wardnet Phase 1 (MVP) Implementation Plan

**Last updated:** March 2026

---

## Context

Wardnet is a self-hosted network privacy gateway for Raspberry Pi that sits alongside an existing router. It encrypts traffic via WireGuard tunnels, provides per-device routing control, and prevents DNS leaks -- all managed from a single web UI. This plan covers Phase 1 (MVP): the foundational system that proves the core value proposition.

**Phase 1 scope:** Core daemon with WireGuard tunnel management, policy routing, device detection, DHCP server (gateway advertisement, static reservations, conflict detection), gateway resilience (hardware watchdog, GARP failover, graceful reboot/shutdown), VPN provider integration (pluggable architecture + NordVPN), DNS leak prevention, REST+WebSocket API with API key auth, web UI (wizard with DHCP onboarding + router MAC discovery, dashboard, device/tunnel management, guided provider setup, safe reboot/shutdown, DHCP panel), CLI tool (including reboot/shutdown commands), and installation packaging.

**Out of scope for Phase 1:** Temporary/scheduled routing, ad blocking, kill switch per-device configuration, mobile app.

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
│   │       │   │   ├── config.rs     # TOML config loading
│   │       │   │   ├── db.rs         # SQLite pool + migrations
│   │       │   │   ├── error.rs      # AppError → IntoResponse
│   │       │   │   ├── state.rs      # AppState (service trait objects)
│   │       │   │   ├── event.rs      # EventPublisher trait + BroadcastEventBus
│   │       │   │   ├── keys.rs       # KeyStore trait + FileKeyStore
│   │       │   │   ├── wireguard.rs  # WireGuardOps trait + types
│   │       │   │   ├── tunnel_monitor.rs  # Background health/stats tasks
│   │       │   │   ├── tunnel_idle.rs     # Idle tunnel teardown watcher
│   │       │   │   ├── dhcp.rs       # DHCP server (lease mgmt, reservations, conflict detection)
│   │       │   │   ├── garp.rs       # GARP (Gratuitous ARP) failover sequences
│   │       │   │   ├── watchdog.rs   # Hardware watchdog integration
│   │       │   │   ├── shutdown.rs   # Graceful shutdown/reboot orchestration
│   │       │   │   └── web.rs        # rust-embed static file serving
│   │       │   └── migrations/       # sqlx SQL migrations
│   │       ├── wctl/                 # binary -- CLI tool
│   │       │   └── src/main.rs       # clap subcommands skeleton
│   │       └── wardnet-types/        # library -- shared types
│   │           └── src/
│   │               ├── lib.rs
│   │               ├── device.rs
│   │               ├── tunnel.rs     # Tunnel, TunnelStatus, TunnelConfig
│   │               ├── routing.rs
│   │               ├── api.rs        # request/response DTOs
│   │               ├── auth.rs       # Session, ApiKeyRecord, Role
│   │               ├── event.rs      # WardnetEvent enum
│   │               └── wireguard_config.rs  # .conf parser + WgConfig types
│   └── web-ui/                       # React + Vite project
│       └── src/
│           ├── api/                  # typed fetch client
│           ├── pages/                # Dashboard, Devices, Tunnels, Settings
│           ├── components/           # Layout, shared
│           └── types/                # TypeScript API types
├── .github/workflows/ci.yml
├── implementation-docs/
├── AGENTS.md
└── README.md
```

> **Note:** Modules for routing/, device/ (detection), dns/, dhcp/, garp/, watchdog/, and install/ will be added in later milestones. Tunnel management, event bus, and key storage are implemented in Milestone 1b.

### Crate Dependencies

- `wardnet-types`: serde, uuid, chrono, thiserror (no runtime deps -- pure data types)
- `wardnetd` depends on `wardnet-types` + tokio, axum, sqlx, wireguard-control, ipnetwork, tokio-util, pnet (device detection + GARP), dhcproto (DHCP server), reqwest (for VPN provider APIs), rust-embed, toml, tracing, argon2, async-trait
- `wctl` depends on `wardnet-types` + clap, reqwest, tabled, tokio

### Web UI Stack

React + TypeScript, Vite, Tailwind CSS, TanStack Query, Zustand, React Router

---

## 2. Daemon Architecture

### Startup Sequence

1. Parse CLI args, load config from `/etc/wardnet/wardnet.toml`
2. Initialize tracing/logging
3. Detect shutdown type: check for graceful shutdown flag file — if absent, record unclean shutdown in DB
4. Open SQLite (WAL mode), run migrations
5. Create EventPublisher (`BroadcastEventBus` wrapping `tokio::broadcast`)
6. Start TunnelManager -- restore tunnels from DB (but do NOT bring interfaces up yet -- tunnels start on-demand)
7. Start RoutingEngine -- reconcile DB state with kernel (ip rule, nftables)
8. Start DeviceDetector -- ARP/packet sniffing on LAN interface
9. Start DhcpServer -- begin serving DHCP leases on LAN interface
10. Start DnsManager -- generate Unbound config, reload Unbound
11. Broadcast GARP reclaiming gateway role (announce gateway IP at Pi's MAC)
12. Start hardware watchdog petting loop
13. Start API server (axum) with shared AppState
14. Write graceful shutdown flag file
15. Signal readiness to systemd (`sd_notify`)

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
- **RoutingEngine**: reacts to tunnel/device events
- **TunnelManager**: reacts to device events (lazy bring-up/teardown)
- **DnsManager**: reacts to routing changes
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

- **Unauthenticated self-service** -- Any device on the LAN can access the web UI without logging in. The UI auto-detects the requesting device (by source IP/MAC) and shows a self-service view: the user can see their own device and change its routing rule (tunnel/direct/default). No login required. This is the default experience for household members.
- **Admin login** -- The first-run wizard sets up an admin account (username + password). Admin logs in to access privileged features: tunnel management, all device management, user overrides, system settings. Login returns a session token (httpOnly cookie).
- **Admin API key** -- Generated during wizard setup. Stored hashed (argon2) in SQLite. Used by CLI and scripts for admin-level API access.
- **CLI (wctl)** -- Authenticates with admin API key via `Authorization: Bearer <key>` header. Stored in `~/.config/wardnet/wctl.toml`.
- **Auth middleware** -- axum middleware with three access levels:
  - **Public (no auth):** `GET /api/devices/me` (returns the requesting device based on source IP), `PUT /api/devices/me/rule` (self-service routing change)
  - **Admin (session cookie or API key):** All other `/api/*` endpoints -- tunnel CRUD, all-devices list, system settings, user management
  - The `/api/auth/login` endpoint is always public
- **Admin override** -- If admin has locked a device's routing rule, the self-service endpoint returns a clear message ("your device's routing is managed by the admin") and rejects changes.

### Key Design Decisions

1. **Trait-based system abstractions** -- WireGuardOps, KeyStore, EventPublisher, FirewallOps (future) traits allow mocking for tests without root
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
13. **Hierarchical tracing spans** -- Root span `wardnetd{version=...}` wraps the entire daemon. Each background component (`tunnel_monitor`, `device_detector`, `idle_watcher`, `api_server`) creates a child span. Per-HTTP-request spans include method and path. All `tokio::spawn` tasks must be `.instrument(span)` since spawned tasks don't inherit parent spans. Version appears in every log entry (console and JSON).
14. **Git-based versioning** -- `build.rs` runs `git describe --tags` at compile time. Release builds from tags get clean SemVer (`0.1.0`). Dev builds get `0.1.1-dev.N+gabc1234`. Shared parsing logic in `build-support/version.rs` included by both `wardnetd` and `wctl` build scripts.

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
- [x] WireGuardOps trait + RealWireGuard (Linux kernel + macOS userspace) + NoopWireGuard (`--mock-network`)
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
- [x] 97 tests total: .conf parser (10), event bus (3), key store (5), tunnel repository integration (11), tunnel service unit (7), plus all existing tests (61)

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
- [x] MAC OUI prefix lookup for manufacturer/device type hinting (embedded database)
- [x] DeviceDiscoveryService trait + impl with full observation processing pipeline
- [x] API: `GET /api/devices` (admin: all, user: own), `GET /api/devices/:id`, `PUT /api/devices/:id` (admin: rename, set type)
- [x] Unified build system: Makefile with `make init`, `make build-pi`, `make run-pi`, `make check`, used by both local dev and CI
- [x] CI updated: removed `cross` tool dependency, uses native `cargo` + cross-linker (same approach locally and on CI)
- [x] Git-based version management: `build.rs` derives version from `git describe` (release: `0.1.0`, dev: `0.1.1-dev.N+gabc1234`). Shared logic in `build-support/version.rs`, used by both `wardnetd` and `wctl` via `include!()`
- [x] Structured observability: hierarchical tracing spans (`wardnetd{version}` → `tunnel_monitor` / `idle_watcher` / `device_detector` / `api_server` → `http_request{method, path}`). Version appears in every log entry. JSON format includes span context via `with_current_span` / `with_span_list`. All background `tokio::spawn` tasks instrumented with parent spans.
- [x] Optional OpenTelemetry export: `[otel]` config section (disabled by default). When enabled, exports traces (spans) and logs via OTLP gRPC to a configured endpoint. Uses `tracing-opentelemetry` to bridge existing tracing spans and `opentelemetry-appender-tracing` for log export. Providers gracefully shut down on daemon exit to flush buffered telemetry. `make run-pi OTEL=true` auto-detects local IP for the collector endpoint.

**Deliverable:** Devices routing through Pi appear automatically, departures detected, hostname resolved.

### Milestone 1d: Policy Routing Engine

**Goal:** Route individual devices through specific tunnels or direct internet. Tunnels brought up on-demand.

- [ ] Routing table management: one Linux routing table per tunnel
- [ ] RoutingEngine.apply_rule(device_ip, target): `ip rule` via rtnetlink + nftables masquerade + DNS DNAT
- [ ] RoutingEngine.remove_rule(device_ip): clean up kernel state
- [ ] Rule persistence in DB; reconciliation on startup
- [ ] Subscribe to `DeviceDiscovered`: if device has a rule targeting a tunnel, tell TunnelManager to bring it up, then apply routing
- [ ] Subscribe to `DeviceIpChanged`: update rules when device IP changes
- [ ] Subscribe to `TunnelDown`: fall back to direct for affected devices (soft fallback)
- [ ] Subscribe to `TunnelUp`: apply pending routing rules for devices waiting on this tunnel
- [ ] Subscribe to `DeviceGone`: remove routing rules, notify TunnelManager
- [ ] Global default policy enforcement
- [ ] API: `PUT /api/devices/:id/rule` -- set routing rule (tunnel/direct/default), admin-only for shared devices

**Deliverable:** Assign device to tunnel, tunnel comes up automatically, traffic exits through VPN. Device leaves, tunnel tears down after timeout.

### Milestone 1e: DHCP Server

**Goal:** Run a DHCP server on the LAN interface so all devices automatically use the Pi as their default gateway and DNS server — no per-device manual configuration.

> **Note — Hostname resolution:** When implementing DHCP, revisit the HostnameResolver to use DHCP lease tables as the primary source for hostname resolution (client-provided hostname from DHCP DISCOVER/REQUEST option 12). Fall back to reverse DNS (`getent hosts`) only when the DHCP table has no hostname for a device. This eliminates the dependency on mDNS/Avahi and provides more reliable names since most devices send their hostname during DHCP negotiation.

- [ ] Embedded DHCP server on the LAN interface (using `dhcproto` or similar crate), advertising Pi as default gateway (option 3) and DNS server (option 6)
- [ ] Include router IP as secondary entry in option 3 for automatic failover on supported clients
- [ ] Default lease time: 10 minutes (configurable)
- [ ] Static DHCP reservations: bind MAC → fixed IP, persist in SQLite (`dhcp_reservations` table)
- [ ] DHCP conflict detection: detect other DHCP servers on the network, emit `DhcpConflictDetected` event and surface admin alert
- [ ] Log all lease assignments and renewals, correlating MAC → IP → device identity in the device registry
- [ ] Per-device static configuration fallback mode: when router DHCP cannot be disabled, generate tailored static config instructions per device type
- [ ] API (admin-only): `GET /api/dhcp/leases`, `PUT /api/dhcp/reservations`, `DELETE /api/dhcp/reservations/:mac`
- [ ] DhcpServer trait + impl for testability; NoopDhcpServer for `--mock-network` mode
- [ ] DB migration: `dhcp_reservations` table (mac, ip, label, created_at)

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

### Milestone 1g: DNS Leak Prevention

**Goal:** VPN-routed devices use tunnel DNS; direct devices use Unbound with DoH/DoT.

- [ ] DnsManager: generate Unbound config fragment at `/etc/unbound/unbound.conf.d/wardnet.conf`
- [ ] nftables DNAT rules: redirect port 53 (UDP+TCP) from VPN-routed devices to local Unbound
- [ ] Unbound forwards to tunnel DNS for VPN-routed traffic, upstream DoH/DoT for direct
- [ ] Subscribe to RoutingRuleChanged and TunnelUp/TunnelDown to update DNS rules
- [ ] Reload Unbound via `unbound-control reload` on config changes

**Deliverable:** DNS queries from VPN-routed devices resolve through tunnel DNS.

### Milestone 1h: Web UI -- Core Pages

**Goal:** Functional web UI for managing the system.

- [ ] API client layer: TanStack Query hooks, typed requests matching wardnet-types
- [ ] Auth: no-login self-service view (auto-detect device by IP, show routing controls), admin login for full management
- [ ] WebSocket client hook: connect to `/api/ws`, dispatch events to Zustand stores
- [ ] App shell: sidebar nav (Dashboard, Devices, Tunnels, Settings)
- [ ] **Dashboard:** device summary (active/gone), tunnel health overview (up/down/idle), quick-action route change
- [ ] **Devices page:** table with name/IP/MAC/route/status/last_seen, click for detail, routing rule selector dropdown
- [ ] **Tunnels page (admin):** list with status/country/bytes, add tunnel (upload .conf or paste), delete with confirmation (warn if devices assigned)
- [ ] **Settings page (admin):** user management (create API keys, assign devices), global default policy, system info
- [ ] **First-Run Wizard:** detect first-boot, steps: set admin username/password -> configure static IP -> DHCP onboarding (detect existing DHCP server, guide user to disable it or switch to per-device fallback mode) -> router MAC discovery (silent, surfaces only on failure) -> add first tunnel -> set default policy -> done
- [ ] **Settings page additions:** last shutdown type (graceful/unclean) with warning banner, Safe Reboot button (prominent), Safe Shutdown button (with confirmation dialog), DHCP panel (active leases, static reservations, lease history)
- [ ] Real-time updates via WebSocket (device status, tunnel health, device discovered/gone, DHCP conflict alerts, unclean shutdown warnings)

**Deliverable:** Full web UI for managing tunnels, devices, routing rules, DHCP, and system lifecycle with auth.

### Milestone 1i: CLI Tool (wctl)

**Goal:** Working CLI for power users and scripting.

- [ ] clap command structure: `status`, `devices`, `set`, `tunnels`, `tunnel add/remove`, `reboot`, `shutdown`
- [ ] reqwest API client using `wardnet-types`, API key auth via `Authorization: Bearer <key>` header
- [ ] `wctl reboot` and `wctl shutdown` commands: invoke graceful GARP-first sequence via API
- [ ] Output: `tabled` for human mode, `serde_json` for `--json`
- [ ] Config at `~/.config/wardnet/wctl.toml` (daemon URL + API key)

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

- [ ] `VpnProvider` trait defining the provider interface:
  - `id()` — unique provider identifier (e.g. "nordvpn")
  - `name()` — display name (e.g. "NordVPN")
  - `auth_methods()` — supported authentication methods (credentials, token, OAuth)
  - `validate_credentials(credentials)` — verify auth against provider API
  - `list_servers(credentials, filters)` — fetch available servers (filterable by country, load, features)
  - `generate_config(credentials, server)` — produce a WireGuard .conf string for the selected server
- [ ] Provider registry: compile-time registration of providers, stored in a `Vec<Arc<dyn VpnProvider>>`. Exposed via `ProviderService` trait.
- [ ] `ProviderService` trait + impl: list providers, validate credentials, list servers, setup tunnel (validate → list servers → generate config → import via TunnelService)
- [ ] NordVPN provider implementation:
  - Auth via NordVPN service credentials (username/password from NordVPN account dashboard) or access token
  - Fetch server list from NordVPN API (`https://api.nordvpn.com/v1/servers`) filtered by WireGuard support + country
  - Generate WireGuard config using NordVPN's WireGuard private key exchange endpoint
  - Server selection: by country code, with optional "recommended" (lowest load)
- [ ] API endpoints (admin-only):
  - `GET /api/providers` — list registered providers with metadata
  - `POST /api/providers/:id/validate` — validate credentials for a provider
  - `GET /api/providers/:id/servers?country=XX` — list available servers
  - `POST /api/providers/:id/setup` — full guided setup: validate + pick server + generate config + import tunnel
- [ ] Provider types in `wardnet-types`: `ProviderInfo`, `ProviderAuthMethod`, `ServerInfo`, `SetupProviderRequest`, `SetupProviderResponse`
- [ ] Tests: mock HTTP client for NordVPN API, test credential validation, server listing, config generation, full setup flow

**Deliverable:** Admin can add a NordVPN tunnel from the API by providing credentials and picking a country — no manual .conf file needed. Architecture ready for community to add Mullvad, ProtonVPN, etc.

### Milestone 1l: Integration Testing & Hardening

**Goal:** Confidence that the system works end-to-end.

- [ ] Network namespace test harness: automated setup/teardown of ns-client, ns-wardnet, ns-vpn-server
- [ ] E2E tests: create tunnel -> detect device -> tunnel comes up -> apply rule -> verify routing -> device leaves -> tunnel tears down after timeout
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
| DNS resolver | Unbound (external, managed via config files) | Battle-tested, no need to embed a resolver |
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

---

## 5. Key Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Cross-compilation for ARM64 with C deps | Use `rustls` (not OpenSSL), bundled SQLite, minimize C deps. Native `cargo` + cross-linker (no Docker-based `cross` tool). |
| nftables vs iptables across Debian versions | Target nftables; install script sets up iptables-nft on older systems. Abstract behind `Firewall` trait. |
| Testing routing without root | Trait-based abstractions (NetlinkOps, WireGuardOps, FirewallOps) enable mocked unit tests. Real integration tests use network namespaces. |
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
- **Deploy to Pi:** `make deploy PI_HOST=<ip>` -- builds and copies binary via SSH, restarts service
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
