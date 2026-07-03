import { defineConfig, devices } from "@playwright/test";

import {
  STORAGE_STATE,
  UI_BASE_URL,
  UI_FRESH_BASE_URL,
  UI_HTTP_BASE_URL,
  UI_LAN_BASE_URL,
} from "./fixtures/seed.js";

// Two runners share one bind-mounted `reports/` dir; the LAN runner sets
// REPORT_SUBDIR=lan so its JUnit/HTML report lands in `reports/lan/` and
// doesn't clobber the mgmt runner's.
const REPORT_DIR = process.env.REPORT_SUBDIR
  ? `reports/${process.env.REPORT_SUBDIR}`
  : "reports";

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
  // Land per-test artifacts (traces, failure screenshots, and — for the
  // visual specs — the `*-actual.png` / `*-diff.png` images) under the
  // bind-mounted `reports/` dir so they upload with the CI artifact. This
  // is how a first, baseline-less CI run surfaces the generated screenshots:
  // harvest the `*-actual.png` files from the artifact as the initial
  // `snapshots/` baselines (README → "Visual regression snapshots"). Default
  // `test-results/` lives only inside the runner container and would be lost.
  outputDir: `${REPORT_DIR}/test-results`,
  fullyParallel: false,
  workers: 1,
  retries: 0,
  // Whole-suite ceiling so a stuck browser launch fails fast with output
  // instead of hanging the CI job (8 short tests finish in minutes).
  globalTimeout: 15 * 60_000,
  // Generous ceilings: compose health waits + first-boot setup push the
  // setup project past Playwright's 30 s default on a cold stack.
  timeout: 60_000,
  expect: {
    timeout: 15_000,
    // Visual-regression defaults (issue #628, V1). Applied to every
    // `toHaveScreenshot` call across the `@visual`-tagged specs:
    //   - `animations:"disabled"` freezes CSS animations/transitions so a
    //     mid-flight frame never diffs (Playwright's screenshot default,
    //     set explicitly for clarity).
    //   - `caret:"hide"` removes the blinking text caret (also the default).
    //   - `maxDiffPixelRatio` is a small antialiasing cushion ONLY — genuine
    //     dynamic content is masked at the call site, not toleranced here.
    // Baselines are generated in the pinned Playwright runner container
    // (deterministic Linux/Chromium rendering); see README → "Visual
    // regression snapshots".
    toHaveScreenshot: {
      animations: "disabled",
      caret: "hide",
      maxDiffPixelRatio: 0.01,
    },
  },
  // Baselines live in a dedicated `snapshots/` dir (bind-mounted into both
  // runners, like `reports/`) instead of Playwright's default co-located
  // `*-snapshots/` folders: the runner image COPIES `tests/` at build time,
  // so a co-located baseline would be baked in and impossible to regenerate.
  // The host bind-mount shadows the image copy, so comparison reads the
  // committed host baselines and `--update`/`updateSnapshots` writes back to
  // the host. `{projectName}` scopes per surface (admin-site / admin-app /
  // user-app) so their identically-named snapshots never collide; the
  // platform suffix is stable because the runner image is a pinned Linux
  // Playwright build.
  snapshotPathTemplate: "snapshots/{projectName}/{arg}{ext}",
  // Comparison-only by default: a missing OR mismatched baseline fails the
  // run (correct for CI). `make e2e-ui-update-snapshots` sets
  // PW_UPDATE_SNAPSHOTS=1 to regenerate every baseline into the bind-mounted
  // `snapshots/` dir. See README → "Visual regression snapshots".
  updateSnapshots: process.env.PW_UPDATE_SNAPSHOTS === "1" ? "all" : "none",
  reporter: [
    ["list"],
    ["junit", { outputFile: `${REPORT_DIR}/junit.xml` }],
    // `open:'never'` — the HTML reporter otherwise spawns a server and
    // hangs the container/CI run on completion.
    ["html", { outputFolder: `${REPORT_DIR}/html`, open: "never" }],
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
    // Discovers the LAN proxy's IP as a device so the user-app's
    // source-IP-classified `devices/me` resolves non-null (C1, #626).
    { name: "seed-lan-device", testMatch: "fixtures/lan-device.setup.ts" },
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
      // Regression guard for the plain-HTTP `:7411` admin surface: same daemon
      // as `admin-site`, but reached DIRECTLY over http:// (no TLS proxy). Proves
      // the session cookie is storable over http:// (not `Secure`) so login works
      // on the pre-provisioning surface. Logs in fresh, so no storageState.
      name: "admin-site-http",
      testMatch: "tests/admin-site-http/**/*.spec.ts",
      dependencies: ["setup"],
      use: {
        ...devices["Desktop Chrome"],
        baseURL: `${UI_HTTP_BASE_URL}/admin/`,
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
      // Device-keyed, no login (C1, #626). Driven by `ui_runner_lan`
      // through the LAN-side TLS proxy, whose IP `seed-lan-device` seeds
      // as a discovered device — so `devices/me` (classified by source
      // IP) resolves non-null and the app renders the device. The
      // null/no-device case is exercised within identity.spec by
      // navigating to the mgmt proxy origin (UI_BASE_URL), whose IP is
      // not a device.
      name: "user-app",
      testMatch: "tests/user-app/**/*.spec.ts",
      dependencies: ["seed-lan-device"],
      use: {
        ...devices["Pixel 7"],
        baseURL: `${UI_LAN_BASE_URL}/`,
        launchOptions: { args: CHROMIUM_ARGS },
      },
    },
  ],
});
