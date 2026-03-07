# Wardnet

Wardnet is a self-hosted network privacy gateway that runs on a Raspberry Pi. It sits alongside an existing home or small-office router and acts as the warden of every device's connection to the internet — encrypting traffic, blocking ads and trackers at the DNS level, and giving you per-device control over how each device connects.

Devices that cannot run VPN software themselves (smart TVs, consoles, IoT) are fully protected at the gateway level.

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
- **Service layer** — business logic traits + implementations (auth, device, system)
- **API layer** — thin axum HTTP handlers that delegate to services
- **State** — holds service trait objects, injected at startup

### Web UI

React 19 + Vite 7 + Tailwind CSS 4 + TanStack Query 5 + React Router 7. Built artifacts are embedded into the daemon binary via `rust-embed`.

## Tech Stack

| Component | Technology |
|-----------|-----------|
| Daemon | Rust 1.94, axum 0.8, SQLite (sqlx 0.8) |
| Web UI | React 19, TypeScript 5.9, Vite 7, Tailwind CSS 4 |
| Package manager | Yarn 4 |
| Auth | argon2 (passwords/API keys), SHA-256 (session tokens) |
| Tunnels | WireGuard |
| Target | Raspberry Pi (aarch64), Linux x86_64, macOS aarch64 |

## Getting Started

### Prerequisites

- Rust 1.94+ (pinned via `rust-toolchain.toml`)
- Node.js 25+
- Yarn 4 (enabled via Corepack)

### Build

```bash
# Build the web UI
cd source/web-ui
yarn install
yarn build

# Build the daemon (embeds web UI dist/)
cd source/daemon
cargo build --release
```

### Run

```bash
# Run with defaults (port 7411, SQLite at ./wardnet.db)
./target/release/wardnetd

# Run with custom config
./target/release/wardnetd --config /path/to/wardnet.toml

# Run with verbose logging
./target/release/wardnetd --verbose
```

### Development

```bash
# Web UI dev server (port 7412, proxies API to daemon on 7411)
cd source/web-ui
yarn dev

# Run daemon
cd source/daemon
cargo run -p wardnetd -- --verbose

# Run tests
cargo test --workspace

# CLI
cargo run -p wctl -- status
```

## CI

GitHub Actions pipeline with 4 jobs:

1. **Build Web** — type-check, lint, format check, build
2. **Build Daemon (x86_64-unknown-linux-gnu)** — fmt, clippy, tests, release build
3. **Build Daemon (aarch64-apple-darwin)** — fmt, clippy, tests, release build
4. **Build Daemon (aarch64-unknown-linux-gnu)** — cross-compiled release build

## License

MIT
