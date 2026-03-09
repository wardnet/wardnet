# Wardnet — Initial Product Requirements

**Project name:** Wardnet  
**Type:** Open source, community  
**Last updated:** March 2026

---

## 1. Product Overview

Wardnet is a self-hosted network privacy gateway that runs on a Raspberry Pi. It sits alongside an existing home or small-office router, and acts as the warden of every device's connection to
the internet — encrypting traffic, blocking ads and trackers at the DNS level, and giving you per-device control over how each device connects. Devices that cannot run VPN software themselves
(smart TVs, consoles, IoT) are fully protected at the gateway level.

### Value Proposition

> **Every device on your network — including ones that can't run a VPN — gets privacy protection and encrypted traffic. You control how each device connects to the internet, from one place.**

Most people treat a VPN as something you install on a phone or laptop. That leaves smart TVs, consoles, and IoT devices completely exposed — and means reconfiguring every device individually
when you want to change anything. Wardnet solves this by running on a single Pi box that sits on your network and acts as a privacy gateway for everything connected to it. Encrypted tunnels,
ad blocking, and per-device routing policy — all managed from one web UI.

The ability to route different devices through different VPN exit points (Countries) is a natural consequence of that control, not the headline feature.

### Goals

- Single Pi box that acts as a privacy and routing gateway for the whole household
- Encrypted, private traffic for every managed device — including those that cannot run VPN clients
- Ad and tracker blocking applied at the DNS level for all devices, regardless of routing policy
- Per-device routing control: each device can independently exit through a VPN tunnel or direct internet
- Admin + self-service model: admin controls shared devices, users control their own
- Open source, community-driven, extensible

### Out of Scope (v1)

- Acting as a full router (Pi sits alongside, not replacing, the existing router — but it does run its own DHCP server to manage gateway assignment)
- IPv6 tunnel routing
- Paid/commercial VPN resale
- Mobile native app (planned for v2 — v1 ships web UI only)

---

## 2. System Architecture Overview

```
Internet
    │
[ISP Router / Modem]  ← handles WiFi; DHCP delegated to Wardnet Pi
    │
    ├─── [Pi Box: Wardnet]  ← new device on LAN, static IP
    │         │
    │         ├── wg_ward0 → VPN Provider, US Exit
    │         ├── wg_ward1 → VPN Provider, UK Exit
    │         ├── wg_ward2 → VPN Provider, DE Exit
    │         └── eth0 → LAN (acts as policy-routing gateway)
    │
    ├─── Smart TV  ──────────► default gateway = Pi (routes via UK)
    ├─── Laptop    ──────────► default gateway = Pi (routes via US)
    ├─── Phone     ──────────► default gateway = Pi (direct internet)
    └─── Other devices ──────► default gateway = Router (unmanaged)
```

Wardnet runs its own DHCP server on the LAN, advertising the Pi as the default gateway and DNS server for all managed devices. Devices automatically route through the Pi without manual configuration. The Pi never replaces the router — it acts as the network's gateway while the router handles WiFi and upstream connectivity.

---

## 3. Functional Requirements

### 3.1 Core Daemon (wardnetd)

The heart of the system. Runs as a background service, owns all WireGuard interfaces and routing policy.

#### 3.1.1 WireGuard Tunnel Management

- **FR-001** Maintain multiple simultaneous outbound WireGuard tunnels (one per country/provider endpoint), each on its own network interface (wg_ward0, wg_ward1, …)
- **FR-002** Support importing WireGuard configuration files (.conf) directly — "bring your own provider"
- **FR-003** Support a pluggable VPN provider system with a `VpnProvider` trait/interface that defines how to authenticate, select servers, and generate WireGuard configurations for a specific provider. Each provider is registered at compile time and exposed via the API/UI. NordVPN is the first implementation (MVP); the architecture must allow other developers to add new providers (Mullvad, ProtonVPN, IVPN, etc.) by implementing the same interface
- **FR-003a** NordVPN provider (MVP): authenticate via NordVPN service credentials or token, fetch available servers by country, generate WireGuard .conf for the selected server, and import it as a tunnel — all from a guided UI flow
- **FR-003b** Provider registry: maintain a list of registered providers with metadata (name, logo URL, supported auth methods, setup instructions). The API exposes `GET /api/providers` to list available providers and `POST /api/providers/:id/setup` to initiate guided setup
- **FR-004** Automatically reconnect tunnels on failure with configurable retry backoff
- **FR-005** Monitor tunnel health via keepalive and last-handshake checks; expose health status over internal API
- **FR-006** Support naming tunnels with a human-readable label and country flag (e.g. "🇺🇸 United States – Mullvad")
- **FR-007** Allow multiple tunnels to the same country (e.g. two US endpoints for redundancy)

#### 3.1.2 Per-Device Policy Routing

- **FR-010** Maintain a policy table mapping device MAC address → routing rule
- **FR-011** Routing rules must support three target types:
  - `tunnel:<id>` — route through a specific VPN tunnel
  - `direct` — bypass VPN, exit through the ISP
  - `default` — inherit the network-wide default policy
- **FR-012** Implement routing using Linux policy routing (`ip rule` + `ip route` per routing table) and nftables/iptables for NAT masquerade per tunnel interface
- **FR-013** When no rule is set for a device, apply the global default policy (configurable by admin, defaults to `direct`)
- **FR-014** Admin-set rules take precedence over user-set rules for the same device. If admin has locked a device, the user-set rule is stored but ignored until admin removes their override
- **FR-015** Policy changes must take effect within 2 seconds without dropping existing connections where possible

#### 3.1.3 Device Detection

- **FR-020** Passively detect devices that are routing traffic through the Pi by inspecting packet source MAC/IP
- **FR-021** When a new device is first seen, record it in the device registry with: MAC address, first seen timestamp, IP address (most recent DHCP-assigned)
- **FR-022** Attempt to resolve a friendly hostname — primary source is the DHCP lease table (client-provided hostname from option 12), falling back to reverse DNS (`getent hosts`). mDNS/Avahi is not required.
- **FR-023** Devices with no admin or user rule are shown in the UI as "unmanaged" — they route per the global default but are visible
- **FR-024** Allow admin to assign a persistent friendly name to any device (overrides auto-detected hostname)
- **FR-025** Detect when a known device changes IP (DHCP renewal) and update policy routing accordingly

#### 3.1.4 Kill Switch & Fallback

- **FR-030** When a VPN tunnel goes down, automatically fall back to direct internet for affected devices (soft fallback — no traffic blocking)
- **FR-031** Log and surface a notification/alert when a fallback event occurs ("Device X fell back to direct internet — UK tunnel was unreachable")
- **FR-032** When the tunnel recovers, automatically restore the device routing to its configured VPN without user intervention
- **FR-033** Fallback behaviour is configurable per-device: admin can set a device to `block` mode instead of `fallback` (e.g. for a privacy-sensitive device)

#### 3.1.5 Temporary & Scheduled Routing

- **FR-040** Support one-shot temporary overrides: "use US tunnel for the next 2 hours, then revert to my default"
- **FR-041** Support schedule-based rules: "use UK tunnel every day from 20:00–23:00, otherwise use my default"
- **FR-042** Support manual toggle: user turns on a rule with no expiry, manually turns it off
- **FR-043** Schedules are defined in local time; the daemon must be timezone-aware
- **FR-044** When a temporary rule expires or a schedule ends, the previous permanent rule is restored
- **FR-045** Multiple schedules can exist per device but must not overlap — the UI must validate and warn on conflict

#### 3.1.6 DNS

- **FR-050** DNS is a network-wide service, independent of routing policy. Any device using the Wardnet box as its DNS server receives ad blocking and DNS leak prevention regardless of whether its traffic is VPN-routed or direct
- **FR-051** Force DNS queries for VPN-routed devices through the active tunnel's DNS server (DNS leak prevention) — DNS requests must not exit through the ISP for tunnelled devices
- **FR-052** For direct-routed devices, DNS queries are still handled locally by Wardnet's resolver, ensuring ad blocking applies and DNS is not exposed to the ISP unfiltered
- **FR-053** Run a local DNS resolver (Unbound or similar) that handles per-device forwarding based on routing policy — tunnel DNS for VPN devices, upstream DoH/DoT for direct devices
- **FR-054** Integrate an ad/tracker blocklist (similar to Pi-hole) applied to all DNS queries passing through Wardnet, for all devices
- **FR-055** Allow admin to enable/disable ad blocking globally or per device
- **FR-056** Allow admin to add custom blocklist sources (URL to a hosts/domain blocklist)
- **FR-057** Blocked domain queries return NXDOMAIN; the UI shows a blocked query count per device

#### 3.1.7 DHCP Server

This is the mechanism that makes all other routing work without per-device manual configuration.

- **FR-058** Wardnet runs its own DHCP server on the LAN interface, advertising itself as the default gateway (option 3) and DNS server (option 6) for all managed devices
- **FR-059** DHCP leases must include the router's IP as a secondary entry in option 3 — clients that support multiple gateways (Linux/NetworkManager, some Windows versions) fall back to the router automatically if the Pi is unreachable
- **FR-060** Default lease time: 10 minutes — short enough for fast recovery after a Pi restart, long enough to not flood the network
- **FR-061** Support static DHCP reservations: bind a MAC address to a fixed IP, ensuring per-device routing rules remain stable across DHCP renewals
- **FR-062** DHCP conflict detection: if another DHCP server is detected on the network after setup, surface an immediate admin alert — two DHCP servers on the same subnet will break the network
- **FR-063** Log all lease assignments and renewals, correlating MAC → IP → device identity in the device registry
- **FR-064** For routers where DHCP cannot be disabled (locked ISP devices), the setup wizard detects this and switches to per-device static configuration mode, generating tailored instructions per device type

#### 3.1.8 Gateway Resilience & Failover

- **FR-065** Configure the Linux hardware watchdog on install — if wardnetd stops responding, the kernel reboots the Pi within 15 seconds
- **FR-066** systemd unit must use Restart=always and RestartSec=2s — daemon restarts immediately on crash before the watchdog fires
- **FR-067** Boot sequence must be optimised at install time (disable unnecessary services, target network-online.target early) — full Wardnet restoration after reboot must complete within 30 seconds
- **FR-068** On any graceful shutdown or reboot (via UI, CLI, or OS), wardnetd must execute a GARP (Gratuitous ARP) sequence before stopping:
  - Broadcast ARP reply announcing the gateway IP is now at the router's MAC
  - Wait 500ms, repeat once
  - Then proceed with shutdown/reboot
  - All managed devices immediately re-point their ARP cache at the real router, maintaining internet connectivity during the Pi's downtime
- **FR-069** On startup, broadcast a GARP reclaiming the gateway role: announce the gateway IP is now at the Pi's MAC — restores routing through Wardnet without waiting for device ARP caches to expire naturally
- **FR-070** Persist the router's MAC address to disk during setup — the GARP failover sequence must not depend on a live network scan at shutdown time (must work even during a crash-triggered reboot)
- **FR-071** Detect on startup whether the previous shutdown was graceful (flag file present) or unclean (flag file absent) — unclean shutdowns surfaced as an informational warning in the dashboard
- **FR-072** The web UI must expose a "Safe Reboot" button that triggers the graceful GARP sequence before rebooting — this must be the prominent, default reboot action, not buried in a settings page
- **FR-073** The web UI must expose a "Safe Shutdown" button with the same GARP-first behaviour, with a clear warning that internet will be unavailable until the Pi is powered back on
- **FR-074** wctl reboot and wctl shutdown CLI commands must invoke the graceful sequence, equivalent to the UI buttons

---

### 3.2 Management API

A local REST + WebSocket API served by the daemon (or a sidecar process), consumed by the web UI, mobile app, and CLI.

- **FR-060** ~~All API communication is over HTTPS (self-signed cert on first boot, with option to provide own cert)~~ **Decision: HTTP only.** This is a LAN-only appliance (like Pi-hole, Home Assistant). TLS adds complexity (cert management, trust store issues) with no real security benefit on a local network. If users need HTTPS they can put a reverse proxy in front.
- **FR-061** Authentication via session tokens (username/password login); sessions expire after configurable idle timeout
- **FR-062** Two roles: `admin` (full access) and `user` (access scoped to their registered devices only)
- **FR-063** WebSocket endpoint for real-time push: device status changes, tunnel health events, fallback alerts
- **FR-064** REST endpoints required (minimum):

| Method | Path                           | Description                           |
|--------|--------------------------------|---------------------------------------|
| GET    | /api/devices                   | List all devices                      |
| GET    | /api/devices/:id               | Device detail + current routing rule  |
| PUT    | /api/devices/:id/rule          | Set permanent routing rule            |
| POST   | /api/devices/:id/temporary     | Set temporary override                |
| POST   | /api/devices/:id/schedule      | Add schedule rule                     |
| DELETE | /api/devices/:id/schedule/:sid | Remove schedule                       |
| GET    | /api/tunnels                   | List all tunnels + health             |
| POST   | /api/tunnels                   | Add new tunnel (import config)        |
| DELETE | /api/tunnels/:id               | Remove tunnel                         |
| GET    | /api/providers                 | List registered VPN providers         |
| POST   | /api/providers/:id/setup       | Guided provider setup (auth + config) |
| GET    | /api/dns/stats                 | Blocked query count, per device       |
| PUT    | /api/dns/blocklists            | Update blocklist config               |
| GET    | /api/system/status             | CPU, memory, uptime, version          |
| POST   | /api/auth/login                | Authenticate                          |
| POST   | /api/auth/logout               | Invalidate session                    |
| GET    | /api/dhcp/leases               | List active DHCP leases               |
| PUT    | /api/dhcp/reservations         | Add/update static DHCP reservation    |
| DELETE | /api/dhcp/reservations/:mac    | Remove static DHCP reservation        |
| POST   | /api/system/reboot             | Trigger graceful GARP-first reboot    |
| POST   | /api/system/shutdown           | Trigger graceful GARP-first shutdown  |

---

### 3.3 Web Management UI

Served directly from the Pi over HTTP on port 7411 (default). Accessible from any browser on the local network.

#### 3.3.1 First-Run Setup Wizard

- **FR-070** On first boot, the web UI shows a setup wizard before anything else
- **FR-071** Wizard steps:
  1. Set admin username and password
  2. Configure the Pi's static IP (or confirm DHCP reservation)
  3. Network onboarding mode selection — wizard detects whether a DHCP server is already active on the network, guides the user to disable it on their router (with model-specific instructions for common routers), then activates Wardnet's DHCP server and verifies at least one device has received a lease before proceeding. If the router is locked, switches to per-device fallback mode with generated instructions.
  4. Router MAC discovery — wizard silently pings the router gateway and records its MAC address for GARP failover. No user interaction required; surfaced only if it fails.
  5. Add first VPN tunnel (BYO config upload or guided provider setup)
  6. Set global default routing policy
  7. Confirm setup and reach the dashboard
- **FR-072** Wizard must be completable without touching a terminal — entirely browser-based

#### 3.3.2 Dashboard

- **FR-080** Show all known devices in a list/grid: name, IP, current routing rule, connection status
- **FR-081** Each device shows a country flag + tunnel name if VPN-routed, or "Direct" badge if bypassing
- **FR-082** Show tunnel health summary: each configured tunnel with online/offline status and latency
- **FR-083** Show global stats: total devices managed, active tunnels, blocked DNS queries today
- **FR-084** Real-time updates via WebSocket — device status changes and fallback alerts appear without page refresh

#### 3.3.3 Device Management

- **FR-090** Admin view: full list of all devices with ability to name, assign routing rules, lock rules
- **FR-091** User view: only their own device(s) — identified by the IP/MAC of the browser's current device
- **FR-092** Routing rule editor per device:
  - Select tunnel (dropdown with country flags) or "Direct" or "Default"
  - Set as permanent, or set a temporary expiry duration
  - Add/edit/delete schedule rules with time picker
- **FR-093** Visual indicator when admin rule is overriding a user rule
- **FR-094** Device type icons: auto-detect and show icon for TV, phone, laptop, tablet, unknown

#### 3.3.4 Tunnel Management (Admin only)

- **FR-100** List all configured tunnels with status, latency, bytes in/out
- **FR-101** Add tunnel via:
  - Upload a WireGuard .conf file
  - Paste WireGuard config text
  - Guided provider setup: select a registered provider → authenticate → pick server/country → auto-generate and import WireGuard config (NordVPN in MVP, extensible to other providers)
- **FR-102** Remove tunnel — warns if any devices are actively using it and offers reassignment before deletion
- **FR-103** Test tunnel button — sends a test request through the tunnel and reports the exit IP and country

#### 3.3.5 DNS & Ad Blocking (Admin only)

- **FR-110** Toggle ad blocking globally
- **FR-111** Per-device ad blocking toggle
- **FR-112** Show total queries blocked today, top blocked domains
- **FR-113** Manage custom blocklist URLs (add/remove)
- **FR-114** Manual blocklist refresh button
- **FR-115** Visual indicator showing that ad blocking applies to direct-routed devices too, not just VPN-routed ones

#### 3.3.6 System Settings (Admin only)

- **FR-120** View system info: CPU usage, memory, uptime, PiRoute version, WireGuard version
- **FR-121** Configure global default routing policy (Direct / specific tunnel)
- **FR-122** User management: create additional user accounts, assign them to devices
- **FR-123** Export/import full configuration (backup/restore)
- **FR-124** Check for updates and view changelog
- **FR-125** View logs: daemon log, tunnel events, DNS query log (filterable)
- **FR-075** Display last shutdown type (graceful vs unclean) with timestamp — unclean shutdowns shown as a persistent warning banner until acknowledged
- **FR-076** Safe Reboot button — prominently placed, labelled clearly as the recommended way to restart the Pi
- **FR-077** Safe Shutdown button — same prominence, with confirmation dialog and consequence warning
- **FR-078** DHCP panel: view active leases (device, MAC, IP, expiry), manage static reservations, view lease history

---

### 3.4 CLI (wctl)

A command-line interface for power users and scripting. Communicates with the local API.

- **FR-130** `wctl status` — show tunnel health + device count
- **FR-131** `wctl devices` — list all devices and their current rules
- **FR-132** `wctl set <device> <tunnel|direct|default>` — set routing rule
- **FR-133** `wctl tunnels` — list tunnels
- **FR-134** `wctl tunnel add --file <path>` — import WireGuard config
- **FR-135** `wctl tunnel test <id>` — test tunnel and print exit IP
- **FR-136** `wctl logs [--follow]` — stream daemon logs
- **FR-137** `wctl backup` / `wctl restore <file>` — config backup/restore
- **FR-074a** `wctl reboot` — trigger graceful GARP-first reboot (equivalent to UI Safe Reboot)
- **FR-074b** `wctl shutdown` — trigger graceful GARP-first shutdown (equivalent to UI Safe Shutdown)
- **FR-138** CLI authenticates using an API token stored in `~/.wctl/token` (generated from web UI)
- **FR-139** JSON output mode: `--json` flag on all commands for scripting

---

### 3.5 Installation

- **FR-140** Provide a one-line install script: `curl -sSL https://get.wardnet.dev | bash`
- **FR-141** Script must: detect OS (Raspberry Pi OS / Debian/Ubuntu), install WireGuard, install Wardnet daemon + web UI, enable systemd services, and print the local URL to open
- **FR-142** Provide a pre-built SD card image for Raspberry Pi (Raspberry Pi OS Lite base) that boots directly into Wardnet — flash-and-boot experience for non-technical users
- **FR-143** On first boot from the SD image, Wardnet broadcasts itself via mDNS as `wardnet.local` so users can reach the setup wizard at `http://wardnet.local:7411` without knowing the IP
- **FR-144** Document manual installation steps for users who want to install on an existing system

---

## 4. Non-Functional Requirements

### 4.1 Performance

- **NFR-001** Routing policy changes must propagate within 2 seconds
- **NFR-002** The daemon must sustain ≥ 300 Mbps aggregate throughput on Raspberry Pi 4 (matching the hardware's WireGuard benchmark ceiling)
- **NFR-003** Web UI must load within 3 seconds on a local network connection
- **NFR-004** Device detection must register a new device within 10 seconds of its first packet passing through the Pi

### 4.2 Reliability

- **NFR-005** The daemon must auto-restart on crash (via systemd)
- **NFR-006** Tunnel reconnection must be attempted within 5 seconds of a detected disconnect
- **NFR-007** The system must survive a Pi reboot and fully restore all routing rules within 30 seconds of boot
- **NFR-008** Configuration must be persisted to disk and survive unexpected power loss (journaled writes, no in-memory-only state)
- **NFR-009** Linux hardware watchdog configured at install — if wardnetd hangs without crashing, kernel reboots the Pi within 15 seconds
- **NFR-010** Graceful reboot/shutdown GARP sequence must complete within 1 second before the system proceeds — hard requirement, not best-effort
- **NFR-011** For graceful restarts, internet interruption for managed devices must be under 2 seconds end-to-end

### 4.3 Security

- **NFR-010** ~~Web UI served over HTTPS only — HTTP redirects to HTTPS~~ **Decision: HTTP only.** See FR-060.
- **NFR-011** Admin password must be set during first-run wizard — no default credentials
- **NFR-012** API tokens must be rotatable without requiring re-login on all sessions
- **NFR-013** WireGuard private keys must be stored with 600 permissions, never exposed via API or UI
- **NFR-014** The management UI must not be reachable from the WAN by default (bind to LAN interface only)
- **NFR-015** All dependencies must be pinned and verifiable (reproducible builds)

### 4.4 Compatibility

- **NFR-020** Supported hardware: Raspberry Pi 4 (primary), Raspberry Pi 5, and any Debian/Ubuntu ARM64 or x86_64 system
- **NFR-021** Minimum OS: Debian 11 (Bullseye) / Ubuntu 22.04
- **NFR-022** Web UI must work in: Chrome, Firefox, Safari, Edge — latest two major versions
- **NFR-023** WireGuard version: kernel module preferred, userspace fallback (wireguard-go) for older kernels

### 4.5 Usability

- **NFR-025** A non-technical user must be able to complete first-run setup in under 10 minutes using the wizard
- **NFR-026** Changing a device's routing rule must require no more than 3 clicks/taps from the dashboard
- **NFR-027** Every destructive action (delete tunnel, remove device) must require confirmation

---

## 5. Technical Stack Recommendations

### 5.1 Core Daemon

**Language: Rust**

Rationale: The daemon is the right place for Rust. It owns the hot path — syscall-level routing table manipulation, nftables rule generation, WireGuard interface lifecycle, and packet-level device detection. Rust's zero-cost abstractions, memory safety without GC pauses, and strong `async` story (Tokio) make it well-suited. Key crates:

- `tokio` — async runtime
- `wireguard-control` — WireGuard interface management via kernel netlink
- `neli` / `rtnetlink` — netlink for routing table manipulation
- `pnet` — packet capture for device detection (passive ARP/traffic sniffing)
- `axum` — embedded REST + WebSocket API server
- `serde` / `serde_json` — config serialisation
- `sqlx` + SQLite — device registry and policy persistence

### 5.2 Web UI

**Framework: React + TypeScript**  
**Build: Vite**  
**Styling: Tailwind CSS**  
**State: Zustand or TanStack Query**  
**Routing: React Router**

The web UI is served as a static bundle embedded in the daemon binary (using `rust-embed`) — no separate web server process needed.

### 5.3 CLI

**Language: Rust** (shares types with the daemon via a shared crate)  
Output formatted with `tabled` for human-readable tables, `serde_json` for `--json` mode.

### 5.4 Mobile App (v2)

**Framework: React Native** — shares business logic and API client with the web UI.

### 5.5 DNS Resolver

**Unbound** — battle-tested, supports per-interface forwarding zones, integrates cleanly with blocklists. Managed by the daemon (config files generated and reloaded dynamically).

---

## 6. Architecture — Component Diagram

```
┌──────────────────────────────────────────────────────────┐
│                   Wardnet Daemon (Rust)                  │
│                                                          │
│  ┌──────────────┐  ┌──────────────┐  ┌────────────────┐  │
│  │ Tunnel Mgr   │  │ Policy Engine│  │ Device Registry│  │
│  │ (WireGuard   │  │ (ip rule /   │  │ (SQLite +      │  │
│  │  interfaces) │  │  nftables)   │  │  ARP watcher)  │  │
│  └──────┬───────┘  └──────┬───────┘  └───────┬────────┘  │
│         │                 │                  │           │
│  ┌──────▼─────────────────▼──────────────────▼────────┐  │
│  │                Internal Event Bus                  │  │
│  └────────────────────────┬───────────────────────────┘  │
│                           │                              │
│  ┌────────────────────────▼───────────────────────────┐  │
│  │            REST + WebSocket API (axum)             │  │
│  │             + Static Web UI (embedded)             │  │
│  └────────────────────────────────────────────────────┘  │
│                                                          │
│  ┌────────────────────────────────────────────────────┐  │
│  │         DNS Manager (Unbound + blocklists)         │  │
│  └────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────┘

External consumers:
  Browser / Web UI  ──► HTTP :7411 (LAN only)
  wctl CLI          ──► HTTP :7411 (LAN only, token auth)
  Mobile App (v2)   ──► HTTP :7411 (LAN only)
```

---

## 7. Data Model (Core Entities)

### Device
```
id           UUID
mac          String (unique)
name         String (admin-assigned, nullable)
hostname     String (auto-detected, nullable)
device_type  Enum (tv, phone, laptop, tablet, unknown)
first_seen   Timestamp
last_seen    Timestamp
last_ip      String
admin_rule   RoutingRule (nullable — admin override)
user_rule    RoutingRule (nullable — self-service)
```

### RoutingRule
```
target       Enum (tunnel | direct | default)
tunnel_id    UUID (nullable, when target=tunnel)
kind         Enum (permanent | temporary | scheduled)
expires_at   Timestamp (nullable, for temporary)
schedules    Vec<Schedule> (for scheduled kind)
created_by   Enum (admin | user)
```

### Schedule
```
id           UUID
days         Vec<Weekday> (Mon–Sun bitmask)
time_start   Time (local)
time_end     Time (local)
timezone     String
```

### Tunnel
```
id           UUID
label        String
country_code String (ISO 3166-1 alpha-2)
provider     String (nullable)
interface    String (wg_ward0, wg_ward1, …)
endpoint     String
public_key   String
last_handshake  Timestamp
status       Enum (up | down | connecting)
bytes_tx     u64
bytes_rx     u64
```

### SystemConfig (additional fields)
```
router_mac        String   — persisted during setup, used for GARP failover
last_shutdown     Enum (graceful | unclean | unknown)
last_shutdown_at  Timestamp
```

### DhcpReservation
```
mac          String (unique)
ip           String
label        String (nullable — friendly name)
created_at   Timestamp
```

---

## 8. Suggested Features Not Yet Mentioned

These are patterns observed in comparable products (Firewalla, GL.iNet) and community requests that would add meaningful value:

### 8.1 Speed Test per Tunnel
A built-in tunnel speed test that measures actual throughput through each configured VPN exit. Helps users choose the best endpoint.

### 8.2 Multi-Tunnel Failover Group
Allow grouping multiple tunnels (e.g. US-1 and US-2) into a failover group. If US-1 goes down, traffic automatically moves to US-2 before falling back to direct.

### 8.3 Domain-Based Routing Rules
In addition to per-device rules, allow routing rules based on destination domain — e.g. "all traffic to netflix.com from any device uses the UK tunnel." Implemented via DNS response interception + ip rule.

### 8.4 Split-Tunnel per App (Future)
On devices that run the PiRoute companion agent (Linux/macOS), route per-application rather than per-device. Out of scope for v1 but worth noting in the roadmap.

### 8.5 Dynamic DNS Support
If the Pi's WAN IP changes (ISP DHCP), auto-update a DDNS record. Useful if users want to reach the management UI while away from home (paired with WireGuard inbound access).

### 8.6 Traffic Usage Dashboard
Monthly traffic summary per device and per tunnel. Useful for households on metered connections.

### 8.7 Guest Network Isolation
Mark certain devices as "guest" — they can only route through specific tunnels and are isolated from other LAN devices at the policy level.

### 8.8 Notification Webhooks
POST to a webhook URL (Discord, Slack, ntfy.sh) on events: tunnel down, device fell back to direct, new unrecognised device appeared.

---

## 9. Phased Delivery Roadmap

### Phase 1 — Foundation (MVP)
- Core daemon: WireGuard tunnel management, policy routing engine, device detection
- DHCP server: gateway advertisement, static reservations, conflict detection, lease logging
- Gateway resilience: hardware watchdog, GARP failover on shutdown/startup, graceful reboot/shutdown
- VPN provider integration: pluggable `VpnProvider` trait, provider registry, NordVPN as first implementation
- REST API with auth (admin session + API key + unauthenticated self-service by IP)
- Web UI: first-run wizard (with DHCP onboarding + router MAC discovery), device list, routing rule assignment, tunnel management, guided provider setup, safe reboot/shutdown, DHCP panel
- CLI: status, devices, set rule, tunnel add/remove, reboot, shutdown
- DNS leak prevention
- Install script + SD card image

### Phase 2 — UX Polish & Advanced Features
- Temporary routing + schedule-based rules
- Fallback alerts and tunnel health notifications
- Ad blocking (blocklist integration)

### Phase 3 — Power Features
- Domain-based routing rules
- Multi-tunnel failover groups
- Traffic usage dashboard
- Additional VPN providers (Mullvad, ProtonVPN, IVPN, Surfshark — community contributions welcome via VpnProvider trait)
- Notification webhooks
- Mobile app (React Native)

### Phase 4 — Ecosystem
- Plugin/extension API for community contributions
- Split-tunnel companion agent (Linux/macOS)
- Guest network policy
- Dynamic DNS support
