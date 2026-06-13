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
 * Why plain HTTP + an insecure-origin flag (not HTTPS): the daemon's
 * `:443` is 503-gated behind a placeholder cert until an ACME cert is
 * issued, which needs DDNS and is infeasible in a compose stack. `:7411`
 * is the always-on plain-HTTP surface, but the session cookie is
 * `Secure` and the PWAs register service workers — both need a secure
 * context. Launching Chromium with
 * `--unsafely-treat-insecure-origin-as-secure` makes the browser treat
 * the HTTP origin as secure, so the cookie stores and SWs register
 * without any TLS/proxy plumbing. See README.md.
 *
 * One shared daemon backs every spec, so file parallelism is hostile
 * (race on shared state): `fullyParallel:false`, `workers:1`.
 */

// Chromium needs the origin treated as secure for the `Secure` session
// cookie (set by the daemon on login) to be stored and replayed, and
// for the PWAs' service workers to register over plain HTTP.
const INSECURE_ORIGIN_ARGS = [
  // Both daemon origins (shared + the fresh wizard daemon) are listed so
  // the Secure session cookie stores and SWs register on either.
  `--unsafely-treat-insecure-origin-as-secure=${UI_BASE_URL},${UI_FRESH_BASE_URL}`,
  "--allow-insecure-localhost",
];

export default defineConfig({
  testDir: ".",
  fullyParallel: false,
  workers: 1,
  retries: 0,
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
    ignoreHTTPSErrors: true,
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    // Run headed (under xvfb in the runner image). Chromium ignores
    // --unsafely-treat-insecure-origin-as-secure in headless mode
    // (playwright#22944), which silently drops the daemon's `Secure`
    // session cookie over the plain-HTTP origin. Headed honours the
    // flag, so authenticated flows (login, wizard advance) work.
    headless: false,
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
        launchOptions: { args: INSECURE_ORIGIN_ARGS },
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
        launchOptions: { args: INSECURE_ORIGIN_ARGS },
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
        launchOptions: { args: INSECURE_ORIGIN_ARGS },
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
        launchOptions: { args: INSECURE_ORIGIN_ARGS },
      },
    },
  ],
});
