# wardnet-bridge

HTTP bridge service for wardnet installations — handles DDNS, ACME DNS-01 credential proxying, and installation lifecycle management.

## Overview

The bridge is a lightweight Axum / SQLite / Tokio microservice that acts as the control-plane for wardnet Raspberry Pi installations. It runs on a public VM in each region and is always placed behind a reverse proxy (Caddy in production). Pi devices communicate with it to:

1. **Register** — claim a subdomain slug, prove ownership of an Ed25519 key-pair, and receive a bearer token.
2. **Update IP** — push their current public IPv4 address; the bridge upserts a Cloudflare A record for `<slug>.my.<region>.wardnet.network`.
3. **Provision ACME** — store and delete the Cloudflare TXT record needed for DNS-01 Let's Encrypt certificate issuance.
4. **Deregister** — delete the installation and its Cloudflare records.

## Security model

| Mechanism | Detail |
|---|---|
| **Registration PoW** | SHA-256(nonce‖name‖pubkey‖proof) must have ≥ 24 leading zero bits. Prevents sybil registration. |
| **Bearer token** | 32 random bytes returned once at registration. The bridge stores only `SHA-256(token)`. |
| **Ed25519 request signing** | Every authenticated request is signed over `"METHOD\npath_and_query\ntimestamp\nhex-sha256(body)"`. |
| **Replay protection** | Signed requests include a Unix timestamp (±60 s window); `(install_id, timestamp, body_hash)` tuples are cached in `ReplayCache` for 120 s. |
| **IP binding** | PoW challenges are IP-bound; a different client IP cannot redeem a challenge issued to another address. |
| **Body size guard** | All requests are buffered to 1 MiB max before any auth check — prevents memory exhaustion on unauthenticated endpoints. |
| **Path gate** | DB token lookup is only attempted for `/v1/installs/*` paths, blocking a DoS vector where an attacker forces DB queries on public endpoints. |
| **Rate limiting** | 20 challenges / IP / hour; 3 registrations / IP / 24 h. |
| **Reserved IP filter** | `PUT /v1/installs/:id/ip` rejects RFC 1918, loopback, link-local, and documentation-range addresses. |
| **Trusted proxy** | `X-Forwarded-For` is only trusted when the TCP peer is a loopback address (i.e. running behind Caddy on the same host). |

## Architecture

```
                ┌────────────┐
   Pi ─HTTPS──▶ │   Caddy    │ ─ reverse proxy
                └─────┬──────┘
                      │ XFF: real client IP
                      ▼
               ┌─────────────┐
               │  auth_layer  │ ← body-size guard + Ed25519 + replay check
               └──────┬───────┘
                      │ Request + Install extension
                      ▼
               ┌─────────────┐        ┌──────────────────┐
               │  API routes  │ ──────▶│ InstallRepository│
               │  (Axum)      │        │ ChallengeRepository│
               └──────┬───────┘        └──────────────────┘
                      │                       │
                      ▼                       ▼
               ┌────────────┐        ┌──────────────────┐
               │ DnsProvider │        │  SQLite (WAL)    │
               │(Cloudflare) │        │  read/write pools│
               └────────────┘        └──────────────────┘
```

The `AppState` is a cheap `Arc`-clone that carries:
- `Config` — loaded from environment at startup
- `Arc<dyn InstallRepository>` — SQLite or mock
- `Arc<dyn ChallengeRepository>` — SQLite or mock
- `Arc<dyn DnsProvider>` — Cloudflare REST or mock
- `Arc<ReplayCache>` — in-process replay window

## API surface

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| `GET` | `/health` | — | Liveness probe |
| `GET` | `/v1/register/challenge` | — | Issue a PoW challenge (rate-limited) |
| `POST` | `/v1/register` | — | Register a new installation |
| `GET` | `/v1/names/:name/available` | — | Check subdomain availability |
| `PUT` | `/v1/installs/:id/ip` | Bearer + Ed25519 | Update public IP / upsert A record |
| `POST` | `/v1/installs/:id/acme` | Bearer + Ed25519 | Provision ACME TXT record |
| `DELETE` | `/v1/installs/:id/acme` | Bearer + Ed25519 | Remove ACME TXT record |
| `DELETE` | `/v1/installs/:id` | Bearer + Ed25519 | Deregister installation |

An OpenAPI document is generated at build time via `utoipa` and `utoipa-axum`.

## Database

SQLite with WAL journaling and `INCREMENTAL` auto-vacuum. Migrations live in `migrations/`. The pool is split:
- `write` pool — 1 connection, serialises all mutations.
- `read` pool — up to 5 connections for `SELECT`-only queries.

In-memory mode (`DATABASE_URL=":memory:"`) is used in tests. Each test run gets a unique shared-cache URI so parallel workers don't collide.

## Configuration

All configuration is read from environment variables at startup. Missing required variables cause an immediate, human-readable error.

| Variable | Required | Default | Description |
|---|---|---|---|
| `DATABASE_URL` | ✓ | — | SQLite file path or `":memory:"` |
| `CLOUDFLARE_API_TOKEN` | ✓ | — | CF token scoped to DNS:Edit on the zone |
| `CLOUDFLARE_ZONE_ID` | ✓ | — | Cloudflare zone ID for `wardnet.network` |
| `REGION` | ✓ | — | Short region label, e.g. `"us"` or `"eu"` |
| `SUBDOMAIN_PARENT` | ✓ | — | DNS parent, e.g. `"my.us.wardnet.network"` |
| `LISTEN_ADDR` | — | `127.0.0.1:8080` | TCP bind address (loopback default — always behind Caddy) |

**Never put `CLOUDFLARE_API_TOKEN` in code.** In production it is injected via GitHub secrets (`BRIDGE_DEPLOY_KEY`, `BRIDGE_DEPLOY_HOST`) into the systemd unit environment file on the VM.

## Building and running

The bridge has no Linux-specific library dependencies (no netfilter, iptables, or netlink), so it builds and runs natively on macOS for development:

```sh
# From repo root
make check-bridge   # clippy + tests
make build-bridge   # release binary

# Or directly
cargo test   --manifest-path source/bridge/Cargo.toml
cargo clippy --manifest-path source/bridge/Cargo.toml --all-targets -- -D warnings
```

For a local smoke test, export the required env vars and run:

```sh
DATABASE_URL=":memory:" \
CLOUDFLARE_API_TOKEN=dummy \
CLOUDFLARE_ZONE_ID=dummy \
REGION=dev \
SUBDOMAIN_PARENT=dev.wardnet.local \
cargo run --manifest-path source/bridge/Cargo.toml
```

## Crate layout

```
source/bridge/
├── src/
│   ├── main.rs              — binary entry point; env config + pool init + server bind
│   ├── lib.rs               — crate root; module declarations
│   ├── config.rs            — Config struct loaded from env
│   ├── state.rs             — AppState (Arc<Inner>), accessor methods
│   ├── error.rs             — ApiError enum → HTTP status + JSON body
│   ├── replay_cache.rs      — In-process replay window (HashMap + lazy expiry)
│   ├── db/
│   │   └── mod.rs           — DbPools (read + write SqlitePool), init()
│   ├── repository/
│   │   ├── mod.rs           — re-exports Install, RegistrationChallenge, traits
│   │   ├── install.rs       — InstallRepository trait + SqliteInstallRepository
│   │   └── challenge.rs     — ChallengeRepository trait + SqliteChallengeRepository
│   ├── auth/
│   │   └── middleware.rs    — auth_layer (body guard + Ed25519 + replay), AuthenticatedInstall extractor
│   ├── dns/
│   │   ├── mod.rs           — DnsProvider trait
│   │   └── cloudflare.rs    — CloudflareDnsProvider (REST API)
│   └── api/
│       ├── mod.rs           — router assembly, OpenAPI doc, middleware stack
│       ├── health.rs        — GET /health
│       ├── challenge.rs     — GET /v1/register/challenge, PoW helpers
│       ├── register.rs      — POST /v1/register
│       ├── names.rs         — GET /v1/names/:name/available
│       ├── ip.rs            — PUT /v1/installs/:id/ip
│       ├── acme.rs          — POST/DELETE /v1/installs/:id/acme
│       ├── deregister.rs    — DELETE /v1/installs/:id
│       └── validation.rs    — shared name + public-key validation
├── migrations/              — SQL migration files (sqlx-migrate)
└── Cargo.toml
```
