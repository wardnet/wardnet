# Milestone 1g: Built-in DNS Server with Ad Blocking

**Last updated:** April 14, 2026

---

## Context

Wardnet replaces Pi-hole entirely. The DNS server is a core feature — not deferred — providing recursive DNS resolution, caching, network-wide ad blocking with AdGuard-compatible filtering syntax, and per-device control. The existing DNS leak prevention (nftables DNAT per device) will be absorbed into the DNS server itself.

This milestone is split into 7 stages. Each stage lands a fully usable version — backend, API, SDK, and UI together — deployed to the Pi via a merged PR.

---

## Progress

| Stage | Status | PR | Description |
|-------|--------|----|-------------|
| 1: DNS Forwarding | Not started | | Core DNS forwarding + config UI |
| 2: Ad Blocking | Not started | | Network-wide ad blocking + management UI |
| 3: Local Records & Zones | Not started | | Custom DNS records + zones UI |
| 4: Security | Not started | | DNSSEC, rebinding, rate limiting, DoT/DoH + config UI |
| 5: Recursive Resolution | Not started | | Full recursive resolver + mode toggle UI |
| 6: Query Logging & Analytics | Not started | | DNS logs, stats, real-time stream + analytics UI |
| 7: Per-Device Controls | Not started | | Per-device ad blocking + device detail UI |

---

## Requirements

### Core DNS Resolution

| Requirement | Pi-hole | AdGuard | Technitium | Routers | Wardnet |
|---|---|---|---|---|---|
| DNS forwarding (configurable upstreams) | Yes | Yes | Yes | Yes | **Yes** |
| Recursive resolver (query root servers) | No | Yes | Yes | Yes (Unbound) | **Yes** (via `hickory-recursor`; user chooses forwarding or recursive) |
| DNS caching (TTL-aware, configurable size) | Yes | Yes | Yes | Yes | **Yes** |
| Local DNS records (A, AAAA, CNAME, TXT, MX, SRV) | Yes | Yes (rewrites) | Yes (zones) | Yes | **Yes** (zone-based, e.g. `talkdesk-id.local -> 127.0.0.1`) |
| Authoritative DNS (serve local zones) | No | No | Yes | No | **Yes** (user-defined zones: `*.local`, `*.lab`, `*.home`) |
| Conditional forwarding (domain -> specific upstream) | Limited | Yes | Advanced | Yes (pfSense) | **Yes** |
| DHCP-DNS integration (auto-register hostnames) | No | No | No | Yes (dnsmasq) | **Yes** (leases auto-register as `{hostname}.lan`) |

### Ad Blocking & Filtering

| Requirement | Pi-hole | AdGuard | Technitium | Wardnet |
|---|---|---|---|---|
| Blocklist support (URL-based, auto-download) | Yes | Yes | Yes | **Yes** |
| Automatic blocklist updates | Weekly | Yes | Yes | **Yes** (cron expression) |
| Allowlist (override blocks) | Yes | Yes | Yes | **Yes** |
| Custom block/allow rules | Yes | Yes | Yes | **Yes** |
| Per-device ad blocking toggle | No | No | Yes (per-client) | **Yes** |
| AdGuard-compatible filtering syntax | No | Yes (native) | No | **Yes** |
| Parental controls / safe search | No | Yes | No | **No** (out of scope) |

#### Filtering Syntax (AdGuard-compatible)

Wardnet adopts the AdGuard DNS filtering syntax. Supported formats:

- **Adblock-style** (primary): `||example.org^` blocks domain + all subdomains
- **Hosts-file**: `0.0.0.0 ads.example.com` (compatibility with existing blocklists)
- **Domains-only**: plain `ads.example.com` (one per line)

Special characters: `||` (domain anchor), `^` (separator), `*` (wildcard), `@@` (exception prefix), `/regex/` (patterns).

DNS-specific modifiers: `$dnstype=A|AAAA`, `$dnsrewrite=1.2.3.4`, `$client=192.168.1.0/24`, `$important`.

```
||ads.example.com^                          # Block domain + subdomains
@@||safe.ads.example.com^                   # Exception: allow this subdomain
/tracker\d+\.example\.com/                  # Regex: block tracker1, tracker2, etc.
||analytics.*^$dnstype=AAAA                 # Block IPv6 queries for analytics.*
||internal.corp^$dnsrewrite=192.168.1.50    # Rewrite to local IP
||ads.com^$client=192.168.1.100             # Block only for specific client
```

### Security

| Requirement | Pi-hole | AdGuard | Technitium | Routers | Wardnet |
|---|---|---|---|---|---|
| DNSSEC validation | Yes (caveats) | Yes | Yes | Yes (Unbound) | **Yes** (hickory-dns `dnssec-ring`) |
| DNS rebinding protection | Limited | Yes | Yes | Yes (dnsmasq) | **Yes** |
| Rate limiting (per-client) | Yes | Yes | Yes | No | **Yes** |
| DNS-over-TLS upstream | External | Yes | Yes | Yes (pfSense) | **Yes** |
| DNS-over-HTTPS upstream | External | Yes | Yes | No | **Yes** |
| Hardcoded DNS interception | No | No | No | Yes (pfSense) | **Yes** (single nftables DNAT rule) |

### Logging & Analytics

| Requirement | Pi-hole | AdGuard | Technitium | Wardnet |
|---|---|---|---|---|
| Query logging (domain, client, result, latency) | Yes | Yes | Yes | **Yes** (local SQLite, queryable via API) |
| Statistics dashboard (total, blocked %, top domains) | Yes | Yes | Yes | **Yes** |
| Real-time query stream | No | No | No | **Yes** (WebSocket, filterable by domain and client) |
| Configurable log retention | Yes | Yes | Yes | **Yes** |

DNS query logs are stored in a dedicated SQLite table (not OpenTelemetry). This keeps DNS analytics self-contained and queryable through the web UI. OpenTelemetry continues to handle daemon operational logs/traces/metrics.

### Integration with Existing Features

| Requirement | Notes |
|---|---|
| Tunnel-aware DNS routing | Devices on a tunnel -> forward to tunnel's DNS. Devices on direct -> recursive resolver or configured upstream. |
| Replaces per-device DNAT | DNS routing logic moves from nftables into the DNS server. Single blanket DNAT catches hardcoded DNS. |
| DHCP advertises Wardnet as DNS | When DNS enabled, DHCP option 6 points to Wardnet IP. |
| Event bus integration | DNS emits events (started, stopped, blocklist updated, config changed). Listens for DHCP lease events. |

---

## Implementation

### Architecture

```
dns/                          # New module (like dhcp/)
├── mod.rs
├── server.rs                 # UdpDnsServer — UDP/TCP listener, query handling
├── runner.rs                 # DnsRunner — lifecycle, blocklist updates, log cleanup
├── filter.rs                 # DnsFilter — AdGuard-syntax filtering engine
├── filter_parser.rs          # Parse adblock/hosts/domain-only rule formats
├── cache.rs                  # DnsCache — TTL-aware response cache
└── tests/

service/dns.rs                # DnsService trait + DnsServiceImpl
repository/dns.rs             # DnsRepository trait
repository/sqlite/dns.rs      # SqliteDnsRepository

api/dns.rs                    # REST endpoints
api/dns_queries_ws.rs         # WebSocket real-time query stream
```

**DNS routing absorbed into DNS server:** The server handles upstream selection based on the device's routing rule. Replaces per-device nftables DNAT. Server looks up client IP's routing target (cached in-memory, invalidated on `RoutingRuleChanged` events).

**Dual-mode resolution:** Forwarding (default, `hickory-resolver`) or recursive (`hickory-recursor`). Both share cache, filter, and logging.

### Dependencies

```toml
hickory-proto = { version = "0.25", features = ["dnssec-ring"] }
hickory-resolver = { version = "0.25", features = ["dns-over-rustls", "dns-over-https-rustls"] }
hickory-recursor = "0.25"
cron = "0.15"  # cron expression parser for blocklist schedules
```

### Database Schema

New tables: `dns_blocklists`, `dns_blocked_domains`, `dns_custom_rules`, `dns_allowlist`, `dns_custom_records`, `dns_zones`, `dns_conditional_rules`, `dns_device_adblock`, `dns_query_log`.

DNS config stored in existing `system_config` KV table (same pattern as DHCP).

### Query Flow

1. Receive UDP/TCP packet -> parse with `hickory-proto`
2. Rate limit check (per-client token bucket)
3. Custom local records / authoritative zones -> return if match
4. DHCP hostname map (`.lan` domain) -> return if match
5. Conditional forwarding rules -> use specific upstream if match
6. Cache -> return if hit
7. Ad blocking filter (AdGuard engine) -> return NXDOMAIN/`0.0.0.0` if blocked
8. DNS rebinding check -> validate no private IPs for external domains
9. Resolve upstream:
   - Tunnel-routed device -> forward to tunnel's DNS
   - Direct device, forwarding mode -> `hickory-resolver` (DoT/DoH if configured)
   - Direct device, recursive mode -> `hickory-recursor` with DNSSEC
10. Cache response, log query, return to client

### API Endpoints

```
GET    /api/dns/config                    # Get DNS configuration
PUT    /api/dns/config                    # Update DNS configuration
POST   /api/dns/config/toggle             # Enable/disable DNS server
GET    /api/dns/status                    # Server status + cache stats + filter stats

GET    /api/dns/blocklists                # List blocklists
POST   /api/dns/blocklists                # Add blocklist (name, url, cron schedule)
DELETE /api/dns/blocklists/{id}           # Remove blocklist
PUT    /api/dns/blocklists/{id}           # Update blocklist (toggle, change cron)
POST   /api/dns/blocklists/{id}/update    # Force update now

GET    /api/dns/allowlist                 # List allowlist
POST   /api/dns/allowlist                 # Add allowlist entry
DELETE /api/dns/allowlist/{id}            # Remove allowlist entry

GET    /api/dns/rules                     # List custom filter rules (AdGuard syntax)
POST   /api/dns/rules                     # Add custom rule
PUT    /api/dns/rules/{id}                # Update custom rule
DELETE /api/dns/rules/{id}                # Remove custom rule

GET    /api/dns/records                   # List custom DNS records
POST   /api/dns/records                   # Add custom record
PUT    /api/dns/records/{id}              # Update custom record
DELETE /api/dns/records/{id}              # Remove custom record

GET    /api/dns/zones                     # List authoritative zones
POST   /api/dns/zones                     # Create zone
PUT    /api/dns/zones/{id}                # Update zone
DELETE /api/dns/zones/{id}                # Delete zone

GET    /api/dns/conditional               # List conditional forwarding rules
POST   /api/dns/conditional               # Add rule
DELETE /api/dns/conditional/{id}          # Remove rule

GET    /api/dns/log                       # Query log (paginated, filter by domain + client)
GET    /api/dns/log/stream                # WebSocket real-time query stream
GET    /api/dns/stats?hours=24            # Aggregated statistics

POST   /api/dns/devices/{id}/adblock      # Set per-device ad blocking
GET    /api/dns/devices/{id}/adblock      # Get per-device ad blocking status

POST   /api/dns/cache/flush               # Flush DNS cache
```

### Files to Modify

- `wardnet-types/src/lib.rs` — add `pub mod dns`
- `wardnet-types/src/event.rs` — add DNS events
- `wardnetd/src/service/routing.rs` — remove per-device DNS redirect calls
- `wardnetd/src/firewall.rs` — add `add_dns_intercept` method
- `wardnetd/src/firewall_nftables.rs` — implement DNS intercept
- `wardnetd/src/dhcp/server.rs` — advertise Wardnet as DNS when enabled
- `wardnetd/src/config.rs` — add `DnsConfig`
- `wardnetd/src/state.rs` — add DNS service to AppState
- `wardnetd/src/bootstrap.rs` — wire DNS into startup
- `Cargo.toml` — add hickory-proto, hickory-resolver, hickory-recursor, cron
- `web-ui/src/pages/Dns.tsx` — replace placeholder
- `web-ui/src/pages/AdBlocking.tsx` — replace placeholder

### Risks

| Risk | Mitigation |
|---|---|
| Port 53 in use (systemd-resolved) | Detect on startup, log clear error. Not an issue on Raspberry Pi OS. |
| SQLite latency for per-query device lookup | In-memory cache (device IP -> routing target, 60s TTL, invalidated on events) |
| Large blocklists memory (~30-50MB) | Acceptable for Pi 4/5. Document memory requirements. |
| Query log growth (~10MB/day) | Configurable retention (default 7 days), automatic cleanup + VACUUM |
| `hickory-recursor` experimental | Default to forwarding mode. Recursive is opt-in. Fallback on error. |
| AdGuard filter syntax complexity | Core subset first, then modifiers incrementally. |

---

## Stages

Each stage ships backend + API + SDK + UI together as a deployable PR.

### Stage 1: DNS Forwarding

**Goal:** Wardnet resolves DNS queries for all LAN devices. DNS page in UI shows status and upstream config.

**Backend:**
- `wardnet-types/src/dns.rs` — `DnsConfig`, `UpstreamDns`, `DnsResolutionMode`
- SQLite migration — all DNS tables (schema ready for later stages) + seed data in `system_config`
- `DnsRepository` trait + `SqliteDnsRepository` (config read/write)
- `DnsService` trait + `DnsServiceImpl` (config + status)
- `dns/server.rs` — `DnsServer` trait, `UdpDnsServer` (UDP on port 53, `hickory-proto` parsing, `hickory-resolver` forwarding), `NoopDnsServer`
- `dns/cache.rs` — TTL-aware response cache with LRU eviction
- `dns/runner.rs` — start/stop lifecycle, event listener
- `config.rs` — add `DnsConfig { bind_address, port }`
- `state.rs` — add `dns_service` to `AppState`
- `bootstrap.rs` — wire DNS into daemon startup
- `firewall.rs` / `firewall_nftables.rs` — `add_dns_intercept()` blanket DNAT
- `service/routing.rs` — remove per-device `add_dns_redirect`/`remove_dns_redirect`
- `dhcp/server.rs` — advertise Wardnet IP as DNS when enabled
- `WardnetEvent::DnsServerStarted`, `DnsServerStopped`, `DnsConfigChanged`

**API:**
- `GET/PUT /api/dns/config`, `POST /api/dns/config/toggle`, `GET /api/dns/status`, `POST /api/dns/cache/flush`

**SDK + UI:**
- `@wardnet/js`: `DnsService` class (config, status, cache flush methods), types
- `useDns.ts`: TanStack Query hooks for config + status
- `pages/Dns.tsx`: replace placeholder — enable/disable toggle, server status card (running, cache hit rate, cache size), upstream servers list (add/remove/reorder), cache settings (size, TTL min/max, flush button)

**Tests:** Server with mock socket, cache, repository, service, API

**Checklist:**
- [ ] Types in `wardnet-types`
- [ ] Migration + seed data
- [ ] Repository trait + SQLite impl
- [ ] Service trait + impl (config/status)
- [ ] DNS server (UDP forwarding + cache)
- [ ] Runner (lifecycle)
- [ ] Config, AppState, bootstrap wiring
- [ ] Firewall: blanket DNS intercept
- [ ] Routing: remove per-device DNAT
- [ ] DHCP: advertise Wardnet as DNS
- [ ] API endpoints
- [ ] Events
- [ ] SDK: DnsService class + types
- [ ] Hooks: useDns.ts
- [ ] DNS page (status, upstream, cache)
- [ ] Unit tests
- [ ] Deploy + test on Pi

---

### Stage 2: Ad Blocking

**Goal:** Network-wide ad blocking with AdGuard syntax. Ad Blocking page shows blocklist management and allowlist.

**Backend:**
- `dns/filter_parser.rs` — parse adblock-style, hosts-file, domains-only into `ParsedRule`; modifiers ($dnstype, $dnsrewrite, $client, $important)
- `dns/filter.rs` — `DnsFilter` engine (HashSet fast path, complex rules, exceptions, regex, `Arc<RwLock>`)
- `dns/runner.rs` — cron-based blocklist download + parse + filter reload
- Repository additions — blocklist CRUD, allowlist CRUD, custom rules CRUD, blocked domain bulk storage
- Service additions — blocklist, allowlist, custom rules management
- Integrate filter into server query path (step 7)

**API:**
- Blocklist endpoints (list, add, remove, update, toggle, force update)
- Allowlist endpoints (list, add, remove)
- Custom filter rules endpoints (list, add, update, remove)

**SDK + UI:**
- SDK: blocklist, allowlist, rules methods + types
- Hooks: `useBlocklists`, `useAllowlist`, `useFilterRules`
- `pages/AdBlocking.tsx`: replace placeholder — blocklists table (name, URL, entry count, last updated, cron schedule, enabled toggle, update now button, delete), add blocklist sheet, allowlist table with add form, custom filter rules editor (textarea with AdGuard syntax, syntax help tooltip), global ad blocking toggle

**Tests:** Filter parser (all formats, modifiers, regex), filter engine, blocklist download mock

**Checklist:**
- [ ] Filter parser (adblock, hosts, domains-only, modifiers)
- [ ] Filter engine
- [ ] Cron-based blocklist updates in runner
- [ ] Repository: blocklist, allowlist, custom rules CRUD
- [ ] Service: blocklist, allowlist, custom rules methods
- [ ] API: blocklist, allowlist, rules endpoints
- [ ] Integrate filter into server query path
- [ ] SDK: blocklist, allowlist, rules methods
- [ ] Hooks: useBlocklists, useAllowlist, useFilterRules
- [ ] Ad Blocking page (blocklists, allowlist, rules editor)
- [ ] Unit tests
- [ ] Deploy + test on Pi (verify ads blocked)

---

### Stage 3: Local Records, Zones & Conditional Forwarding

**Goal:** Custom DNS records, authoritative zones, DHCP hostname integration, conditional forwarding. DNS page extended with these features.

**Backend:**
- Repository additions — records CRUD, zones CRUD, conditional forwarding CRUD
- Service additions — records, zones, conditional forwarding management
- Server: authoritative zone resolution (step 3), conditional forwarding (step 5)
- Runner: listen for `DhcpLeaseAssigned` events, maintain in-memory hostname->.lan map

**API:**
- Records endpoints (list, add, update, remove)
- Zones endpoints (list, create, update, delete)
- Conditional forwarding endpoints (list, add, remove)

**SDK + UI:**
- SDK: records, zones, conditional forwarding methods + types
- Hooks: `useCustomRecords`, `useDnsZones`, `useConditionalForwarding`
- `pages/Dns.tsx` additions: custom DNS records table (domain, type, value, TTL, zone — CRUD), zones management section (create/edit/delete zones), conditional forwarding rules table (domain -> upstream — CRUD), DHCP hostnames info (auto-registered `.lan` entries)

**Tests:** Local record resolution, zone authority, conditional forwarding, DHCP hostname integration

**Checklist:**
- [ ] Repository: records, zones, conditional forwarding CRUD
- [ ] Service: records, zones, conditional forwarding methods
- [ ] API: records, zones, conditional forwarding endpoints
- [ ] Server: authoritative zone resolution
- [ ] Server: conditional forwarding in query path
- [ ] Runner: DHCP lease event -> hostname map
- [ ] SDK: records, zones, conditional methods
- [ ] Hooks: useCustomRecords, useDnsZones, useConditionalForwarding
- [ ] DNS page: records table, zones section, conditional forwarding table
- [ ] Unit tests
- [ ] Deploy + test on Pi

---

### Stage 4: Security

**Goal:** DNSSEC, rebinding protection, rate limiting, encrypted upstream. DNS page extended with security settings.

**Backend:**
- DNSSEC: `dnssec-ring` feature, forwarding (DO bit + AD flag), recursive (full validation)
- Rebinding protection: reject private IPs in responses to external domains
- Rate limiting: per-client-IP token bucket
- DoT/DoH: `hickory-resolver` encrypted features, `UpstreamDns.protocol` Tls/Https variants
- Config additions in `system_config`

**API:**
- Existing `PUT /api/dns/config` extended with DNSSEC, rebinding, rate limit fields

**SDK + UI:**
- SDK: config types updated with security fields
- `pages/Dns.tsx` additions: security settings section — DNSSEC toggle, rebinding protection toggle, rate limit config (queries/sec, 0=off), upstream protocol selector per server (UDP/TCP/TLS/HTTPS)

**Tests:** DNSSEC validation, rebinding rejection, rate limiter, DoT/DoH

**Checklist:**
- [ ] DNSSEC validation (forwarding + recursive modes)
- [ ] DNS rebinding protection
- [ ] Per-client rate limiting
- [ ] DoT upstream support
- [ ] DoH upstream support
- [ ] Config additions
- [ ] SDK: updated config types
- [ ] DNS page: security settings section
- [ ] Unit tests
- [ ] Deploy + test on Pi

---

### Stage 5: Recursive Resolution

**Goal:** Full recursive resolver via root servers. DNS page adds resolution mode toggle.

**Backend:**
- Integrate `hickory-recursor` crate
- Bundle root hints file (IANA `named.root`)
- Resolution mode toggle (`forwarding` / `recursive`) in config
- Server: route direct-device queries through recursor when in recursive mode
- Fallback: on recursor failure, log warning and attempt forwarding

**API:**
- Existing `PUT /api/dns/config` extended with `resolution_mode`

**SDK + UI:**
- SDK: config types updated with resolution mode
- `pages/Dns.tsx` additions: resolution mode selector (forwarding / recursive), info text explaining each mode, upstream servers section hidden in recursive mode

**Tests:** Recursive resolution mock, mode switching, fallback

**Checklist:**
- [ ] `hickory-recursor` integration
- [ ] Root hints bundling
- [ ] Resolution mode toggle in config
- [ ] Server: recursive path in query flow
- [ ] Fallback to forwarding on error
- [ ] SDK: updated config types
- [ ] DNS page: mode selector
- [ ] Unit tests
- [ ] Deploy + test on Pi

---

### Stage 6: Query Logging & Analytics

**Goal:** Full DNS observability. Ad Blocking page extended with query log and stats dashboard.

**Backend:**
- Server: log each query to async channel (non-blocking)
- Runner: batch drain channel to `dns_query_log` SQLite, periodic cleanup
- Repository additions — query log batch insert, paginated read, stats aggregation
- Service additions — query log, stats
- `api/dns_queries_ws.rs` — WebSocket real-time query stream (broadcast channel, filter by domain + client)

**API:**
- `GET /api/dns/log` (paginated, filter by domain + client)
- `GET /api/dns/stats?hours=24`
- `GET /api/dns/log/stream` (WebSocket)

**SDK + UI:**
- SDK: query log, stats methods + types, WebSocket client helper
- Hooks: `useQueryLog`, `useDnsStats`, `useQueryStream`
- `pages/AdBlocking.tsx` additions: stats dashboard (total queries, blocked count, blocked %, queries over time chart), top domains table, top blocked table, top clients table, real-time query log viewer (WebSocket-powered, columns: timestamp, device/client, domain, type, result badge, latency — filter by domain + client)
- Dashboard additions: DNS stat cards (queries today, ads blocked today with %)

**Tests:** Batch insert, log cleanup, stats aggregation, WebSocket stream

**Checklist:**
- [ ] Server: async query logging channel
- [ ] Runner: batch insert to SQLite + cleanup
- [ ] Repository: log insert, paginated read, stats queries
- [ ] Service: query log, stats methods
- [ ] API: log endpoint, stats endpoint
- [ ] WebSocket: real-time query stream
- [ ] SDK: log, stats, stream methods
- [ ] Hooks: useQueryLog, useDnsStats, useQueryStream
- [ ] Ad Blocking page: stats dashboard, query log viewer
- [ ] Dashboard: DNS stat cards
- [ ] Unit tests
- [ ] Deploy + test on Pi

---

### Stage 7: Per-Device Controls

**Goal:** Per-device ad blocking toggle integrated into device management.

**Backend:**
- Repository: `dns_device_adblock` CRUD
- Service: `set_device_adblock`, `get_device_adblock`
- Filter: check `per_device_disabled` set during query evaluation

**API:**
- `POST /api/dns/devices/{id}/adblock`, `GET /api/dns/devices/{id}/adblock`

**SDK + UI:**
- SDK: device adblock methods
- Hooks: `useDeviceAdblock`
- Device detail sheet/page: "Ad blocking" toggle with status indicator
- Devices table: ad blocking status column or badge

**Tests:** Per-device toggle, filter respects disabled devices

**Checklist:**
- [ ] Repository: device adblock CRUD
- [ ] Service: device adblock methods
- [ ] Filter: per-device disable check
- [ ] API: device adblock endpoints
- [ ] SDK: device adblock methods
- [ ] Hooks: useDeviceAdblock
- [ ] Device detail: ad blocking toggle
- [ ] Devices table: ad blocking indicator
- [ ] Unit tests
- [ ] Deploy + test on Pi
