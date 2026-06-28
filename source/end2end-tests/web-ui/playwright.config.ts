import { defineConfig, devices } from "@playwright/test";

import {
  STORAGE_STATE,
  UI_BASE_URL,
  UI_FRESH_BASE_URL,
} from "./fixtures/seed.js";

/**
 * Playwright harness for Wardnet's three web surfaces, each embedded in
 * `wardnetd` via rust-embed and served on one origin:
 *   - admin-site (desktop SPA) → `/admin/`
 *   - admin-app  (mobile PWA)  → `/admin-app/`
 *   - user-app   (device PWA)  → `/`
 *
 * The browser reaches the daemon over a self-signed HTTPS proxy
 * (`tls_proxy`, Caddy) rather than the daemon's plain-HTTP :7411: a real
 * HTTPS origin is what lets the daemon's `Secure` session cookie be
 * stored and PWA service workers register. (The daemon's own :443 is
 * 503-gated until an ACME cert is issued, infeasible in compose; the
 * earlier `--unsafely-treat-insecure-origin-as-secure` route is ignored
 * by headless Chromium — playwright#22944 — and hung under xvfb when run
 * headed, so a TLS proxy + `ignoreHTTPSErrors` is the robust path.)
 *
 * One shared daemon backs every spec, so file parallelism is hostile
 * (race on shared state): `fullyParallel:false`, `workers:1`.
 */

const CHROMIUM_ARGS = [
  // Chromium runs as root in the Playwright image — required or it
  // refuses to launch.
  "--no-sandbox",
  // Docker's default /dev/shm is 64 MB; Chromium can exhaust it. Use /tmp.
  "--disable-dev-shm-usage",
  // Trust the tls_proxy's self-signed cert at the BROWSER level, not just the
  // context level. `ignoreHTTPSErrors` (set in `use` below) covers page
  // navigations and fetches, but NOT the service-worker *script* fetch:
  // registering a SW from an origin with a cert error fails with
  // "An SSL certificate error occurred when fetching the script" even with
  // `ignoreHTTPSErrors`. This flag disables cert verification process-wide so
  // the admin-app / user-app service workers register and the offline-shell
  // specs can run. (Harmless here — the whole stack is a throwaway self-signed
  // proxy; see README.md → "Why a self-signed TLS proxy".)
  "--ignore-certificate-errors",
];

export default defineConfig({
  testDir: ".",
  fullyParallel: false,
  workers: 1,
  retries: 0,
  // Whole-suite ceiling so a stuck browser launch fails fast with output
  // instead of hanging the CI job (8 short tests finish in minutes).
  globalTimeout: 15 * 60_000,
  // Generous ceilings: compose health waits + first-boot setup push the
  // setup project past Playwright's 30 s default on a cold stack.
  timeout: 60_000,
  expect: { timeout: 15_000 },
  reporter: [
    ["list"],
    ["junit", { outputFile: "reports/junit.xml" }],
    // `open:'never'` — the HTML reporter otherwise spawns a server and
    // hangs the container/CI run on completion.
    ["html", { outputFolder: "reports/html", open: "never" }],
  ],
  use: {
    // Trust the proxy's self-signed cert.
    ignoreHTTPSErrors: true,
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  projects: [
    // Runs first: seeds the admin via the SDK and writes the admin
    // session into STORAGE_STATE for the authed surfaces to reuse.
    { name: "setup", testMatch: "fixtures/global.setup.ts" },
    {
      name: "admin-site",
      testMatch: "tests/admin-site/**/*.spec.ts",
      // setup.spec runs the one-shot wizard on the pristine daemon
      // (admin-site-setup project) — exclude it from the seeded surface.
      testIgnore: "tests/admin-site/setup.spec.ts",
      dependencies: ["setup"],
      use: {
        ...devices["Desktop Chrome"],
        baseURL: `${UI_BASE_URL}/admin/`,
        storageState: STORAGE_STATE,
        launchOptions: { args: CHROMIUM_ARGS },
      },
    },
    {
      // First-run setup-wizard UI, walked on the never-seeded
      // `wardnetd-ui-fresh` from a clean state. No `setup` dependency
      // and no storageState — the wizard creates the admin itself.
      name: "admin-site-setup",
      testMatch: "tests/admin-site/setup.spec.ts",
      use: {
        ...devices["Desktop Chrome"],
        baseURL: `${UI_FRESH_BASE_URL}/admin/`,
        launchOptions: { args: CHROMIUM_ARGS },
      },
    },
    {
      name: "admin-app",
      testMatch: "tests/admin-app/**/*.spec.ts",
      dependencies: ["setup"],
      use: {
        ...devices["Pixel 7"],
        baseURL: `${UI_BASE_URL}/admin-app/`,
        storageState: STORAGE_STATE,
        launchOptions: { args: CHROMIUM_ARGS },
      },
    },
    {
      // Device-keyed, no login. From the mgmt-side runner the source IP
      // is not a discovered LAN device, so the app renders its
      // no-device state. Real device flows arrive with the LAN-side
      // runner in C1 (#626).
      name: "user-app",
      testMatch: "tests/user-app/**/*.spec.ts",
      dependencies: ["setup"],
      use: {
        ...devices["Pixel 7"],
        baseURL: `${UI_BASE_URL}/`,
        launchOptions: { args: CHROMIUM_ARGS },
      },
    },
  ],
});
