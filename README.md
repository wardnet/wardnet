# Wardnet

[![CI](https://github.com/pedromvgomes/wardnet/actions/workflows/ci.yml/badge.svg)](https://github.com/pedromvgomes/wardnet/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/pedromvgomes/wardnet/branch/main/graph/badge.svg)](https://codecov.io/gh/pedromvgomes/wardnet)
[![Rust](https://img.shields.io/badge/rust-1.94-orange.svg)](https://www.rust-lang.org)
[![Rust Report Card](https://rust-reportcard.xuri.me/badge/github.com/pedromvgomes/wardnet)](https://rust-reportcard.xuri.me/report/github.com/pedromvgomes/wardnet)
[![Security Audit](https://github.com/pedromvgomes/wardnet/actions/workflows/security.yml/badge.svg)](https://github.com/pedromvgomes/wardnet/actions/workflows/security.yml)
[![Known Vulnerabilities](https://snyk.io/test/github/pedromvgomes/wardnet/badge.svg)](https://snyk.io/test/github/pedromvgomes/wardnet)
[![Dependabot](https://badgen.net/github/dependabot/pedromvgomes/wardnet)](https://github.com/pedromvgomes/wardnet/pulls?q=is%3Apr+author%3Aapp%2Fdependabot)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Wardnet is a self-hosted network privacy gateway that runs on a Raspberry Pi. It sits alongside an existing home or small-office router and acts as the warden of every device's connection to the internet — encrypting traffic, blocking ads and trackers at the DNS level, and giving you per-device control over how each device connects.

Devices that cannot run VPN software themselves (smart TVs, consoles, IoT) are fully protected at the gateway level.

## Project Status

**Phase 1 (MVP) — in progress.** Wardnet is being daily-driven on a single Raspberry Pi at home but is **not yet a finished product**. If you want to try it now, expect to read the source occasionally and to be on hand to debug a Pi. Detailed milestone progress lives in [`implementation-docs/plans/phase-1-mvp-implementation-plan.md`](implementation-docs/plans/phase-1-mvp-implementation-plan.md); known bugs and feature ideas in [`implementation-docs/follow-ups.md`](implementation-docs/follow-ups.md).

**What works today**

- WireGuard tunnel CRUD with NordVPN provider integration (single-server selection)
- Per-device routing policy (direct / specific tunnel / network default), applied via `ip rule` + nftables
- Built-in DHCP server with leases, static reservations, and conflict detection
- Device detection (ARP, OUI lookup, departure tracking)
- DNS leak prevention on tunnel-routed devices (port 53 DNAT to the tunnel's DNS)
- Web UI for dashboard, devices, tunnels, DHCP
- REST + WebSocket API with API-key auth, CLI tool (`wctl`)
- OpenTelemetry metrics export and Pyroscope continuous profiling (opt-in)

**What's not done yet**

- DNS server / network-wide ad blocking (Pi-hole replacement) — UI placeholder only
- Gateway resilience (GARP failover, hardware watchdog, graceful shutdown)
- Web UI: setup wizard, full DHCP panel polish
- Installation packaging (`install.sh`, release workflow)
- Mobile app, kill switch per device, scheduled routing

**Known caveats when in use**

- After switching a device *off* a tunnel (VPN → direct, or between tunnels), the device's existing TCP sockets may stay stuck for ~30–60s while their stack times out. Routing on the Pi is correct immediately; toggling Wi-Fi on the device fixes it instantly. See pedromvgomes/wardnet#77.
- A NordVPN server selected at tunnel-creation time may be unhealthy when you actually try to use it; the daemon currently still marks such tunnels as "up" even when no WireGuard handshake has ever completed. See pedromvgomes/wardnet#79 and pedromvgomes/wardnet#80.

## Features

- **Per-device routing** — route each device through a specific VPN tunnel, direct internet, or the network default
- **WireGuard tunnels** — lazy on-demand tunnels that start when needed and tear down after idle timeout
- **DNS-level ad/tracker blocking** — applied to all managed devices regardless of routing policy
- **Admin + self-service model** — admin controls shared devices, users control their own (auto-detected by IP)
- **Web UI** — manage everything from a single dashboard
- **Single binary** — daemon embeds the web UI and serves it directly, no separate web server needed

## Architecture

```
source/
├── daemon/                 # Rust workspace
│   └── crates/
│       ├── wardnet-types/  # Shared types (devices, tunnels, routing, events)
│       ├── wardnetd/       # Daemon binary (API server, DB, embedded web UI)
│       └── wctl/           # CLI tool
└── web-ui/                 # React + TypeScript frontend
```

### Daemon (wardnetd)

Layered architecture with dependency injection via traits:

- **Repository layer** — data access traits + SQLite implementations
- **Service layer** — business logic traits + implementations (auth, device, tunnel, system)
- **API layer** — thin axum HTTP handlers that delegate to services
- **Infrastructure** — EventPublisher (event bus), WireGuardOps (tunnel control), KeyStore (private key files), TunnelMonitor (background stats/health)
- **State** — holds service trait objects + event publisher, injected at startup

### Web UI

React 19 + Vite 7 + Tailwind CSS 4 + TanStack Query 5 + React Router 7. Built artifacts are embedded into the daemon binary via `rust-embed`.

## Tech Stack

| Component       | Technology                                            |
|-----------------|-------------------------------------------------------|
| Daemon          | Rust 1.94, axum 0.8, SQLite (sqlx 0.8)                |
| Web UI          | React 19, TypeScript 5.9, Vite 7, Tailwind CSS 4      |
| Package manager | Yarn 4                                                |
| Auth            | argon2 (passwords/API keys), SHA-256 (session tokens) |
| Tunnels         | WireGuard                                             |
| Target          | Raspberry Pi (aarch64-linux-gnu), Linux x86_64        |

## Getting Started

### Prerequisites

- Rust 1.94+ (pinned via `rust-toolchain.toml`)
- Node.js 25+
- Yarn 4 (enabled via Corepack)
- **Daemon checks on macOS**: Podman or Docker (the daemon uses Linux-only kernel interfaces and cannot compile natively on macOS — `make check-daemon` runs checks inside a Linux container automatically)

### First-time setup

```bash
make init
```

This installs the Rust cross-compilation target, the aarch64-linux-gnu linker (via Homebrew on macOS or apt on Linux), and yarn dependencies.

### Build

```bash
# Build everything (web UI + daemon for host target)
make build

# Build only the web UI
make build-web

# Build only the daemon
make build-daemon

# Cross-compile for Raspberry Pi (aarch64-linux-gnu)
make build-pi
```

### Run

```bash
# Run with defaults (port 7411, SQLite at ./wardnet.db)
./source/daemon/target/release/wardnetd

# Run with custom config
./source/daemon/target/release/wardnetd --config /path/to/wardnet.toml

# Run without real network backends (for local development)
./source/daemon/target/release/wardnetd --mock-network --verbose
```

### Deploy to Raspberry Pi

```bash
# Build and deploy via SSH (default: wardnet@gateway)
make deploy

# Override target host
make deploy PI_HOST=192.168.1.50
```

### Development

```bash
# Run all checks (format, lint, tests for web + daemon)
# On macOS, daemon checks run inside a Linux container (podman/docker)
make check

# Run tests only (Linux — native)
cd source/daemon && cargo test --workspace

# Run tests only (macOS — via container, auto-detects podman or docker)
make check-daemon

# Line-coverage summary (macOS — via container; Linux — native)
make coverage-daemon

# CLI
cd source/daemon && cargo run -p wctl -- status
```

### Available Make targets

Run `make help` for the full list:

| Target           | Description                                            |
|------------------|--------------------------------------------------------|
| `make init`      | Install all dev dependencies                           |
| `make build`     | Build web UI + daemon (host target)                    |
| `make build-web` | Build web UI only                                      |
| `make build-daemon` | Build daemon for host target                        |
| `make build-pi`  | Cross-compile daemon for Pi (aarch64-linux-gnu)        |
| `make check`     | Run all checks (web + daemon)                          |
| `make check-web` | Typecheck + lint + format check for web UI             |
| `make check-daemon` | Format + clippy + tests (container on macOS)        |
| `make coverage-daemon` | Line-coverage summary (container on macOS)       |
| `make run-pi`    | Cross-compile, deploy, and run on a Raspberry Pi       |
| `make deploy-prod` | Production deploy to a Raspberry Pi                  |
| `make clean`     | Clean all build artifacts                              |

## CI

GitHub Actions pipeline using the same Makefile targets:

1. **Check Web** — `make check-web`
2. **Build Web** — `make build-web`
3. **Check Daemon** — `make check-daemon` (runs on Ubuntu)
4. **Build Daemon** — `make build-daemon` (x86_64 Linux) and `make build-pi` (aarch64 Linux)

## License

MIT
