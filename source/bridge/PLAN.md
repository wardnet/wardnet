# Bridge traffic forwarding — implementation plan

Covers everything discussed: MySQL migration, active/active topology, SNI demuxer,
reverse-tunnel layer, and Android Private DNS (DoT) passthrough. Wardnetd daemon
changes (tunnel client, ACME ownership, DoT server) are explicitly out of scope
here and need their own issue.

---

## Context and starting point

The merged bridge (`source/bridge/`) is a control-plane-only service: DDNS
registration, IP updates, ACME TXT record management, and installation lifecycle.
It has no traffic-forwarding capability. The Cloudflare A record for
`<slug>.my.<region>.wardnet.network` points at the bridge VM/LBS, but arriving
connections go nowhere — there is no path from the bridge to the Pi.

Three work streams fix this:

1. **Database** — SQLite + Litestream → OCI MySQL (simplifies multi-writer,
   drops Litestream complexity).
2. **Tunnel layer** — Pi dials the bridge over an authenticated WebSocket;
   bridge splices inbound streams into that tunnel (TLS passthrough, so
   the private key never leaves the Pi).
3. **SNI demuxer** — port 443 and port 853 entry points on each bridge node
   that route `bridge.<REGION>.*` to Caddy (API cert) and `*.my.<REGION>.*`
   to the tunnel router.  Port 853 enables Android Private DNS (DoT).

Infrastructure decisions (active/active, sticky sessions, OCI LBS config) are
documented here but are ops work, not code changes in this repo.

---

## Phase 1 — SQLite → OCI MySQL

### Why

- MySQL is a shared external store, so all active/active nodes see the same data
  without Litestream or object-storage replication.
- OCI MySQL free tier: 1 OCPU / 16 GB / 50 GB storage + 50 GB backup, single
  instance.  Acceptable SPOF: bridge downtime = degraded service (new
  registrations and renewals fail), not outage (existing installs continue via
  their already-set Cloudflare records and valid certs).
- Removes the read/write pool split (MySQL handles concurrent connections
  natively); `DbPools` keeps its two-field struct but both fields point to the
  same pool, so no repository code changes.

### `Cargo.toml`

```toml
# before
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite", "uuid", "chrono", "migrate"] }

# after
sqlx = { version = "0.8", features = ["runtime-tokio", "mysql", "uuid", "chrono", "migrate"] }
```

Add to `[dev-dependencies]`:
```toml
testcontainers = "0.23"
testcontainers-modules = { version = "0.11", features = ["mysql"] }
```

### `src/db/mod.rs`

Drop all SQLite-specific types (`SqlitePool`, `SqliteConnectOptions`,
`SqliteJournalMode`, `SqliteAutoVacuum`, `SqliteSynchronous`, `SqlitePoolOptions`,
`Uuid`-based in-memory URI).

Replace with a single `MySqlPool` with sensible defaults:

```rust
use sqlx::mysql::{MySqlConnectOptions, MySqlPoolOptions};
use sqlx::MySqlPool;

const MAX_CONNECTIONS: u32 = 10;
const MIN_CONNECTIONS: u32 = 2;
const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct DbPools {
    pub read: MySqlPool,
    pub write: MySqlPool,
}

impl DbPools {
    pub fn single(pool: MySqlPool) -> Self {
        Self { read: pool.clone(), write: pool }
    }
}

pub async fn init(database_url: &str) -> anyhow::Result<DbPools> {
    let pool = MySqlPoolOptions::new()
        .max_connections(MAX_CONNECTIONS)
        .min_connections(MIN_CONNECTIONS)
        .acquire_timeout(ACQUIRE_TIMEOUT)
        .connect(database_url)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    tracing::info!(database_url, "database initialised");
    Ok(DbPools::single(pool))
}
```

### Migrations — rewrite for MySQL

`20260101000000_initial.sql`:

```sql
CREATE TABLE installs (
    id               CHAR(36)     PRIMARY KEY NOT NULL,
    name             VARCHAR(64)  NOT NULL UNIQUE,
    public_key       VARCHAR(64)  NOT NULL,
    token_hash       VARCHAR(64)  NOT NULL UNIQUE,
    ip               VARCHAR(45),
    cf_a_record_id   VARCHAR(64),
    cf_acme_record_id VARCHAR(64),
    created_at       DATETIME(3)  NOT NULL,
    updated_at       DATETIME(3)  NOT NULL
);

CREATE TABLE registration_log (
    id         BIGINT      NOT NULL AUTO_INCREMENT PRIMARY KEY,
    remote_ip  VARCHAR(45) NOT NULL,
    created_at DATETIME(3) NOT NULL,
    INDEX idx_registration_log_ip_time (remote_ip, created_at)
);
```

`20260528000000_registration_challenges.sql`:

```sql
CREATE TABLE registration_challenges (
    id          CHAR(36)     PRIMARY KEY NOT NULL,
    nonce       VARCHAR(64)  NOT NULL,
    difficulty  INT UNSIGNED NOT NULL,
    remote_ip   VARCHAR(45)  NOT NULL,
    created_at  DATETIME(3)  NOT NULL,
    expires_at  DATETIME(3)  NOT NULL,
    used_at     DATETIME(3),
    INDEX idx_challenges_ip_time    (remote_ip, created_at),
    INDEX idx_challenges_expires_at (expires_at)
);
```

### Repository layer

Two SQL dialect differences to fix in `src/repository/install.rs` and
`src/repository/challenge.rs`:

1. **Datetime literals**: replace `datetime('now', '-1 day')` / `datetime('now',
   '-1 hour')` with `NOW() - INTERVAL 1 DAY` / `NOW() - INTERVAL 1 HOUR`.
2. **Datetime storage**: columns are now `DATETIME(3)` (MySQL), not `TEXT` (ISO
   8601 string). SQLx maps `DateTime<Utc>` ↔ `DATETIME` natively with the
   `chrono` feature — no manual `to_rfc3339()` / `.parse()` calls needed;
   remove them.
3. **`difficulty` column**: `INT UNSIGNED` maps directly to `u32` — drop the
   `u32::try_from(row.difficulty)?` conversion.

### `src/config.rs`

- Rename the doc comment on `database_url` field from "SQLite file path or
  `:memory:`" to "MySQL DSN, e.g. `mysql://user:pass@host/wardnet_bridge`".
- Remove the `:memory:` special-case in `db::init`.

### Tests

Replace the in-memory SQLite path with `testcontainers`:

```rust
// tests/common/mod.rs  (new helper)
pub async fn test_db() -> DbPools {
    let container = testcontainers::runners::AsyncRunner::run(
        testcontainers_modules::mysql::Mysql::default()
    ).await.unwrap();
    let url = format!(
        "mysql://root@127.0.0.1:{}/test",
        container.get_host_port_ipv4(3306).await.unwrap()
    );
    db::init(&url).await.unwrap()
}
```

Each integration test that currently passes `DATABASE_URL=":memory:"` switches
to `test_db().await`. Docker is required in CI; the Makefile `check-bridge`
target gains a Docker availability check.

---

## Phase 2 — SNI demuxer

### Problem

`bridge.<REGION>.wardnet.network` (API), `*.my.<REGION>.wardnet.network` (Pi
HTTPS), and `*.my.<REGION>.wardnet.network:853` (Pi DNS-over-TLS) all arrive at
the same bridge nodes. The OCI NLB in TCP mode is L4-only — it cannot route by
SNI. Caddy terminates TLS for the bridge's own cert but cannot do TLS passthrough
to the tunnel router in its standard configuration.

### Solution

The bridge service spawns SNI demuxer tasks for **port 443** and **port 853**.
Both listeners share identical routing logic (peek SNI → dispatch); only the
destination port carried in the CONNECT frame differs.

```
OCI NLB TCP :443          OCI NLB TCP :853
      │                         │
      ▼                         ▼
[SNI demuxer :443]        [SNI demuxer :853]
      │                         │
  ┌───┴──────────┐          ┌───┴────────────┐
  │ BRIDGE_HOST  │ *.my.*   │ (always *.my.*)│
  ▼              ▼          ▼                │
[Caddy]   [TunnelRouter]  [TunnelRouter]     │
 API        dest_port=443   dest_port=853    │
            (HTTPS)         (DoT)            │
```

Port 853 connections are always `*.my.<REGION>.*` — the bridge's own hostname is
never used for DNS. Unknown SNI on port 853 is dropped immediately.

### Android Private DNS (DoT) user flow

1. User opens Android Settings → Network → Private DNS → enter hostname.
2. Hostname: `<slug>.my.<region>.wardnet.network`  (shown in the wardnet setup UI).
3. Android connects to port 853, presents the hostname in the TLS SNI.
4. SNI demuxer routes to the Pi's wardnetd DNS-over-TLS server via the tunnel.
5. wardnetd serves DNS responses with ad-blocking, custom rules, etc. applied.
6. No VPN required. Works system-wide on Android 9+.

This is complementary to WireGuard inbound (issue #266): DoT gives DNS-only
access without a VPN; WireGuard gives full network access.

### New config fields

| Variable | Required | Default | Description |
|---|---|---|---|
| `SNI_LISTEN_ADDR` | — | `0.0.0.0:443` | Where the HTTPS SNI demuxer binds |
| `DOT_LISTEN_ADDR` | — | `0.0.0.0:853` | Where the DoT SNI demuxer binds |
| `CADDY_ADDR` | — | `127.0.0.1:8443` | Where Caddy listens for API traffic |
| `BRIDGE_HOSTNAME` | ✓ | — | e.g. `bridge.use1.wardnet.network` |

`SUBDOMAIN_PARENT` (existing) is used as the wildcard pattern for Pi traffic
(`*.my.use1.wardnet.network`).

`DOT_LISTEN_ADDR` can be set to empty string `""` to disable DoT passthrough if
not yet needed.

### `src/sni/mod.rs` — implementation sketch

```rust
// Peek the first ~512 bytes (enough for a TLS ClientHello).
// Parse the SNI extension from the handshake record.
// No TLS state machine — read-only peek, then hand off the raw stream.

pub async fn run(config: Arc<Config>, tunnel_registry: Arc<TunnelRegistry>) {
    let listener = TcpListener::bind(&config.sni_listen_addr).await?;
    loop {
        let (stream, _peer) = listener.accept().await?;
        let config = config.clone();
        let registry = tunnel_registry.clone();
        tokio::spawn(async move {
            let (sni, stream) = peek_sni(stream).await?;
            if sni == config.bridge_hostname {
                splice(stream, &config.caddy_addr).await;
            } else if let Some(slug) = slug_from_sni(&sni, &config.subdomain_parent) {
                route_to_tunnel(stream, slug, &registry).await;
            }
            // unknown SNI: drop connection
        });
    }
}
```

`peek_sni` reads the TLS record header + handshake header + ClientHello fields
until it reaches the SNI extension (or exhausts the record without finding one).
It returns the peeked bytes as a `Bytes` object along with the now-drained socket
so the full stream (including those peeked bytes) can be spliced onward. Use
`tokio::io::split` + a `BufReader` with a fixed peek window; do **not** use
`tokio::net::TcpStream::peek` (OS peek is not guaranteed to return all bytes at
once).

A minimal TLS ClientHello SNI parser is ~80 lines of Rust following RFC 8446
§4.1.2. Use the `bytes` crate for cursor-based parsing. No external TLS parser
crate needed.

---

## Phase 3 — Tunnel layer

### Protocol

Pi holds one persistent authenticated WebSocket connection to the bridge. The
bridge uses this connection to forward new inbound TCP streams via a lightweight
multiplexing protocol.

```
Pi  ──WSS──▶  bridge  GET /v1/installs/:id/tunnel
              (existing Bearer + Ed25519 auth applies)

Bridge ──▶ Pi : CONNECT  conn_id:u32  dest_port:u16
Pi     ──▶ Bridge : READY  conn_id:u32
Bridge ↔  Pi : DATA    conn_id:u32  payload:bytes
Bridge ↔  Pi : CLOSE   conn_id:u32
Pi     ──▶ Bridge : PING (every 30 s)
Bridge ──▶ Pi : PONG
```

Message framing: 7-byte header for CONNECT `[type:u8, conn_id:u32_be,
dest_port:u16_be]`; 5-byte header `[type:u8, conn_id:u32_be]` for all other
fixed-size frames; variable-length payload appended for DATA frames.

`dest_port` tells the Pi which local port to connect to:
- `443` → wardnetd HTTPS server (TLS terminated by wardnetd)
- `853` → wardnetd DoT DNS server (TLS terminated by wardnetd)

Adding a port here costs 2 bytes on the CONNECT frame and avoids any need for
separate tunnel connections or out-of-band negotiation. Future ports (e.g. a
future SOCKS5 or admin port) are additive.

`conn_id` is a `u32` counter maintained per tunnel connection (wraps at
`u32::MAX`; at realistic load this is fine).

This framing is intentionally minimal — if the codebase later needs stream
multiplexing with flow control, replace with yamux.

### New files

**`src/tunnel/mod.rs`**

```
pub mod registry;
pub mod handler;    // WebSocket upgrade handler
pub mod router;     // route_to_tunnel() used by SNI demuxer
```

**`src/tunnel/registry.rs`**

```rust
// TunnelRegistry: DashMap<Uuid, TunnelSender>
// TunnelSender: mpsc::Sender<ForwardRequest>
// ForwardRequest: { conn_id: u32, stream: TcpStream }
//
// TunnelRegistry::register(install_id, sender)
// TunnelRegistry::unregister(install_id)
// TunnelRegistry::find(install_id) -> Option<TunnelSender>
// TunnelRegistry::find_by_name(slug) -> Option<(Uuid, TunnelSender)>
//   ↑ needs slug→install_id mapping; store in a second DashMap<String, Uuid>
//     populated on WebSocket connect, cleared on disconnect.
```

**`src/tunnel/handler.rs`** — `GET /v1/installs/:id/tunnel`

```rust
// 1. Upgrade to WebSocket (axum::extract::ws::WebSocketUpgrade).
// 2. Register the sender in TunnelRegistry (install_id → sender).
// 3. Spawn a task: read READY / PING frames from Pi; write CONNECT / PONG frames.
// 4. On WebSocket close or error: unregister from TunnelRegistry.
```

**`src/tunnel/router.rs`** — called by the SNI demuxer

```rust
// route_to_tunnel(stream: TcpStream, slug: &str, registry: &TunnelRegistry)
// 1. registry.find_by_name(slug) → TunnelSender.
// 2. Assign conn_id, send ForwardRequest to the tunnel task.
// 3. Tunnel task sends CONNECT conn_id to Pi over WebSocket.
// 4. Wait for READY conn_id from Pi (timeout: 5 s).
// 5. Splice: tokio::io::copy_bidirectional between the TcpStream and the
//    per-conn_id byte channel set up by the tunnel task.
```

### Modified files

**`src/api/mod.rs`** — register the new route:

```rust
routes!(tunnel::ws_tunnel)   // GET /v1/installs/:id/tunnel
```

**`src/api/mod.rs`** OpenAPI info block — add tunnel tag.

**`src/state.rs`** — add `tunnel_registry: Arc<TunnelRegistry>` to `AppState`.

**`src/main.rs`** — spawn SNI demuxer task alongside axum `serve`:

```rust
let tunnel_registry = Arc::new(TunnelRegistry::new());
let state = AppState::new(config.clone(), pools, dns, replay_cache, tunnel_registry.clone());
tokio::select! {
    r = axum::serve(listener, router) => r?,
    r = sni::run(config, tunnel_registry)  => r?,
}
```

**`src/config.rs`** — add `sni_listen_addr`, `caddy_addr`, `bridge_hostname`.

**`AGENTS.md`** — add tunnel invariants:

- `TunnelRegistry` is in-memory only; it is not persisted. After a node restart
  all Pis must reconnect. This is expected.
- The SNI demuxer task must be restarted if it panics; wrap the `run` loop in
  `tokio::spawn` with a restart supervisor rather than letting the whole process
  exit.
- `conn_id` wraps at `u32::MAX`; do not assume monotonically increasing IDs in
  the splicing layer.

### Migration for new tunnel table (none required)

Tunnel connections are purely in-memory. The `installs` table already has the
`name` (slug) field needed to map SNI → install_id. No schema change.

---

## Phase 4 — Deployment / infrastructure (ops, no code changes)

### OCI NLB (TCP mode)

- Two listeners: **:443** and **:853**, both targeting the same backend set
  (bridge node private IPs). Health check = `TCP :8080` (or HTTP `GET /health`).
- **Source IP affinity (sticky sessions)**: required so each Pi's API calls and
  tunnel connection always hit the same backend node (which holds the in-memory
  tunnel state). Enable on the NLB backend set.
- NLB idle connection timeout: raise to **3600 s** (1 h). Pi-side heartbeat
  (PING every 30 s) keeps tunnels alive; this gives generous headroom.
- Android DoT connections are short-lived (query/response, a few seconds each);
  no special timeout tuning needed for port 853.

### Active/active

- Bridge nodes are identical; each holds a subset of all Pi tunnels (determined
  by sticky sessions).
- On node failure the NLB stops routing to it; affected Pis reconnect to a
  surviving node (backoff logic on Pi side — separate wardnetd issue).
- Unaffected Pis on other nodes are not disrupted.

### Caddy on bridge nodes

- Caddy listens on `127.0.0.1:8443` (not 443 — that is now owned by the SNI
  demuxer).
- Caddy manages the cert for `bridge.<REGION>.wardnet.network` via HTTP-01 or
  DNS-01 as appropriate.
- Caddyfile forwards `bridge.<REGION>.wardnet.network` → `localhost:8080`.

### MySQL

- DSN injected via systemd environment file, same pattern as `CLOUDFLARE_API_TOKEN`.
- `DATABASE_URL=mysql://wardnet:<password>@<oci-mysql-host>/wardnet_bridge`
- Connection pool size 10 per node; OCI MySQL free tier supports up to 90
  concurrent connections (well within budget for 2–3 nodes).

### Litestream

- Remove from deployment. No object-storage bucket needed for the bridge.

---

## Out of scope — separate issues required

These are **wardnetd** (Pi daemon) changes, not bridge changes:

1. **Tunnel client** — wardnetd establishes and maintains the WebSocket tunnel to
   `bridge.<REGION>.wardnet.network` after registration. Exponential backoff on
   disconnect. Accepts CONNECT frames from the bridge, reads `dest_port`, and
   opens a local connection to `127.0.0.1:<dest_port>` for each stream.

2. **wardnetd-managed TLS** — wardnetd drives the ACME DNS-01 dance using the
   bridge's existing `/v1/installs/:id/acme` endpoints. Cert private key lives in
   wardnetd's `SecretStore`. wardnetd handles TLS termination directly (replacing
   Caddy on the Pi), so the private key never leaves the Pi's security boundary.
   Issue #436 (Caddy on Pi) is superseded by this.

3. **DNS-over-TLS server in wardnetd** — wardnetd's existing DNS server gains a
   TLS listener on port 853. Wraps the same resolver and filter pipeline as the
   LAN DNS server. Uses the same cert managed under point 2 above. No new cert
   infrastructure needed — `<slug>.my.<region>.wardnet.network` already covers
   port 853 (certs are hostname-scoped, not port-scoped). Android Private DNS
   users enter this hostname in Settings → Network → Private DNS.

4. **Android setup UX** — the wardnet setup wizard / admin UI surfaces the
   Private DNS hostname (`<slug>.my.<region>.wardnet.network`) with a "Copy"
   button and instructions for Android Settings → Network → Private DNS.

---

## Work order

```
Phase 1 (MySQL)
  └─ no new concepts; isolated to db/ and repository/ modules
  └─ unblocks Phase 2 and Phase 3 (both need MySQL-backed tests)

Phase 2 (SNI demuxer)
  └─ depends on: Phase 1 (test infra)
  └─ can be tested independently (no tunnel needed; just verify routing)

Phase 3 (Tunnel layer)
  └─ depends on: Phase 2 (SNI demuxer calls route_to_tunnel)
  └─ integration test: real WebSocket tunnel, SNI demuxer, end-to-end splice

Phase 4 (Ops/deployment)
  └─ no code gate; can be prepared in parallel with Phase 1–3
```

Total new source files: ~6 (`src/sni/mod.rs`, `src/tunnel/mod.rs`,
`src/tunnel/registry.rs`, `src/tunnel/handler.rs`, `src/tunnel/router.rs`,
`tests/common/mod.rs`).

Modified source files: `Cargo.toml`, `src/db/mod.rs`, `src/config.rs`,
`src/state.rs`, `src/main.rs`, `src/api/mod.rs`, `src/repository/install.rs`,
`src/repository/challenge.rs`, both migration SQL files, `AGENTS.md`, `README.md`.
