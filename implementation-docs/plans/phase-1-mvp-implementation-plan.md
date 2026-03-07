# Wardnet Phase 1 (MVP) Implementation Plan

**Last updated:** March 2026

---

## Context

Wardnet is a self-hosted network privacy gateway for Raspberry Pi that sits alongside an existing router. It encrypts traffic via WireGuard tunnels, provides per-device routing control, and prevents DNS leaks -- all managed from a single web UI. This plan covers Phase 1 (MVP): the foundational system that proves the core value proposition.

**Phase 1 scope:** Core daemon with WireGuard tunnel management, policy routing, device detection, DNS leak prevention, REST+WebSocket API with API key auth, web UI (wizard + dashboard + device/tunnel management), CLI tool, and installation packaging.

**Out of scope for Phase 1:** Temporary/scheduled routing, ad blocking, kill switch per-device configuration, curated provider wizard, mobile app.

---

## 1. Project Structure

### Cargo Workspace

```
wardnet/
├── Cargo.toml                    # workspace root
├── crates/
│   ├── wardnetd/                 # binary -- the daemon
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── api/              # axum REST + WebSocket + auth middleware
│   │   │   ├── tunnel/           # WireGuard tunnel lifecycle
│   │   │   ├── routing/          # policy routing (ip rule, nftables)
│   │   │   ├── device/           # passive device detection
│   │   │   ├── dns/              # Unbound config generation
│   │   │   ├── db/               # SQLite via sqlx
│   │   │   ├── event/            # internal event bus
│   │   │   ├── config/           # daemon TOML config
│   │   │   └── system/           # system stats
│   │   └── migrations/           # sqlx SQL migrations
│   ├── wctl/                     # binary -- CLI tool
│   │   └── src/
│   │       ├── main.rs
│   │       ├── commands/         # one module per subcommand
│   │       └── output/           # table vs JSON formatting
│   └── wardnet-types/            # library -- shared types
│       └── src/
│           ├── lib.rs
│           ├── device.rs
│           ├── tunnel.rs
│           ├── routing.rs
│           ├── api.rs            # request/response DTOs
│           └── event.rs          # event types
├── web/                          # React + Vite project
│   └── src/
│       ├── api/                  # TanStack Query hooks
│       ├── ws/                   # WebSocket client
│       ├── stores/               # Zustand stores
│       ├── pages/                # Dashboard, Devices, Tunnels, Settings, wizard/
│       ├── components/           # layout/, devices/, tunnels/, shared/
│       ├── types/                # TypeScript API types
│       └── lib/                  # utils (countries, formatters)
├── install/                      # install script + systemd unit + Unbound template
└── implementation-docs/
```

### Crate Dependencies

- `wardnet-types`: serde, uuid, chrono (no runtime deps -- pure data types)
- `wardnetd` depends on `wardnet-types` + tokio, axum, sqlx, wireguard-control, rtnetlink, pnet, rust-embed, toml, tracing
- `wctl` depends on `wardnet-types` + clap, reqwest, tabled, tokio

### Web UI Stack

React + TypeScript, Vite, Tailwind CSS, TanStack Query, Zustand, React Router

---

## 2. Daemon Architecture

### Startup Sequence

1. Parse CLI args, load config from `/etc/wardnet/wardnet.toml`
2. Initialize tracing/logging
3. Open SQLite (WAL mode), run migrations
4. Create EventBus (`tokio::broadcast`)
5. Start TunnelManager -- restore tunnels from DB (but do NOT bring interfaces up yet -- tunnels start on-demand)
6. Start RoutingEngine -- reconcile DB state with kernel (ip rule, nftables)
7. Start DeviceDetector -- ARP/packet sniffing on LAN interface
8. Start DnsManager -- generate Unbound config, reload Unbound
9. Start API server (axum) with shared AppState
10. Signal readiness to systemd (`sd_notify`)

### Internal Event Bus

Components communicate via `tokio::broadcast::Sender<WardnetEvent>`:

```
TunnelManager  --> TunnelUp, TunnelDown, TunnelStatsUpdated
DeviceDetector --> DeviceDiscovered, DeviceIpChanged, DeviceGone
RoutingEngine  --> RoutingRuleChanged
```

Subscribers:
- **WebSocket handler**: pushes events to connected UI clients
- **RoutingEngine**: reacts to tunnel/device events
- **TunnelManager**: reacts to device events (lazy bring-up/teardown)
- **DnsManager**: reacts to routing changes

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
     - Reverse DNS lookup (`dig -x <ip>`)
     - mDNS query via Avahi/Bonjour (`avahi-resolve -a <ip>`)
     - Take whichever responds first; store as auto-detected hostname
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

1. **Trait-based system abstractions** -- NetlinkOps, WireGuardOps, FirewallOps traits allow mocking for tests without root
2. **Event bus over direct coupling** -- components are independently testable
3. **nftables DNAT for DNS leak prevention** -- more reliable than Unbound per-client config
4. **Single binary with embedded web UI** -- rust-embed, no separate web server
5. **Reconciliation on startup** -- daemon reads DB desired state and applies to kernel, handles crashes/reboots
6. **Private keys stored as files** -- `/etc/wardnet/keys/<tunnel-id>.key` (mode 600), never in SQLite or API responses
7. **Lazy tunnel lifecycle** -- tunnels brought up on-demand when devices need them, torn down after idle timeout
8. **HTTP only for MVP** -- Plain HTTP on LAN (like Pi-hole, Home Assistant). Optional HTTPS with user-provided cert in Phase 2.
9. **Dedicated wardnet user** -- No running as root. Daemon runs as `wardnet` user with Linux capabilities.

### Running Without Root

The daemon runs as a dedicated `wardnet` system user, never as root:

- **systemd capabilities:**
  ```ini
  User=wardnet
  Group=wardnet
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

### Milestone 1a: Project Scaffolding & Foundation

**Goal:** Compilable workspace, database, basic API endpoint, auth skeleton.

- [ ] Initialize Cargo workspace with 3 crates
- [ ] Define shared types in `wardnet-types`: Device, Tunnel, RoutingRule, RoutingTarget, WardnetEvent, API DTOs, auth types (ApiKey, Role, Session)
- [ ] SQLite setup: initial migration (devices, tunnels, routing_rules, api_keys, sessions, system_config tables), WAL mode
- [ ] Daemon config loading from TOML (`/etc/wardnet/wardnet.toml`)
- [ ] Basic `main.rs`: parse args, load config, open DB, start axum
- [ ] Tracing setup (console + JSON output)
- [ ] Auth middleware: three-tier access (public self-service, admin session, admin API key)
- [ ] `GET /api/devices/me` -- public, returns requesting device by source IP
- [ ] `PUT /api/devices/me/rule` -- public, self-service routing change (blocked if admin-locked)
- [ ] `GET /api/system/status` -- admin-only endpoint
- [ ] `POST /api/auth/login` -- admin login, returns session cookie
- [ ] Scaffold web UI: Vite + React + Tailwind + React Router + TanStack Query
- [ ] `rust-embed` serving web UI dist from daemon
- [ ] GitHub Actions CI: cargo check/test/clippy, npm build
- [ ] Cross-compilation config for `aarch64-unknown-linux-gnu`

**Deliverable:** `cargo run` starts daemon, serves placeholder web page, auth works, responds to `/api/system/status`.

### Milestone 1b: WireGuard Tunnel Management

**Goal:** Create, destroy, and monitor WireGuard tunnels via API. Tunnels start down and are brought up on-demand.

- [ ] WireGuard `.conf` file parser
- [ ] TunnelManager: create/destroy interfaces via `wireguard-control` (netlink)
- [ ] Lazy lifecycle: tunnels configured but down by default, brought up via explicit `bring_up(tunnel_id)` call
- [ ] Tunnel persistence in SQLite; restore configs (not interfaces) on daemon start
- [ ] Health monitoring: background task polling `last_handshake` every 10s for active tunnels, emit events
- [ ] Stats collection: byte counters every 5s for active tunnels
- [ ] Idle tunnel teardown: subscribe to `DeviceGone`, start countdown, tear down if no devices need it
- [ ] EventBus wired with tunnel events
- [ ] API (admin only): `GET /api/tunnels`, `POST /api/tunnels`, `DELETE /api/tunnels/:id`
- [ ] Interface naming: `wg_ward0`, `wg_ward1`, ... (avoid collisions)
- [ ] Unit tests for config parser; integration tests for interface lifecycle (require capabilities, `#[ignore]`)

**Deliverable:** Add tunnel via API, bring it up on demand, monitor health, tear it down.

### Milestone 1c: Device Detection

**Goal:** Passively detect devices routing through the Pi, track presence, detect departure.

- [ ] DeviceDetector using `pnet`: raw socket on LAN interface capturing ARP packets + IP traffic
- [ ] New MAC -> insert to DB, emit `DeviceDiscovered`, start async hostname resolution (reverse DNS + mDNS via Avahi)
- [ ] Known MAC with new IP -> update DB, emit `DeviceIpChanged`
- [ ] `last_seen` batch updates every 30s
- [ ] Device departure: configurable timeout (default 5min), emit `DeviceGone` when exceeded
- [ ] Device reappearance: re-emit `DeviceDiscovered` if previously gone
- [ ] MAC OUI prefix lookup for manufacturer/device type hinting (embedded database)
- [ ] API: `GET /api/devices` (admin: all, user: own), `GET /api/devices/:id`, `PUT /api/devices/:id` (admin: rename, set type)

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

### Milestone 1e: DNS Leak Prevention

**Goal:** VPN-routed devices use tunnel DNS; direct devices use Unbound with DoH/DoT.

- [ ] DnsManager: generate Unbound config fragment at `/etc/unbound/unbound.conf.d/wardnet.conf`
- [ ] nftables DNAT rules: redirect port 53 (UDP+TCP) from VPN-routed devices to local Unbound
- [ ] Unbound forwards to tunnel DNS for VPN-routed traffic, upstream DoH/DoT for direct
- [ ] Subscribe to RoutingRuleChanged and TunnelUp/TunnelDown to update DNS rules
- [ ] Reload Unbound via `unbound-control reload` on config changes

**Deliverable:** DNS queries from VPN-routed devices resolve through tunnel DNS.

### Milestone 1f: Web UI -- Core Pages

**Goal:** Functional web UI for managing the system.

- [ ] API client layer: TanStack Query hooks, typed requests matching wardnet-types
- [ ] Auth: no-login self-service view (auto-detect device by IP, show routing controls), admin login for full management
- [ ] WebSocket client hook: connect to `/api/ws`, dispatch events to Zustand stores
- [ ] App shell: sidebar nav (Dashboard, Devices, Tunnels, Settings)
- [ ] **Dashboard:** device summary (active/gone), tunnel health overview (up/down/idle), quick-action route change
- [ ] **Devices page:** table with name/IP/MAC/route/status/last_seen, click for detail, routing rule selector dropdown
- [ ] **Tunnels page (admin):** list with status/country/bytes, add tunnel (upload .conf or paste), delete with confirmation (warn if devices assigned)
- [ ] **Settings page (admin):** user management (create API keys, assign devices), global default policy, system info
- [ ] **First-Run Wizard:** detect first-boot, steps: set admin username/password -> add first tunnel -> set default policy -> done
- [ ] Real-time updates via WebSocket (device status, tunnel health, device discovered/gone)

**Deliverable:** Full web UI for managing tunnels, devices, and routing rules with auth.

### Milestone 1g: CLI Tool (wctl)

**Goal:** Working CLI for power users and scripting.

- [ ] clap command structure: `status`, `devices`, `set`, `tunnels`, `tunnel add/remove`
- [ ] reqwest API client using `wardnet-types`, API key auth via `Authorization: Bearer <key>` header
- [ ] Output: `tabled` for human mode, `serde_json` for `--json`
- [ ] Config at `~/.config/wardnet/wctl.toml` (daemon URL + API key)

**Deliverable:** All MVP CLI commands working against running daemon.

### Milestone 1h: Installation & Packaging

**Goal:** One-command install on Raspberry Pi.

- [ ] `install.sh`: detect OS, install WireGuard + Unbound + nftables, download binary, create `wardnet` user/group, set up dirs + permissions, install systemd unit, enable IP forwarding, print URL
- [ ] `wardnetd.service`: User=wardnet, AmbientCapabilities=CAP_NET_ADMIN CAP_NET_RAW CAP_NET_BIND_SERVICE, Restart=always
- [ ] mDNS advertisement as `wardnet.local`
- [ ] GitHub Actions release workflow: cross-compile aarch64 + x86_64, build web UI, embed, publish release
- [ ] Documentation: README quick start, manual install steps

**Deliverable:** `curl -sSL https://get.wardnet.dev | bash` installs working Wardnet.

### Milestone 1i: Integration Testing & Hardening

**Goal:** Confidence that the system works end-to-end.

- [ ] Network namespace test harness: automated setup/teardown of ns-client, ns-wardnet, ns-vpn-server
- [ ] E2E tests: create tunnel -> detect device -> tunnel comes up -> apply rule -> verify routing -> device leaves -> tunnel tears down after timeout
- [ ] Failure tests: tunnel down + fallback, daemon restart + state recovery, kill -9 + reconciliation
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

---

## 5. Key Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Cross-compilation for ARM64 with C deps | Use `rustls` (not OpenSSL), bundled SQLite, minimize C deps. Use `cross` tool. |
| nftables vs iptables across Debian versions | Target nftables; install script sets up iptables-nft on older systems. Abstract behind `Firewall` trait. |
| Testing routing without root | Trait-based abstractions (NetlinkOps, WireGuardOps, FirewallOps) enable mocked unit tests. Real integration tests use network namespaces. |
| WireGuard kernel module missing | Install script checks and falls back to wireguard-dkms or wireguard-go. |
| Non-root capabilities insufficient | Validate all required syscalls work with CAP_NET_ADMIN + CAP_NET_RAW early in Milestone 1b. If any operation requires root, find alternative (e.g. helper binary with setuid for that specific operation). |
| SQLite write contention | WAL mode + short transactions. Write frequency is low (config changes, not high throughput). |
| Daemon crash leaves stale kernel state | Reconciliation on startup: compare DB desired state with kernel actual state, fix drift. Graceful shutdown cleans up on SIGTERM. |
| Unbound config writes need permissions | Add `wardnet` user to `unbound` group, or use sudoers entry scoped to `unbound-control reload` only. |

---

## 6. Development Workflow

- **Web UI dev:** `cd web && npm run dev` -- Vite on :5173, proxies `/api/*` to daemon
- **Daemon dev:** `cargo run -p wardnetd` -- reads web/dist/ from filesystem in debug mode (rust-embed `debug_embed=false`)
- **CLI dev:** `cargo run -p wctl -- <command>`
- **Networking dev on macOS:** Use a Linux VM (Multipass/Docker) for kernel networking tests
- **Mock mode:** `wardnetd --mock-network` logs all kernel commands instead of executing (for UI development without real tunnels)

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
