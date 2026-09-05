# Architecture

## Layered design with dependency injection

```
wardnetd (main.rs)   →  wires real Linux backends, calls init_services(), starts axum server
                              │
wardnetd-api          │  AppState + Axum router: thin handlers, extract request, call service
                              ↓
wardnetd-services     │  Services struct + init_services(): AuthService, BackupService,
                      │  DeviceService, TunnelService, RoutingService, DhcpService,
                      │  VpnProviderService, SystemService, LogService, UpdateService
                              ↓
wardnetd-data         │  RepositoryFactory: AdminRepository, DeviceRepository, TunnelRepository,
                      │  DhcpRepository, DnsRepository, SystemConfigRepository, DatabaseDumper,
                      │  SecretStore, …
                              ↓
SQLite                │  Parameterized queries only (`.bind()`), never string interpolation

wardnet-common        ─  Shared types, config, events — referenced by all crates above
wardnetd-mock         ─  Dev binary: same wardnetd-api/services/data stack, no-op Linux backends
```

- **Traits define ALL boundaries** — every layer depends on trait interfaces, not concrete types. This includes infrastructure: `TunnelInterface`, `SecretStore`, `EventPublisher`, `FirewallManager`, `PolicyRouter`, `CommandExecutor`, `PacketCapture`, `DhcpSocket`, `DatabaseDumper`, `BackupArchiver`, `NordVpnApi` (provider-specific HTTP abstraction).
- **`wardnetd-services`** exports a `Services` struct and `init_services()` function — the single wiring point for all service implementations.
- **`AppState`** (in `wardnetd-api`) holds `Arc<dyn Service>` trait objects; no pool exposed to handlers.
- **API handlers never touch the database** — they call services, services call repositories.
- **Cross-service access is always service-to-service** — if service A needs data or behaviour from domain B, it receives and calls `Arc<dyn BService>`. Service A must never hold `Arc<dyn BRepository>` from domain B. Injecting a sibling repository directly bypasses that domain's business rules and is the first step toward duplicating logic across services. Every operation a sibling needs must be exposed as a method on the owning service's trait.
- **Database-provider concerns live next to the repositories**: the `DatabaseDumper` trait + its SQLite impl live in `wardnetd-data/src/database_dumper.rs`, *not* in the backup module. A future non-SQLite provider ships its own dumper alongside its own repositories and the backup service picks it up through `RepositoryFactory::dumper()` with no service-layer changes.
- **Secret-store concerns are provider-owned too**: `SecretStore::backup_contents` / `restore_from_backup` live on the trait, so each provider (`FileSecretStore` today; `HashicorpVault`, `1Password`, etc. later) decides what travels with a bundle and what stays in the external service.
- **mDNS advertisement is a self-contained runner**: `wardnetd::mdns_advertiser::MdnsAdvertiser` (Linux production binary only) publishes `<hostname>.local.` so the setup wizard is reachable without a known IP. It is a runner, not a service — mirrors `HeartbeatRunner` / `RouteMonitor`, not a trait-backed service. The mock binary does not start it.

## Tunnel rebuild endpoint (issue #480)

`POST /api/tunnels/{id}/rebuild` — admin-only endpoint that invokes `TunnelService::rebuild(id)`. The service method calls `tear_down_core` then `bring_up_core` (the same path used by the internal watchdog). The handler returns 404 for unknown tunnel IDs and guards against issuing a rebuild while a test probe is already in flight (409 Conflict). The response is `RebuildTunnelResponse { ok: true }`.

Key conventions:
- `tear_down_core` / `bring_up_core` are the internal primitives shared between `test_tunnel`, the idle watchdog, and `rebuild`. Always call them — never reach into `TunnelInterface` directly from `rebuild`.
- Errors from `bring_up_core` are **logged** (not silently swallowed) before returning to the caller.
- The in-flight guard reuses the same `Arc<Mutex<HashSet<Uuid>>>` that `test_tunnel` already holds; this prevents a rebuild from racing a concurrent test.

## Stats subsystem (issue #409)

A generic pre-aggregating stats subsystem. All layers (data, services, API) are complete as of PR 2.

### Three-tier storage — mirrors the `tunnel_metrics` pattern

| Table | Granularity | Retention | Key |
|---|---|---|---|
| `stats_intraday` | 1-minute buckets (`bucket_ts` = Unix seconds truncated to minute) | 25 h (`INTRADAY_RETENTION`) | `(metric, labels, bucket_ts)` |
| `stats_hourly` | 1-hour rollup (`hour_ts`) | 8 days (`HOURLY_RETENTION`) | `(metric, labels, hour_ts)` |
| `stats_daily` | 1-day rollup (`day` = `YYYY-MM-DD` UTC) | 13 months / 397 days (`DAILY_RETENTION_DAYS`) | `(metric, labels, day)` |

The retention constants live in `wardnetd-services/src/stats/service.rs`; the
hourly tier was added by the `20260611000000_stats_hourly.sql` migration. The
`Hour` query granularity reads the `stats_hourly` table — it is **not** computed
on the fly from intraday rows.

### Instrument kinds

- **Counter** — upsert accumulates: `value = value + excluded.value`
- **Gauge** — upsert overwrites: `value = excluded.value`

The `kind` column (`"counter"` / `"gauge"`) governs which SQL branch fires in `SqliteStatsRepository::upsert_intraday`.

### Label design — avoid high-cardinality explosions

Labels are stored as a **sorted JSON object string** (e.g. `{"outcome":"blocked"}`). Expression indexes on `json_extract(labels, '$.outcome')`, `$.domain`, and `$.client` cover the common filter paths without requiring separate columns.

The metric set was deliberately split to bound worst-case intraday row counts:

| Metric | Labels | Notes |
|---|---|---|
| `dns.queries` | `{outcome}` | Counter per outcome; low cardinality |
| `dns.latency_ms` | `{outcome}` | Gauge per outcome |
| `dns.queries.by_domain` | `{domain}` | Counter; blocked queries only — keeps domain set bounded |
| `dns.queries.by_client` | `{client}` | Counter per client IP |

Do **not** use a single `dns.queries` metric with `{client, domain, outcome}` labels — that cross product explodes row counts.

### Shared types (`wardnet-common/src/stats.rs`)

`StatsQuery`, `StatsTopQuery`, `StatsBucket`, `StatsSeriesPoint`, `StatsQueryResponse`, `StatsTopEntry`, `StatsTopResponse` — used by both the service/repository layer and the API/SDK.

### Repository interface (`wardnetd-data/src/repository/stats.rs`)

`StatsRepository` trait methods: `upsert_intraday`, `rollup_daily`, `trim_intraday`, `trim_daily`, `query_intraday`, `query_daily`, `top_n`. Accessed via `RepositoryFactory::stats()`.

### In-process pipeline

```
DNS query (DnsLogSink::record)
    │
    ↓  Counter/Gauge instruments (Meter → StatsBuffer)
StatsBuffer  (in-memory HashMap, Mutex-guarded, ~10 s window)
    │
    ↓  StatsFlushRunner::perform_flush  (every 10 s)
StatsRepository::upsert_intraday  →  stats_intraday table
    │
    ↓  StatsFlushRunner::perform_maintenance  (every 1 h, also at startup)
StatsRepository::rollup_daily  →  stats_daily table
StatsRepository::trim_intraday / trim_daily  (retention enforcement)
```

### Service layer (`wardnetd-services/src/stats/`)

| Module | Purpose |
|---|---|
| `buffer.rs` | `StatsBuffer` — in-memory accumulator; counters sum, gauges overwrite |
| `meter.rs` | `Meter` factory + `Counter` / `Gauge` instruments (OTel-style API) |
| `service.rs` | `StatsService` trait + `StatsServiceImpl` (time-series and top-N queries; calls `require_admin`) |
| `flush_runner.rs` | `StatsFlushRunner` — background task: 10 s flush + 1 h maintenance; follows runner contract |

`StatsService::query` supports three granularities: `Minute` (raw intraday rows), `Hour` (the `stats_hourly` rollup table), and `Day` (the `stats_daily` rollup table).

### API endpoints (`wardnetd-api/src/api/stats.rs`)

| Method | Path | Body | Description |
|---|---|---|---|
| `GET` | `/api/stats` | `StatsQuery` JSON | Time-series query (minute/hour/day granularity) |
| `GET` | `/api/stats/top` | `StatsTopQuery` JSON | Top-N label values ranked by total |

Both endpoints require admin auth. `StatsService` enforces this via `auth_context::require_admin()`.

### DNS stats migration from `DnsRepository`

`DnsRepository` previously exposed `query_stats`, `top_domains`, `top_clients`, and `series_buckets`. These on-the-fly aggregation methods were removed in PR 2. DNS stats are now served entirely through the generic `StatsService` + `/api/stats` / `/api/stats/top` endpoints using the four `dns.*` metrics recorded by `DnsLogSink`.

## Local-DNS subsystem (issue #217)

Zones, custom records, and conditional forwarding rules managed via the admin UI. Three storage layers in `wardnetd-data`: `DnsZone`, `CustomDnsRecord`, `ConditionalForwardingRule` tables, all CRUD'd through `DnsLocalRepository`. The service layer is `DnsLocalServiceImpl` in `wardnetd-services/dns_local/`.

### AuthoritativeView — lock-free in-memory snapshot

`AuthoritativeView` (`wardnetd-services/dns/authoritative.rs`) is an immutable snapshot of all **enabled** records and forwarding rules, built from the repository at startup and replaced atomically on every `WardnetEvent::DnsLocalChanged`. It is held behind an `Arc<ArcSwap<AuthoritativeView>>` in `UdpDnsServer` so each query reads a consistent snapshot lock-free, with no contention on the write path.

Key design rule: records whose zone has `enabled = false` are excluded even if the record itself is enabled. Records with no zone are included when their own flag is set. Forwarding rules are sorted longest-domain-first so the first suffix match is always the most specific one.

The view also carries a **reverse (PTR) index**: every enabled A record whose value is a private/internal IPv4 (RFC 1918, link-local, RFC 6598 CGN) is inverted into an `IPv4 → records` map. This is the complement of the DHCP `.lan` forward integration — the same `{hostname}.lan → IP` records the `DhcpLanRunner` writes become the source of reverse answers, so the index rides the existing `DnsLocalChanged` rebuild with no extra data source or event. It lets the gateway answer `in-addr.arpa` PTR queries for private ranges locally (RFC 6303) rather than leaking them upstream.

### Resolution pipeline (per-query order)

| Step | What happens | Notes |
|---|---|---|
| **0 — upstream selection** | Client IP → `UpstreamId` from the applied-state routing snapshot; a device-authenticated (DoT) client keys on its device id through the persisted-rule `dns_device_upstream_snapshot` first, falling back to the IP-keyed map on a miss (#923) | Miss in both = `Default` (system-wide upstream). The device-keyed map is what lets a *roaming* device's queries follow its tunnel binding — its transport peer is the relay loopback, useless as a routing key. Entries exist only while the bound tunnel is not `Down` (down ⇒ soft fallback to `Default`, mirroring `handle_tunnel_down` on the LAN path; kill-switch #235 will gate this per device), and a `Default` rule is excluded when the device's zone forbids tunnel egress (mirroring the zone enforcer's applied-state-only clamp). Rebuilt by `DnsDeviceSnapshotListener` — a coalescing bus listener reacting to rule/policy/tunnel-status/DNS-override/zone events — plus once at startup inside `RoutingService::reconcile`; rebuilds are serialized, and a failed per-tunnel lookup retains prior entries rather than wiping them |
| **0.4 — reverse PTR** | Private-range `in-addr.arpa` PTR answered locally: hostname from the reverse index, else authoritative NXDOMAIN + synthetic SOA | RFC 6303; a conditional-forwarding rule on the reverse name wins. Returns `Authoritative` / `AuthoritativeNxdomain` |
| **0.5 — authoritative** | `AuthoritativeView::lookup` — answers directly, sets AA bit | Bypasses cache and filter entirely; returns `DnsQueryResult::Authoritative` |
| **0.6 — conditional forwarding rule match** | `AuthoritativeView::match_forwarding_rule` — selects per-domain upstream | Captured before the cache check; forwarding fires at step 3 if filter passes |
| **1 — cache** | Per-`UpstreamId` response cache | Tunnel and LAN devices have separate cache namespaces |
| **2 — filter** | `DnsFilterService::check` — block / rewrite / pass | Applies even to conditionally-forwarded domains |
| **2.5 — rate limit** | Per-client-IP token bucket, checked **only here** — right before a query leaves for an upstream | `rate_limit_per_second == 0` disables. Local answers (authoritative, reverse PTR, cache hit, filter block/rewrite) returned above and are exempt; the limiter exists to protect upstreams |
| **3 — forward** | Conditional upstream (if matched at 0.6 and filter passed), otherwise default or tunnel upstream | `forward_via_conditional` binds an undeviced socket; `forward_via_tunnel` uses `SO_BINDTODEVICE` |

Authoritative answers fully short-circuit the pipeline (no cache store, no filter). The rate limiter guards only the upstream-bound path (step 2.5): its purpose is to protect upstream resolvers, so a burst of locally-answerable queries — e.g. a mesh node's RFC1918 PTR storm — is answered every time and never REFUSED, which is what stops such a client's REFUSED-driven retry loop from self-amplifying. CNAME handling: for A/AAAA queries on a domain that has only a CNAME record in the view, the CNAME goes into the answer section and the target A/AAAA record (if also in the view) goes into the additional section.

### Event-driven rebuild

`DnsLocalServiceImpl` publishes `WardnetEvent::DnsLocalChanged` after every mutation. `domain` carries the **subtree the change governs** — the zone name for a zone mutation, the record or rule domain otherwise — and a mutation that moves a domain (a rename) emits twice, once for the vacated name and once for the claimed one.

`DnsRunner` handles the event by calling `DnsServer::update_authoritative_view` (atomic ArcSwap swap) and `DnsServer::invalidate_subtree`.

Eviction is **subtree-scoped**, not exact-name (issue #1184). Every form of local DNS applies to a whole subtree — a forwarding rule and an authoritative zone match by suffix, a wildcard record covers everything below its suffix — while the cache is consulted *before* any of them (step 1, ahead of the step-3 forward). Evicting only the exact name therefore left every already-cached subdomain resolving the old way until its TTL expired, so a new forwarding rule looked stored, enabled, and correct in the UI while doing nothing to the names that motivated it. A wildcard domain (`*.suffix`) evicts its suffix subtree; over-eviction costs one re-resolution, under-eviction costs correctness.

### Background runners call auth-gated services, never repositories

Background runners (`DnsRunner`, `DnsFilterRunner`, `DnsQueryLogRunner`,
`DbMaintenanceRunner`, `DeviceRetentionRunner`, `DhcpLanRunner`) hold
`Arc<dyn *Service>` trait
objects, **not** repository handles. Each runs its service calls under an
admin auth context:

```rust
let admin_ctx = AuthContext::Admin { admin_id: Uuid::nil() };
auth_context::with_context(admin_ctx.clone(), service.some_method()).await
```

This keeps the service layer the single auth-and-events chokepoint:
`DnsLocalService::upsert_record`, for example, owns `DnsLocalChanged`
emission, so the DHCP `.lan` runner can never write a record without
triggering the authoritative-view rebuild. The `Services` struct does **not**
expose `dns_local_repo`; `DbMaintenanceRunner` takes a thin
`MaintenanceService` rather than `MaintenanceRepository`. Reaching for a
`Arc<dyn *Repository>` inside a runner is a layering violation.

## DNS forwarding ladder (issue #1199)

The default-forwarder path — step 3 of the resolution pipeline above, whenever
the selected upstream is `Default` — is a ladder this daemon walks itself,
built in `wardnetd/src/dns/upstream_pool.rs` and driven by `walk_ladder` in
`pipeline.rs`.

### Why not one hickory resolver holding every upstream

That is what it used to be, and it did not do what the admin UI promises.
`NameServerPool::try_send` treats `ServerOrderingStrategy` as a *sort* and then
races `num_concurrent_reqs` servers — **2** by default, which we never set — in
parallel, returning whichever answers first and penalising the loser's SRTT. So
"Failover (in order)" sent every query to the first *two* providers at once: the
configured order was not honoured, and a second provider saw traffic the admin
never agreed to show it. It also made the query log unfixable, because hickory's
`Lookup` carries no record of which name server answered — which is why the
`upstream` column used to report `upstream_servers[0]` for every query
regardless of what actually happened.

One single-server resolver per upstream, walked here, fixes all of it at once:
the order is exactly what was configured, one provider is asked at a time, and
the rung that answered is known by construction.

### `UpstreamPool` — `all` vs `serving`

`all` is every usable upstream with its own resolver, rebuilt only when
`update_config` sees upstreams / DNSSEC / forwarder mode change. `serving` is
the ordered subset the ladder actually tries, recomputed from scratch after
every probe round. Both live in one `ArcSwap`, so a rebuild never blocks a query
in flight, and the prober republishes `serving` through `rcu` so a concurrent
config rebuild wins rather than being clobbered. Entries are shared `Arc`s:
an upstream that is evicted and later restored keeps its warm `DoT`/`DoH`
connection.

`serving` = effective upstreams (mode-narrowed) − those the prober reports
unreachable, ordered by mode: `Failover` keeps the configured order, `Fastest`
sorts by the prober's EWMA (the same number the DNS page displays — previously
the UI showed our EWMA while hickory routed by its own hidden SRTT), `Single`
is the pinned server. Two invariants: an upstream *absent* from the health
snapshot is unmeasured, not down, or the pool would empty on every startup; and
if every upstream looks unreachable the full set is restored, because a ladder
of zero servers answers nothing.

### Bounds

Set explicitly rather than inherited — hickory's defaults (5s timeout, 2
*retries*, each rerunning the whole ladder) are what let a degraded upstream
stall a query for 20-30s while the client's stub resolver had long since given
up and was retransmitting into our own rate limiter.

| Bound | Source | Scope |
|---|---|---|
| `opts.timeout` | `DnsConfig::upstream_timeout_ms` (default 1500ms) | one server's connection *and* hickory's internal pool loop |
| per-rung `tokio::time::timeout` | same value | one rung, so a black-holing server cannot starve the ones behind it |
| whole-ladder `tokio::time::timeout` | `DnsConfig::forward_deadline_ms` (default 3500ms) | the query, kept under a glibc stub's ~5s patience |
| `opts.attempts = 0` | constant | hickory's retry is off; the next *rung* is the retry, and it reaches a different server |
| `opts.num_concurrent_reqs = 1` | constant | redundant today, set so a future second server here cannot silently reintroduce racing |

`attempts = 0` is not "one packet": `UdpClientStream` still retransmits up to
four datagrams spaced by `max(1.2 × SRTT, 333ms)` underneath.

**A negative answer is terminal.** NXDOMAIN and NODATA are resolutions, not
failures — failing over on one would re-ask a question the authority already
answered, leak the name to a second provider, and risk a contradictory answer.
Only a genuine failure advances a rung.

### Attribution

`dns_query_log.upstream` names the rung that answered, exactly. When the whole
ladder is exhausted it is **NULL**: no upstream served the query, so naming one
would skew the per-upstream aggregates and point diagnosis at the wrong
provider — which is precisely what the old behaviour did during the incident
that prompted this. Which servers failed is in the per-upstream `warn!` lines
instead, one per failing rung, plus one line per transition when the prober
takes an upstream out of rotation or puts it back.

Sustained failure also raises a `dns_upstream_unreachable` anomaly, one per
upstream, from a preventive detector reading the same `UpstreamHealth` handle.
That handle exists because of construction order: the anomaly registry is built
during service wiring, while `UdpDnsServer` is constructed later by the daemon
binary, so both are handed a clone rather than one holding a reference to the
other. The prober keeps its **own** resolver per upstream, separate from the
serving ones — for `DoT`/`DoH` a shared connection would queue the probe behind
live queries and measure latency-plus-queueing. The cost of that independence:
a probe's success is not proof the serving path is healthy, so reachability is a
routing *hint*, and the per-rung deadline is what actually protects a client.

### Recursive mode — relaxed QNAME minimization (issue #1002)

Recursive mode (`build_recursor`) runs hickory's recursor with
`QNameMinimization::Relaxed` rather than its `Strict` default. `Strict`
enforces RFC 8020 — "nothing exists below an NXDOMAIN" — while the recursor
walks a name label by label looking for the zone cut, so an intermediate
label answered NXDOMAIN aborts the lookup. A spec-correct server answers
NODATA for an empty non-terminal and the walk continues; Route 53 answers
NXDOMAIN for an ENT sitting **above a delegation**, which stranded names
that were delegated and resolvable through every public resolver — a
household VPN's SAML login host, in the incident that prompted this. Under
`Relaxed` the walk continues past that NXDOMAIN, which is what 1.1.1.1 /
8.8.8.8 / 9.9.9.9 and Unbound with `harden-below-nxdomain: no` already do.

What is given up is the negative-answer shortcut, not the trust boundary:
the answer still comes from the authoritative server reached at the zone
cut, and DNSSEC validation is unchanged. The setting is asserted by a unit
test rather than left to the dependency's default, because the failure it
prevents is a silent outage that only shows up against one zone shape.

## DDNS subsystem (issue #527 / #521 umbrella)

Keeps the Pi's public A record current and (in later commits) handles ACME DNS-01 TXT records. Lives entirely in `wardnetd-services/src/ddns/`.

### Shape

```
DdnsUpdateRunner  ──(admin auth ctx)──▶  DdnsService  ──▶  DnsProvider
 (5-min tick)                            (auth-gated)       (bridge | cloudflare)
```

`DdnsUpdateRunner` holds `Arc<dyn DdnsService>` and calls it under an admin auth context — it never touches repositories or providers directly. `DdnsService` is the auth-and-persistence chokepoint: every method opens with `auth_context::require_admin()`.

### Provider abstraction (`provider.rs`)

`DnsProvider` trait with three methods: `upsert_a(ip)`, `set_txt(name, value)`, `delete_txt(name)`. Two impls:

| Impl | File | Auth |
|---|---|---|
| `BridgeProvider` | `bridge.rs` | Ed25519-signed requests (seed → `SigningKey`); bearer token in header; PoW-based registration via `register_install` |
| `CloudflareProvider` | `cloudflare.rs` | Per-request Bearer token; list-then-create/update against CF v4 API |

Providers are **rebuilt per call** from stored config + secrets (`build_provider()`). This is intentional: reads are cheap at the 5-minute cadence, and rebuilding means a provider switch takes effect without any cache-invalidation plumbing. All providers share one pooled `reqwest::Client`.

### Storage split

| Kind | Location | Keys |
|---|---|---|
| Non-secret config | `system_config` table | `ddns_provider`, `ddns_install_id`, `ddns_subdomain`, `ddns_region`, `ddns_bridge_base_url`, `ddns_last_public_ip`, `ddns_domain`, `ddns_cf_zone_id` |
| Secrets | `SecretStore` | `ddns/bridge/signing_key` (32-byte Ed25519 seed), `ddns/bridge/bearer_token`, `ddns/cloudflare/api_token` |

### Supporting modules

| Module | Purpose |
|---|---|
| `region.rs` | Built-in region catalog (`REGION_CATALOG`); `select_best` probes all regions concurrently and picks lowest latency |
| `public_ip.rs` | WAN public-IP discovery; rejects non-global IPv4 (RFC 1918, loopback, link-local); tries multiple echo endpoints in order |
| `runner.rs` | `DdnsUpdateRunner` — idle-until-configured 5-min tick; follows the runner contract (accepts `&tracing::Span`, instruments spawn) |

### Service methods

| Method | Notes |
|---|---|
| `register_with_bridge(name)` | Probes regions, calls `register_install` (PoW), persists secrets first then config, returns `DdnsRegistration{subdomain, region}` |
| `check_name_available(name)` | Probes best region, asks bridge; used by wizard |
| `refresh_public_ip()` | Discovers WAN IP, short-circuits if unchanged, calls `provider.upsert_a(ip)` |
| `status()` | Reads provider, FQDN, and last-published IP from config; returns `DdnsStatus` |

## Daemon-owned TLS (issue #528 / #521 umbrella)

`wardnetd` terminates TLS itself — no Caddy. ACME DNS-01 issuance reuses the
DDNS providers (via `DdnsService::set_acme_challenge` / `clear_acme_challenge`)
to publish `_acme-challenge` TXT records.

### Shape

```text
TlsRenewalRunner ─(admin ctx)─▶ TlsService ─▶ acme (instant-acme) ─▶ DdnsService.set_acme_challenge
 (12h tick)                     (auth-gated)  └▶ CertActivator.activate (hot-swap :443)
```

`TlsRenewalRunner` holds `Arc<dyn TlsService>` and calls it under an admin
context — never an ACME client, provider, or repository directly (runner
contract). `TlsService::ensure_certificate()` is one idempotent
issue-if-missing-or-renew-if-<30d method; inert (`TlsStatus::NotConfigured`)
when no FQDN is active, so the runner is idle until DDNS is configured. The
wizard (C9) and Settings (C10) call the same method. Cert + key (and the ACME
account credentials) are read/written **only** through the `SecretStore`
abstraction — never direct filesystem access.

### Always-bound `:443` / 503-until-provisioned serving

The `:443` listener is **always bound**, seeded at boot from the stored real
cert if present, else from a throwaway rcgen **placeholder** self-signed cert. A
shared `provisioned: Arc<AtomicBool>` (default `false` for the placeholder) gates
a **503 guard layer** on the `:443` app: every route returns
`503 "TLS not provisioned"` until a real cert loads, so the admin API is never
served under the untrusted placeholder. Pre-provisioning, the operator uses
`:7411` plain HTTP (unguarded). `:80` 308-redirects to HTTPS. The listener is
constant — no supervisor, no mid-run listener start.

### `CertActivator` abstraction boundary

The serving stack (`axum-server` + `RustlsConfig`) lives in `wardnetd`, not in
`wardnetd-services`. The `CertActivator` trait (defined in `wardnetd-services`,
implemented by `wardnetd::tls_server::CertActivatorImpl`) is the seam:
`activate(chain, key)` calls `RustlsConfig::reload_from_pem` (lock-free in-memory
swap) and flips the `provisioned` flag. It is injected via `Backends` so the
TLS service can swap the live cert without the services crate depending on the
serving stack. The aws-lc-rs crypto provider is installed once in `main` (both
ring + aws-lc-rs are in the tree → rustls can't auto-pick).

| Module (`wardnetd-services/src/tls/`) | Purpose |
|---|---|
| `mod.rs` | `TlsService` trait + impl, `CertActivator` trait, `TlsStatus`, `load_stored_cert` |
| `acme.rs` | instant-acme DNS-01 orchestration; CSR/leaf key via rcgen; `parse_not_after` (x509-parser) |
| `runner.rs` | `TlsRenewalRunner` — idle-until-configured 12h tick; follows the runner contract |

## Bridge service (issue #435)

Rust / Axum / PostgreSQL microservice for DDNS registration and ACME DNS-01 credential proxying. Deployed as a single binary behind a transparent L4 proxy (nginx + PROXY protocol v1); the bridge **terminates TLS for its own FQDN itself** via ACME HTTP-01 and passes tenant traffic through to the home-Pi tunnels — no Caddy (see `docs/adr/0007-bridge-self-terminated-tls.md`). Each bridge instance owns one region, keyed by its inforge region slug (e.g. `use1`).

### Security invariants

These rules are hard requirements — violating any of them opens a specific attack vector.

| Rule | What it prevents | Where it lives |
|---|---|---|
| **Default to loopback (`127.0.0.1:8080`)** | Exposes unauthenticated endpoints on every interface if set to `0.0.0.0` | `config.rs` `LISTEN_ADDR` default |
| **X-Forwarded-For only trusted from loopback peers** | IP spoofing in rate-limit and challenge-binding checks from direct connections | `api/challenge.rs` `client_ip()` |
| **Canonical payload includes `path_and_query`** | Attacker replaces query params without invalidating signature if only `path` is covered | `auth/middleware.rs` `auth_layer` |
| **Challenge IP binding** | A challenge solved by attacker on their IP cannot be replayed from victim's IP | `api/register.rs` |
| **Name uniqueness check BEFORE burning challenge** | Avoids wasting the caller's PoW work on a taken name | `api/register.rs` `register_install` |
| **In-memory replay cache** | Replays of valid signed requests within the ±60 s timestamp window | `replay_cache.rs`, wired in `auth_layer` |
| **Reject private/reserved IPv4 ranges** | SSRF via DNS A record pointing at RFC 1918 / loopback addresses | `api/ip.rs` `is_reserved_ipv4()` |
| **SHA-256(bearer_token) stored** | Bearer token never exposed at rest even if DB is compromised | `repository/install.rs` `token_hash` column |
| **`pub_key_bytes: [u8; 32]` on `Install`** | Avoids per-request base64 decode + allocation; key decoded once at DB row load | `repository/install.rs` `InstallRow::into_install` |

### Rate limits

| Limit | Scope | Endpoint |
|---|---|---|
| 3 registrations / IP / 24 h | IP address | `POST /v1/register` |
| 20 challenges / IP / hour | IP address | `GET /v1/register/challenge` |

### Auth model

Every request to `/v1/installs/*` must carry:
- `Authorization: Bearer <token>`
- `X-Wardnet-Timestamp: <unix_ts>` — rejected if `|now - ts| > 60 s`
- `X-Wardnet-Signature: <base64>` — Ed25519 over `"<METHOD>\n<path_and_query>\n<ts>\n<hex-sha256(body)>"`

Unauthenticated endpoints (`GET /v1/health`, `GET /v1/register/challenge`, `POST /v1/register`, `GET /v1/names/{name}/available`) never trigger a DB token lookup regardless of whether an `Authorization` header is present.

### Shared validation (`api/validation.rs`)

`RESERVED_NAMES`, `is_valid_name()`, `validate_name()`, and `validate_public_key()` live in one place. Do **not** duplicate them in handler modules — the name availability check and the registration handler must apply identical rules.

## Watchdog + health subsystem (issue #214)

A **three-layer** recovery model. `Restart=always` only catches a daemon that
*exits*; a livelocked or deadlocked daemon keeps systemd happy and never
recovers. The layers escalate from "report" to "restart the service" to "reboot
the host". See [0014-watchdog-and-health.md](../docs/adr/0014-watchdog-and-health.md).

### Layer 1 — `HealthMonitor` (`wardnetd-services/src/health/`)

`HealthCheck` is a `Send + Sync`, `#[async_trait]` trait (`name() -> &'static str`
+ `check() -> CheckOutcome`). `HealthMonitor` holds `Vec<Arc<dyn HealthCheck>>`
plus an `arc_swap::ArcSwap<HealthSnapshot>` (the same lock-free pattern as the
local-DNS `AuthoritativeView`). `refresh()` runs every check **concurrently**
(`futures::future::join_all`), each wrapped in a `tokio::time::timeout`
(`health.check_timeout_secs`, default 2 s) so a hung probe becomes
`Down { detail: "timeout" }` rather than stalling the cycle. A per-check
**consecutive-failure debounce** flips a component to DOWN only after
`failure_threshold` (default 3) straight failures and recovers it on the first
success; overall is DOWN if any component is DOWN. `HealthMonitorRunner`
(`wardnetd/src/health_runner.rs`, child span `health`) drives `refresh()` every
`health.refresh_interval_secs` (default 5 s). The four probes are registered in
`main.rs`: `DbHealthCheck` (`SELECT 1` via `MaintenanceRepository::ping`),
`LivenessHealthCheck`, `DnsServerHealthCheck`, `DhcpServerHealthCheck`. The
DNS/DHCP probes are **desired-vs-actual**, not raw `is_running()`: each reads
its configured `enabled` flag through the auth-gated service under a nil-admin
`auth_context` (like the runners) and reports DOWN only when
`enabled && !is_running()` — never for a deliberately toggled-off service,
which would otherwise make the soft watchdog restart-loop a healthy daemon. The
mock registers only liveness + DB (its noop DNS/DHCP servers never bind).
`GET /health` (`wardnetd-api/src/api/health.rs`,
unauthenticated, `security(())`) maps overall UP→200 / DOWN→503 with a
per-component body, reading the snapshot from `AppState::health_monitor`.

### Layer 2 — soft watchdog (`wardnetd/src/watchdog/soft.rs`, span `watchdog{layer=soft}`)

`SoftWatchdogRunner` ticks at `WATCHDOG_USEC/2` and sends `sd_notify(WATCHDOG=1)`
**only** when overall health is UP *and* the snapshot is fresh (younger than
`2 × refresh_interval`); otherwise it withholds the ping and systemd's
`WatchdogSec=15` restarts the *service*. The gating decision is the testable
`should_ping(&snapshot, staleness)` helper. `sd_notify` is behind a `Notifier`
trait (`SdNotifier` real impl; tests inject a fake) so the policy is unit-tested
without a real `NOTIFY_SOCKET`. `main.rs` also sends `READY=1` via the same
notifier once all listeners bind — which is why the unit is `Type=notify`.

### Layer 3 — hard watchdog (`wardnetd/src/watchdog/hard.rs`, span `watchdog{layer=hard}`)

`HardwareWatchdogRunner` pets `/dev/watchdog` every `watchdog.pet_interval_secs`
**ungated** — it never consults health. This is the backstop for a *total*
freeze where even the health loop can't run. The device is behind a
`WatchdogOps` trait (`wardnetd-services/src/system/watchdog_ops.rs`) wired onto
`Backends` like `SystemPowerOps`/`GarpOps`: `LinuxWatchdog`
(`wardnetd/src/system/linux_watchdog.rs`, `WDIOC_SETTIMEOUT` ioctl + write-a-byte
keep-alive + `'V'` magic-close disarm) in production, `NoopWatchdog` in the mock.
`shutdown()` disarms first so a clean stop never reboots. **Invariant: the
hardware pet is never health-gated** — that is the whole point of having a
third layer below the health-gated soft restart.

## Anomaly subsystem (issues #1097 / #225)

Typed, admin-facing conditions with an open/resolved lifecycle. Replaces the
in-memory recent-errors ring buffer, which could only answer "what went wrong
recently" — the wrong question for anything that alerts.

### The catalog and the registry

[`AnomalyType`](../source/daemon/crates/wardnet-common/src/anomaly.rs) is a
hardcoded catalog; each entry carries its severity, component, remediation
hint, and coarse landing page as `const fn` accessors. Those are **derived, not
stored**, so a reworded hint applies retroactively.

`AnomalyDetectorRegistry` (`wardnetd-services/src/anomaly/registry.rs`) maps
each type to exactly one `AnomalyDetector`, built once during
`create_services` from `[anomalies.enabled]` — the same shape as
`VpnProviderRegistry`. Adding an anomaly is a catalog variant plus a
registration; nothing in the engine changes.

### Two modes, one choke point

| Mode | Driver | For |
|---|---|---|
| **Preventive** | `AnomaliesDetectionEngine` calls `detect()` on the cadence each detector declares via `interval()` | conditions that are *state* you can inspect (a blocklist's failure counter) |
| **Reactive** | `AnomalyListener` maps error-flavoured `WardnetEvent`s to reports | conditions that are *events*, with nothing left to inspect (a tunnel that failed to start) |

Both funnel into `AnomalyService::submit`, which deduplicates on
`(type, subject_id)` and notifies **only on the open edge**. That is what makes
a condition holding for days alert once instead of once per observation.
Resolution notifies only when the open did (`notified_at`), so "it is working
again" can never arrive without its "it is broken".

**Invariant: alerting is edge-triggered by a database constraint, not by
service state.** A partial unique index on
`(anomaly_type, COALESCE(subject_id, '')) WHERE resolved_at IS NULL` permits at
most one *open* anomaly per subject, so a second open attempt collides rather
than relying on anything to remember. That is what survives a restart, a lost
write, and two detectors racing. `COALESCE` is required because NULLs never
compare equal in a unique index — without it, every box-wide anomaly would open
a fresh row per observation. Resolved rows sit outside the index, so a
condition that recurs later is genuinely new and alerts again.

### Closing the loop

`reevaluate_all` asks each open anomaly's detector whether the condition still
holds. Where a cheap authoritative check exists it is used (a tunnel anomaly
clears when the tunnel reports `Up`). Where none does — `route_table_lost`,
`dhcp_conflict` — the detector declares `stale_after` instead of faking one,
and the service expires the anomaly without ever asking. A detector error or
timeout leaves the anomaly **open**: silently closing a problem we failed to
check is the worst outcome available.

The engine keeps a deadline per detector in a min-heap rather than a shared
tick, so cadences stay independent, and it holds only `Arc<dyn AnomalyService>`
— the registry, repository, and notification path all sit behind it, per the
runner contract.

**Per-entity errors are anomalies, not columns.** An open anomaly whose
`subject_id` is a tunnel *is* that tunnel's current error — which is why there
is no `tunnels.last_error`, and why `GET /api/anomalies` filters by
`subject_id`. One writer, one lifecycle, one place to look.

## Network-Zone enforcement subsystem (issue #736)

Phase 1 / CI-2 of epic #244. Turns a device's [`NetworkZone`] into nftables
rules so a zone bites on a flat shared subnet, live-reloaded with no restart.
Design rationale, verdict choices, and the default-policy caveat are in
[`docs/adr/0019-network-zone-enforcement.md`](../docs/adr/0019-network-zone-enforcement.md);
domain terms in [`CONTEXT.md`](../CONTEXT.md) ("Zone packet enforcement").

### Shape

`ZoneEnforcementService` (`wardnetd-services/src/zone_enforcement/`) is a
**separate** event-bus subscriber from the routing service — driven by
`ZoneEnforcementListener` (`wardnetd/src/zone_enforcement_listener.rs`, span
`zone_enforcement_listener{}`). It shares the `FirewallManager` + `PolicyRouter`
backends with the routing service (they cooperate on the one `wardnet` nftables
table + per-device conntrack) but owns its own rules. The two listeners are
independent: routing manages kernel policy routing, the enforcer manages packet
gating, and neither blocks the other.

### Two gates, keyed by device IP (comment UDATA, restart-survivable)

- **Egress gate** — forward-chain `drop` (`wardnet:zone:egress:<ip>`). Tunnel
  forbidden ⇒ drop `oifname wg_ward*` (a `meta oifname` + bitwise-mask + compare
  prefix match, so it matches any tunnel index without enumerating interfaces).
  Direct forbidden ⇒ drop `oifname <lan_interface>`. A packet function of the
  zone's `allowed_targets` alone — never the device's current routing target.
- **Admin-UI gate** — a new `input` base chain (accept policy) carries
  reject-with-tcp-reset rules (`wardnet:zone:adminui:<ip>`) for device→Pi :443
  and :7411 when `admin_ui_reachable = false`; DNS/DHCP pass untouched. "Connection
  refused" ⇒ TCP reset, not a silent drop. `init_wardnet_table` /
  `flush_wardnet_table` gained this `input` chain.

### Live reload + reconcile

The listener maps events to per-device recomputes, each followed by a conntrack
flush so open flows re-evaluate at once: `NetworkZoneChanged`→`apply_zone`,
`DeviceZoneChanged`/`DeviceDiscovered`→`apply_device`, `DeviceIpChanged`→
`handle_ip_change` (re-key old→new IP), `DeviceGone`→`remove_device`. Startup
`reconcile` re-applies every device's rules and drops orphaned rules for IPs no
longer backed by a device, and runs **after** `RoutingService::reconcile` (which
(re)creates + flushes the shared table).

### Closing the default-policy caveat (event + callback)

`RoutingService::set_default_policy` now emits
`WardnetEvent::DefaultPolicyChanged` (RoutingServiceImpl gained an
`EventPublisher` dep). The enforcer subscribes and, for each `Default`-ruled
device whose zone forbids the newly-resolved kind, calls back into
`RoutingService::apply_rule_for_device(id, Direct)` to unbind it — the one edge
the #735 write-time gate cannot catch. The routing engine stays zone-free; the
tradeoff (stored `Default` vs applied `Direct` divergence, re-derived each boot)
is recorded in the ADR.

**Honest limit:** same-subnet peer↔peer traffic is not affected — the daemon
never sees it on a flat L2 segment (the AP's job, or the isolate-members rung
#737).

### New-device quarantine (issue #738)

An off-by-default `quarantine_new_devices` `system_config` toggle (owned by
`NetworkZoneService`, exposed at `GET/PUT /api/network/quarantine-new-devices`).
It is **notification-only**: placement is unchanged — every new device already
lands in the `is_default_for_new` zone unconditionally (`DeviceDiscoveryServiceImpl::insert_new_device`),
and enforcement already reacts to `DeviceDiscovered`. When the toggle is on, the
**truly-first-ever** discovery path (only `insert_new_device`, never the reappear
path — so idempotent by construction) publishes a dedicated
`WardnetEvent::NewDeviceQuarantined`; `PushService` turns it into an admin
"approve this device" push. A real quarantine is achieved by pointing
`is_default_for_new` at a restrictive Guest zone (the #735 lever). Approve =
existing `PUT /api/devices/{id}/zone`. Note `DeviceDiscovered` is **not** a
valid first-ever signal (it also fires on every reconnect).

## Managed devices + retention subsystem (issue #1181)

See [ADR 0032](../docs/adr/0032-managed-devices-and-retention.md) for the
reasoning; this is the shape.

`devices.managed` is an explicit, latching column — **never** derived from
`name`. It is promoted by any *admin* configuration act and cleared only by an
explicit release. That gives the invariant everything else rests on:

> `managed = 0` implies no admin artefacts exist for this device.

which is why `DeviceRetentionRunner` can delete an unmanaged row without
checking anything else.

### Promotion

Routed through `DeviceService::mark_managed` (per the
single-service-per-repository rule) from `PrivateDnsService::grant_device`,
`InboundWgService::add_peer`, and `RoutingProfileService::set_device_profiles`.
Services that already hold `DeviceRepository` directly — `dhcp`,
`network_zone`, `zone_exception`, `dns_filter` — call `set_managed` on it
rather than acquiring a second handle; do not extend those holdings to new
services.

Two call sites are reachable by a **non-admin** caller and gate promotion on
the auth context, not on the write succeeding:
`DeviceDiscoveryService::update_device` (a device may rename itself) and
`DeviceService::set_rule` (a device may set its own routing). Promoting there
would make every guest device permanently exempt from retention.

**Adding a new per-device table? Decide whether it promotes.** If an admin
creates the row, it must — otherwise the prune will cascade it away 30 days
after the device was last seen, silently.

### Release (`POST /api/devices/{id}/release`)

Lives in `wardnetd-api/src/api/devices.rs`, **not** in `DeviceService`, and
that is forced: `InboundWgServiceImpl` and `PrivateDnsServiceImpl` both hold
`Arc<dyn DeviceService>`, so the reverse edge would be an `Arc` cycle and a
construction-order deadlock.

It reverts every artefact and sets `managed = 0` **last**, so a partial failure
leaves the device still managed — never half-released with a live credential it
is no longer recorded as owning. Every step is idempotent, so a retry
completes.

### Prune (`DeviceDiscoveryService::prune_unmanaged_devices`)

Lives on the *discovery* service because deleting the row is only half the job.
**Delete first, then evict from memory, holding `lock_for_mac(mac)` across
both.** Skipping the eviction leaves the pruned MAC in `state` with
`gone = true`, so the next observation takes the `Reappear` arm with a dangling
`device_id`; `update_last_seen_and_ip` then matches zero rows and returns
`Ok(())` silently, `handle_unknown_mac` is never reached, and the device is
invisible in the UI while its traffic flows unattributed until restart.
Evicting first is wrong the other way — an observation in the window re-inserts
from the not-yet-deleted row. Also evicts `ip_history` and `device_locks`,
which nothing bounded before.

[`NetworkZone`]: ../source/daemon/crates/wardnet-common/src/network_zone.rs
