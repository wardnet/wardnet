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
