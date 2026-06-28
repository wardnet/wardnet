# Web UI end-to-end tests (Playwright)

Playwright suite covering Wardnet's three frontend surfaces, all embedded
in `wardnetd` via rust-embed and served on one origin:

| Project      | Surface                | Path          |
| ------------ | ---------------------- | ------------- |
| `admin-site` | desktop admin SPA      | `/admin/`     |
| `admin-app`  | admin mobile PWA       | `/admin-app/` |
| `user-app`   | device-keyed user PWA  | `/`           |

This is the **PW-0 scaffold** (epic #614): harness + one smoke spec per
surface. Feature coverage lands in the A/B/C-stage sub-issues.

## Run

```sh
make e2e-ui      # build web → real-asset daemon image → compose → run suite
make e2e-all     # daemon (Vitest) + web-ui (Playwright)
```

The suite runs inside the `ui_runner` container against a dedicated,
self-seeded `wardnetd-ui` instance (`compose.ui.yaml`) — isolated from
the API/kernel Vitest suite under `../daemon`. JUnit + an HTML report are
written to `reports/`.

### LAN client (`test_debian`)

A real Debian container running `wardnet-test-agent client serve` on `:3001`
is attached to `wardnet_lan`. DHCP-lease specs drive it via
`fixtures/dhcp.ts:seedDhcpLease` (calls `/dhcp/renew` until a daemon-issued
address lands on `eth0`). `ui_runner` depends on it with
`condition: service_started`, not `service_healthy` — a flaky client should
fail only the leases spec, not abort the whole suite. The IPAM range
(`.2–.15`) sits below the pool the spec sets (`.100–.150`), so any `.100+`
address is unambiguously daemon-issued. Agent helpers in `fixtures/dhcp.ts`
are ported from `source/end2end-tests/daemon/tests/helpers.ts` because this
harness deliberately avoids importing the daemon package.

### Blocklist fixture server (`blocklist_server`)

A static HTTP server (Caddy `file-server`, same pinned image as `tls_proxy`)
on `wardnet_mgmt` at the fixed IP `10.90.0.20`, serving
`fixtures/blocklist/hosts.txt`. The ad-blocking spec (`adblocking.spec.ts`)
adds a blocklist pointing at `http://10.90.0.20/hosts.txt`
(`WARDNET_BLOCKLIST_FIXTURE_URL`) and forces an update; the **daemon** fetches
and parses it, so this exercises the real download path (the daemon's
downloader uses a plain `reqwest` client — no HTTPS/SSRF guard). The URL uses
the server's IP, not its Docker service name, because the daemon is itself the
DNS resolver on this stack. `ui_runner` depends on it with
`condition: service_healthy`.

## Why a self-signed TLS proxy (HTTPS)

Two things need a *secure context*: the daemon's session cookie is set
`Secure` (`crates/wardnetd-api/src/api/auth.rs`), and both PWAs register
service workers. The browser therefore needs a real HTTPS origin.

The daemon's own `:443` can't provide it here — it's **503-gated**
behind a placeholder certificate until a real ACME cert is issued, which
needs DDNS and is infeasible in a compose stack (`:7411` is the
always-on plain-HTTP surface). So a **Caddy sidecar (`tls_proxy`)**
terminates TLS with an auto-generated self-signed cert (`tls internal`)
and reverse-proxies to each daemon's `:7411`; Playwright trusts it via
`ignoreHTTPSErrors`. The browser hits `https://wardnetd-ui-tls` /
`https://wardnetd-ui-fresh-tls` (Caddy routes by Host/SNI).

This replaced an earlier plain-HTTP +
`--unsafely-treat-insecure-origin-as-secure` approach: that flag is
ignored by headless Chromium (playwright#22944, so the `Secure` cookie
was dropped) and hung under `xvfb` when run headed. Real TLS keeps the
suite headless and makes the cookie + service workers work natively.

Node-side seeding (`global.setup`) still talks to the daemon's
plain-HTTP `:7411` directly — it reads the login token from the response
body and never relies on the cookie, avoiding self-signed-TLS handling
in Node.

## Auth

The `setup` project seeds the admin and completes the wizard over the
REST API (plain `fetch` — see `fixtures/seed.ts` for why not the
source-only `@wardnet/js` SDK), then writes `.auth/admin.json` carrying
the `wardnet_session` cookie (built from the login token). The
`admin-site` and `admin-app` projects reuse it via `storageState`.
`user-app` is unauthenticated (device-keyed); from the mgmt-side runner
it shows the no-device state.

## Local overrides

- `WARDNET_UI_BASE_URL` — daemon base URL the browser hits (default
  `http://wardnetd-ui:7411`; e.g. `http://localhost:7411` against a
  locally port-mapped daemon).

## Selector convention

This is the **authoritative** selector convention for the whole web-ui
Playwright suite. `.agents/testing.md` links here; the rationale is
recorded in
[`docs/adr-e2e-selector-convention.md`](../../../docs/adr-e2e-selector-convention.md).

**`data-testid` is the primary locator.** Locate every element with
`page.getByTestId(...)`, then **additionally assert** the human-facing
label / role / text where it is meaningful. The testid keeps locators
stable across the pending branding re-skin and copy/DOM churn (a spec
only changes when the contract changes); the extra label assertion
preserves accessibility/intent coverage.

Rules:

1. **Attribute**: `data-testid` (Playwright's zero-config default — no
   `testIdAttribute` override).
2. **Naming**: flat, kebab-case, area-prefixed — `nav-devices`,
   `mobile-menu-trigger`, `stat-devices`, `page-title`, `login-username`,
   `notfound-page`. Per-surface project scoping prevents cross-surface
   clashes, so no namespacing.
3. **Placement**: declare testids on **app components** (admin-site /
   admin-app / user-app) and the shared `@wardnet/web` components (e.g.
   `LoginForm`); forward them through `@wardnet/ui` primitives via their
   existing `...props` spread (`Button`, `Input`, `StatTile` already
   forward). Complex primitives that expose several independently-targetable
   slots (e.g. `data-table` with `searchTestId`, `addTestId`,
   `rowActionsTestId`; `DataTableGroup.testId`; `RowAction.testId`) use
   explicit named props instead of a single spread — the principle is the
   same: consumers supply testids, the primitive never hardcodes them.
4. **Label assertion**: assert a label/role/text only on elements that
   carry a meaningful one — interactive controls (`toContainText`,
   `toHaveRole`, `toHaveAttribute("aria-current", …)`) and headings
   (asserted via `getByRole("heading", …)`, which doubles as both the
   step gate and the label). Structural containers get a testid but no
   text assertion.
5. **Scope**: add testids as specs need them. Don't pre-seed testids for
   elements no spec exercises.

A note on `getByRole("heading", …)` in the setup wizard: each step
transition is gated on the next step's heading. The heading is the
meaningful label, so the role-based assertion satisfies rule 4 — the
interactive controls within the step are still located by testid.
