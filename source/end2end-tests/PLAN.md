# End-to-end test plan

Living document for the wardnet e2e initiative. Adjacent to the two
suites it governs:

- `source/end2end-tests/daemon/` — API + kernel specs (Vitest + JS SDK
  + `wardnet-test-agent` probes); already in place through Stage 6c.
- `source/end2end-tests/web-ui/` — browser specs (Playwright); not yet
  scaffolded. **Stage U-0** below sets it up.

The daemon's `compose.yaml` header points readers here for the full
topology and roadmap.

The suites drive the real `wardnetd` binary in a Docker topology, hit
its HTTP API via the typed JS SDK, and probe kernel state from inside
LAN clients via `wardnet-test-agent`. The UI suite drives the React app
served by `wardnetd` (rust-embed) through Playwright. No mocks of the
daemon itself — mocks live only at the *upstream* boundary (NordVPN
API, blocklist HTTP servers, future update manifest server).

## Coverage today (Stages 6a–6c, PR #207)

### Daemon suite

| Spec | Touches |
|---|---|
| `health.spec.ts` | `/api/info` (unauth), setup wizard idempotent, empty tunnels list |
| `dhcp.spec.ts` | DHCP toggle, pool narrowing, dynamic lease in `.100–.150`, renew = same IP |
| `dhcp-reservations.spec.ts` | reservation in `.151–.199`, force re-DISCOVER, reserved IP returned, delete |

Helpers: `helpers.ts` covers `WardnetClient` auth (bearer),
`waitForReady`, `acquireLeaseInRange`, `agentGet`/`agentPost`,
range-aware `ipv4InRange`.

### Web UI suite

Nothing yet — no test framework wired up; only `lint` / `type-check` /
`format:check` exist on `web-ui/package.json`. Stage U-0 introduces
Playwright.

## Topology today

```
                 wardnet_mgmt 10.90/24            wardnet_lan 10.91/24
                       │                                  │
  test_runner ─────────┤                                  ├───── test_debian (eth0, dhclient)
   .10                 │                                  │       :3001 client serve
                       │      ┌─────────────────┐         │
                       └──────│    wardnetd     │─────────┤
                       .2     │  10.90.0.2 mgmt │  .1     │
                              │  10.91.0.1  lan │         ├───── test_ubuntu (eth0, dhclient)
                              │  10.92.0.2  wan │         │       :3001 client serve
                              └────────┬────────┘
                                       │
                                wardnet_wan 10.92/24 (provisioned, no peers yet)
```

## Untested surface (inventory)

Asterisk (\*) marks day-zero scope per project memory ("ad blocking is
day-zero, replaces Pi-hole").

### API / daemon

Grouped by SDK service (`source/sdk/wardnet-js/src/services/`).

- **DnsService** \* — `getConfig` / `updateConfig` / `toggle` /
  `status` / `flushCache`; blocklists CRUD + `updateBlocklistNow` job;
  allowlist CRUD; custom filter rules CRUD. **Plus** end-to-end
  resolution from a LAN client via `/dns/resolve` probe.
- **TunnelService** — `create` (import `.conf`), `delete`, plus the
  bring-up/tear-down lifecycle and stats collection (currently observed
  only via `list`).
- **DeviceService** — `list`, `getById`, `getMe` (source-IP), `setMyRule`,
  admin-lock enforcement, `update`.
- **ProviderService** — `list`, `listCountries`, `validateCredentials`,
  `listServers`, `setupTunnel`. Requires a NordVPN-API mock.
- **BackupService** — `export`, `previewImport`, `applyImport`,
  `listSnapshots` round-trip with a known reservation/blocklist.
- **UpdateService** — `status`, `check`, `install`, `rollback`,
  `updateConfig`, `history`. Requires a manifest-server mock.
- **SystemService** — `getStatus`, `restart`.
- **AuthService** — login negatives (bad creds, missing/expired token),
  setup wizard reject-after-completion path.
- **JobsService** — exercised indirectly via blocklist refresh; no
  dedicated spec.
- **LogService** WebSocket — connect, receive, `set_filter`,
  pause/resume, lag handling.

Plus two **infra capabilities** not yet deployed:

1. The test-agent's *server mode* (`/pid`, `/ip-rules`, `/nft-rules`,
   `/wg/{iface}`, `/link/{iface}`, `/fixtures/{name}`) — required to
   assert kernel state on the wardnetd side. Today only the LAN-side
   `client serve` probes are running. Need a sidecar (or daemon-side
   co-located process) exposing this on `wardnet_mgmt` to the runner.
2. **LAN-side API proxy** — `/devices/me` and `/devices/me/rule` are
   classified by source IP. To drive them as a real LAN client we need
   the test-agent's `client serve` to grow a thin
   `POST /proxy { method, path, body }` that forwards to
   `http://wardnetd:7411/api/...` from inside the client's network
   namespace.

### Web UI

The suite must cover every page at least at the smoke level (loads,
no console errors, reachable from the layout) and exercise the primary
flows on each. Pages on disk today (`source/web-ui/src/pages/`):
`Setup`, `Login`, `Dashboard`, `Devices`, `MyDevice`, `Dhcp`, `Dns`,
`AdBlocking`, `Tunnels`, `Settings`, `NotFound`.

Untested flows worth calling out:
- First-run setup wizard.
- Login form (good + bad creds, redirect to original target).
- Devices table → drill into device → change routing rule.
- DHCP page — toggle, edit pool, create/delete reservation.
- Ad-blocking page \* — add blocklist, force update, see status,
  add/remove allowlist entry, add/remove custom rule.
- DNS page — toggle, edit upstream, flush cache.
- Tunnels page — import a `.conf`, see it listed, delete.
- Settings page — admin password change, restart daemon (modal/progress
  dialog).
- NotFound, layout sidebar/topnav, theme toggle, mobile menu.

## Staged rollout

Stages are ordered by dependency and value. Each stage = one PR.

### Daemon lane

#### Stage 7 — DNS subsystem \* (LAN topology, no new client/gateway containers)

Specs:
- `dns-config.spec.ts` — toggle on/off, `getConfig` round-trip,
  `status` reports running + upstream, `flushCache` clears counters.
- `dns-resolve.spec.ts` — drive test_debian's
  `/dns/resolve?server=10.91.0.1` for a public name; verify daemon
  answered (cache miss → hit on second call).
- `dns-blocklists.spec.ts` — bring up a tiny static-file HTTP server
  on `wardnet_lan` serving a 3-line hosts blocklist; `createBlocklist`
  pointing at it, `updateBlocklistNow`, poll `JobsService.get` until
  `succeeded`; resolve a listed name → NXDOMAIN / 0.0.0.0.
- `dns-allowlist.spec.ts` — same blocklist, then `createAllowlistEntry`
  for one domain → resolves normally; delete restores blocked behavior.
- `dns-rules.spec.ts` — custom block rule (regex/glob), custom allow
  rule overrides a blocklisted domain; precedence semantics.

Infra: `blocklist_server` container on `wardnet_lan` (busybox httpd or
nginx:alpine) serving fixtures from a bind-mounted dir.

Helpers: `resolveViaAgent(agent, name, server?)`, `waitForJob(jobs, id, timeoutMs)`.

#### Stage 8 — Devices & per-device routing (LAN-only)

Specs:
- `devices-list.spec.ts` — both LAN clients with leases → ARP
  discovery surfaces them in `/devices`; `getById` returns expected
  fields.
- `devices-me.spec.ts` — drive `/devices/me` from each LAN client via
  the proxy; verify source-IP classification.
- `devices-rules.spec.ts` — `setMyRule` to `Direct` / `Block`; assert
  the resulting `ip rule` set on the daemon side via the kernel-state
  agent.
- `admin-lock.spec.ts` — admin locks device → `setMyRule` returns 403.

Infra:
- Extend `wardnet-test-agent` (`client serve`) with `POST /proxy`
  forwarding to a configured base URL.
- Deploy the test-agent's *server mode* alongside wardnetd. Two
  options, decide during impl:
  - (a) sidecar container sharing the daemon's network namespace
        (`network_mode: "service:wardnetd"`).
  - (b) co-locate a second binary inside the wardnetd container,
        launched by a systemd unit drop-in.

#### Stage 9 — Tunnels & WireGuard topology

Add `wg_gateway_1`, `wg_gateway_2` on `wardnet_wan` (real `wg`; static
keys baked into fixtures).

Specs:
- `tunnel-import.spec.ts` — create from fixture conf, `list` shows it,
  delete clears it. No bring-up.
- `tunnel-bringup.spec.ts` — bring up via the path the daemon supports
  (today: assigning a device to the tunnel); kernel-state agent confirms
  `wg show wgN` reports the peer; tear-down removes the interface.
- `tunnel-stats.spec.ts` — drive ping through the tunnel from a routed
  device; `list` shows non-zero rx/tx counters.

Open question: `TunnelService` SDK only exposes `list` / `create` /
`delete`. Bring-up is automatic when a device targets the tunnel. The
bring-up spec will set a device rule and observe the tunnel-up event
indirectly — confirm with the daemon team before adding a `bringUp`
SDK method.

#### Stage 10 — VPN provider integration

Add `nordvpn_mock` container — small HTTP server (Node) implementing
the subset of the NordVPN API that
`wardnetd-services/src/vpn/nordvpn.rs` calls. Static fixture data.

Specs:
- `provider-list.spec.ts` — NordVPN registered.
- `provider-validate.spec.ts` — happy-path token + 401 path.
- `provider-countries.spec.ts` — listCountries returns fixture set.
- `provider-setup.spec.ts` — `setupTunnel` creates a tunnel; appears
  in `tunnels.list()`; cleanup deletes it.

#### Stage 11 — System, backup, update

- `system-status.spec.ts` — version/uptime fields, device count.
- `system-restart.spec.ts` — request restart, `/api/info` recovers.
  Order-sensitive — keep in its own file, run via setup that re-runs
  `waitForReady` on the next spec.
- `backup-roundtrip.spec.ts` — pre-seed a unique reservation and
  blocklist; export bundle (passphrase ≥12 chars), delete the seeds,
  `previewImport` + `applyImport` of the same bundle, verify both
  reappear; `listSnapshots` shows the pre-restore snapshot.
- `update-status.spec.ts` — read `status`; switch channel via
  `updateConfig`; `check` against a fixture manifest server returns
  "no update available" or a fixture release.

Infra: `update_manifest_server` (Stage 11) — static-file HTTPS server
with the daemon's expected JSON shape. May reuse the blocklist HTTP
fixture container with TLS termination.

#### Stage 12 — Auth negatives & coverage

- `auth-login.spec.ts` — wrong password → 401; missing bearer → 401
  on admin endpoint; setup wizard returns "already completed" once
  flipped.
- `auth-coverage.spec.ts` — table-driven matrix asserting representative
  admin endpoints reject unauth (DHCP config, DNS config, devices list,
  tunnels list, backup status). One spec.

#### Stage 13 — Logs WebSocket

- `logs-stream.spec.ts` — connect via `LogService` (Node uses undici's
  WebSocket polyfill), receive ≥1 entry, `set_filter`, pause/resume.

### Web UI lane (Playwright)

The UI is served by `wardnetd` (rust-embed in `web.rs`). The Playwright
runner is a new container in the same compose, pointed at
`http://wardnetd:7411/`. Specs share the same daemon as the daemon-suite
specs but live in a parallel directory.

Layout under `source/end2end-tests/web-ui/`:
- `playwright.config.ts` — Chromium only initially (Firefox/WebKit
  optional later); JUnit reporter into `reports/`.
- `tests/` — one file per page or flow.
- `Dockerfile.ui-runner` — `mcr.microsoft.com/playwright:v1.x-jammy`
  base.
- Compose service `ui_runner` on `wardnet_mgmt`, mirroring `test_runner`.

#### Stage U-0 — Scaffold (no specs yet)

- Add Playwright deps and config; `yarn` install pinned versions
  (deterministic CI).
- New compose service `ui_runner` with health gate on wardnetd, a
  bind mount for `reports/`, repo mounted read-only.
- Add `make e2e-ui` (and roll into `make e2e-all`) — symmetric with
  the daemon target.
- One smoke spec `home.spec.ts`: navigate to `/`, expect the app
  shell renders without console errors.
- CI: extend `.github/workflows/tests-e2e.yml` with a parallel UI job.

#### Stage U-1 — Auth & setup flow

- `setup.spec.ts` — first-run wizard happy path: navigate to fresh
  daemon (fixture: backup/import a "blank" snapshot, or a separate
  compose project for isolation), submit credentials, redirected to
  Dashboard.
- `login.spec.ts` — bad creds shows error; good creds redirects;
  deep-link redirect target preserved.

Open question: setup wizard is one-shot. Two options:
- (a) Run UI specs first while daemon is still pre-setup. Daemon
      specs after will pick up the admin user the UI created. Removes
      the password constant from `helpers.ts` — it must instead be
      written by the UI spec and read by daemon specs.
- (b) Run UI specs against a dedicated compose project (separate
      daemon instance). More isolation, more CI minutes.

Recommended: (a) for cost; share the password through an env var the
UI spec sets and the daemon-suite helper reads.

#### Stage U-2 — Dashboard, layout, theme

- `dashboard.spec.ts` — primary tiles render with seeded data.
- `layout.spec.ts` — sidebar links navigate; mobile menu opens;
  theme toggle persists.
- `notfound.spec.ts` — unknown route shows 404 page.

#### Stage U-3 — DHCP page

- `dhcp.spec.ts` — toggle on/off, edit pool start/end (form
  validation), create reservation (use the reservation range from
  the daemon-side helper to avoid colliding), delete reservation.
- `dhcp-leases.spec.ts` — leases table populated after LAN clients
  have leases (depends on the daemon-suite Stage 6c state).

#### Stage U-4 — DNS + AdBlocking pages \*

- `dns.spec.ts` — toggle, edit upstream, flush cache, status banner.
- `adblocking.spec.ts` — add blocklist (URL fixture from the
  blocklist_server in Stage 7 daemon infra), force update, status
  reaches `succeeded`; add allowlist entry; add custom rule.

#### Stage U-5 — Devices & MyDevice

- `devices.spec.ts` — table shows discovered LAN clients, drill-in
  page shows fields, change routing rule.
- `mydevice.spec.ts` — when accessed *from* a LAN client (Playwright
  driving Chromium *inside* the client container, see infra below),
  shows the calling device.

Infra for `mydevice`: optionally run a second Playwright runner on
`wardnet_lan` so the source IP matches a discovered device. Skip if
costly — assert the API behavior in the daemon suite instead.

#### Stage U-6 — Tunnels & Providers

- `tunnels.spec.ts` — import from `.conf` (file upload), see in list,
  delete. Depends on Stage 9 daemon infra (gateway containers) only
  if we want to actually bring the tunnel up; configuration import
  alone doesn't.
- `providers.spec.ts` — wizard through the provider setup flow
  (NordVPN), depends on Stage 10 daemon infra (`nordvpn_mock`).

#### Stage U-7 — Settings

- `settings.spec.ts` — admin password change (then re-login),
  restart daemon shows progress dialog and recovers, language/theme
  persistence.

#### Stage U-8 — Visual regression (optional)

- Playwright screenshot snapshots for Dashboard, AdBlocking, Devices.
  CI baseline maintained per platform; update flow documented in
  `web-ui/README.md`.

## Cross-cutting conventions

- One daemon, shared across spec files (`isolate: false`,
  `singleFork: true` for Vitest; Playwright `fullyParallel: false`,
  `workers: 1`). Specs MUST clean up after themselves OR be written
  order-tolerant. Setup wizard, DHCP toggle, and DHCP pool config
  are idempotent; new specs must keep that property.
- IP ranges, per the topology comments in `compose.yaml`:
  - `.100–.150` dynamic DHCP pool
  - `.151–.199` reservation range
  - `.200+` static service addresses (blocklist server, mocks)
- All admin-authed work (daemon suite) reuses
  `ensureAdminAndLogin(client)` — it's idempotent and reads the
  shared module-scoped admin password constant. UI Stage U-1
  changes how this constant is sourced (see open question above).
- New helpers go in `helpers.ts` (daemon) or `tests/fixtures/` (UI).
  Keep specs declarative.
- Each new container is added to `compose.yaml` with a `healthcheck`
  and `depends_on: { condition: service_healthy }` from the runner.
- macOS dev: `cargo clippy --workspace` fails on linux-only crates;
  scope clippy to touched crates and rely on `make check-daemon` for
  the full pass before push.

## Non-goals

- No multi-host scenarios (Pi cluster) — single-daemon only.
- No load / benchmark tests — correctness only.
- No DPI / packet-content assertions beyond what the test-agent's
  `/ping` and `/dns/resolve` give. Wire-level inspection remains a
  manual ops task.
- No accessibility audit suite — covered by lint plugins on the UI
  side, not e2e.
