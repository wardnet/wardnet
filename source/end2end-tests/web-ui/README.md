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

## Why HTTP + an insecure-origin flag (not HTTPS)

The daemon's `:443` listener is bound but **503-gated** behind a
throwaway placeholder certificate until a real ACME certificate is
issued — which requires DDNS and is infeasible in a compose stack. The
always-on honest surface is plain HTTP on `:7411`.

But two things need a *secure context*: the session cookie is set
`Secure` (`crates/wardnetd-api/src/api/auth.rs`), and both PWAs register
service workers. So Chromium is launched with
`--unsafely-treat-insecure-origin-as-secure=<UI_BASE_URL>` — the browser
then treats the HTTP origin as secure, the `Secure` cookie is stored and
replayed, and service workers register, all without TLS/proxy plumbing.

When daemon-owned TLS becomes testable end-to-end, the PWA install /
offline / secure-context assertions (B1/C1) can move to a real HTTPS
origin; until then this flag is the harness's secure-context shim.

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
