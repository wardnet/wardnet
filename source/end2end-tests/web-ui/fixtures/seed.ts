// biome-ignore lint/correctness/noNodejsModules: test harness, run by Node — never shipped to a browser
import { createHash } from "node:crypto";
// biome-ignore lint/correctness/noNodejsModules: test harness, run by Node — never shipped to a browser
import { mkdirSync, rmSync } from "node:fs";

/**
 * Daemon seeding for the Playwright harness, over plain `fetch` against
 * the REST API.
 *
 * Why not the typed `@wardnet/js` SDK (as the daemon Vitest suite does):
 * the SDK ships as source-only TypeScript (`exports` → `src/index.ts`,
 * depends on `consola`). Vitest transpiles it transparently, but
 * Playwright's loader can't strip a portal-symlinked dependency's TS,
 * and the symlink realpath breaks `consola` resolution. Seeding touches
 * only a handful of endpoints, so typed `fetch` is the simpler, robust
 * choice here. If a later stage needs the full SDK, revisit with a
 * transform that covers the portal'd source.
 */

/**
 * Base URL of the daemon as the BROWSER reaches it — over the Caddy TLS
 * proxy (`tls_proxy`), not the daemon's plain-HTTP port. Real HTTPS is
 * required so the daemon's `Secure` session cookie is stored and PWA
 * service workers register; the daemon's own :443 is ACME-gated (503) so
 * a self-signed proxy terminates TLS in front of :7411. Override locally
 * with `WARDNET_UI_BASE_URL=https://localhost:8443`.
 */
export const UI_BASE_URL =
  // biome-ignore lint/correctness/noProcessGlobal: test harness, run by Node — never shipped to a browser
  process.env.WARDNET_UI_BASE_URL ?? "https://wardnetd-ui-tls";

/** Hostname the session cookie is scoped to (derived from UI_BASE_URL). */
export const UI_HOST = new URL(UI_BASE_URL).hostname;

/**
 * Browser base URL of the pristine, never-seeded daemon used by the
 * first-run setup-wizard spec (A1), via its own TLS proxy vhost.
 */
export const UI_FRESH_BASE_URL =
  // biome-ignore lint/correctness/noProcessGlobal: test harness, run by Node — never shipped to a browser
  process.env.WARDNET_UI_FRESH_BASE_URL ?? "https://wardnetd-ui-fresh-tls";

/**
 * Browser base URL of the LAN-side TLS proxy (`tls_proxy_lan`) used by the
 * device-keyed user-app (C1, #626). The proxy lives on wardnet_lan and
 * forwards to the daemon's LAN IP, so the daemon classifies requests from
 * this origin as coming from `LAN_PROXY_IP` — seeded as a discovered
 * device so `GET /api/devices/me` resolves non-null.
 */
export const UI_LAN_BASE_URL =
  // biome-ignore lint/correctness/noProcessGlobal: test harness, run by Node — never shipped to a browser
  process.env.WARDNET_UI_LAN_BASE_URL ?? "https://wardnetd-ui-lan-tls";

/**
 * The LAN-side proxy's fixed wardnet_lan IP (compose `tls_proxy_lan`).
 * This is the source IP the daemon sees for user-app requests over the
 * LAN proxy, and the IP the harness seeds as a discovered device. Outside
 * the wardnet_lan IPAM `ip_range` (10.91.0.0/28) and inside the /24.
 */
export const LAN_PROXY_IP = "10.91.0.20";

/**
 * API base for Node-side seeding (global.setup). Hits the daemon's
 * plain-HTTP port DIRECTLY (not through the proxy): seeding reads the
 * login token from the response body, never relies on the cookie, and
 * avoids Node self-signed-TLS handling.
 */
export const API_BASE_URL =
  // biome-ignore lint/correctness/noProcessGlobal: test harness, run by Node — never shipped to a browser
  process.env.WARDNET_API_BASE_URL ?? "http://wardnetd-ui:7411/api";

/**
 * Browser base URL of the daemon's plain-HTTP `:7411` admin surface, reached
 * DIRECTLY (no TLS proxy). This is the pre-provisioning admin endpoint most
 * instances actually use — HTTPS only exists once a cert is issued via the
 * DDNS / BYO-domain flow. Used by the `admin-site-http` project to regression-
 * guard that the session cookie is storable by a browser over `http://` (i.e.
 * NOT marked `Secure`); the rest of the suite runs over the HTTPS proxy, where a
 * `Secure` cookie would be stored regardless and hide that bug.
 */
export const UI_HTTP_BASE_URL =
  // biome-ignore lint/correctness/noProcessGlobal: test harness, run by Node — never shipped to a browser
  process.env.WARDNET_UI_HTTP_BASE_URL ?? "http://wardnetd-ui:7411";

/** Where the `setup` project writes the admin session for authed surfaces. */
export const STORAGE_STATE = ".auth/admin.json";

// Setup-wizard credentials. Derived deterministically (not randomBytes)
// so every Playwright worker process computes the SAME value: the
// `setup` project creates the admin in one worker and `login.spec`
// authenticates with it in another — a per-process random password
// would mismatch. A hashed constant (vs a plaintext literal) keeps
// secret scanners quiet; it's test-only and never leaves the throwaway
// compose stack.
export const ADMIN_USERNAME = "admin";
export const ADMIN_PASSWORD = `e2e-${createHash("sha256")
  .update("wardnet-web-ui-e2e-admin")
  .digest("hex")
  .slice(0, 16)}`;

// Wizard steps in order (serde snake_case of WizardStep in
// wardnet-common/src/api.rs). Walked one-by-one so every transition
// moves strictly forward.
const WIZARD_ORDER = [
  "admin",
  "network",
  "dhcp",
  "router_mac",
  "dns",
  "tunnel",
  "policy",
  "remote_access",
  "review",
  "completed",
] as const;
type WizardStep = (typeof WIZARD_ORDER)[number];

interface SetupStatus {
  setup_completed: boolean;
  wizard_step: WizardStep;
  wizard_mode?: string | null;
}

/** JSON request against the daemon API. Throws on non-2xx. */
export async function api<T>(
  path: string,
  init?: RequestInit & { token?: string },
): Promise<T> {
  const headers = new Headers(init?.headers);
  headers.set("Content-Type", "application/json");
  if (init?.token) headers.set("Authorization", `Bearer ${init.token}`);
  const res = await fetch(`${API_BASE_URL}${path}`, { ...init, headers });
  if (!res.ok) {
    throw new Error(
      `${init?.method ?? "GET"} ${path} → ${res.status}: ${await res.text()}`,
    );
  }
  // Some endpoints (204) have no body; guard the parse.
  const text = await res.text();
  return (text ? JSON.parse(text) : undefined) as T;
}

/** Polls `/api/info` until the daemon responds, or throws. */
export async function waitForReady(timeoutMs = 120_000): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  let lastError: unknown;
  while (Date.now() < deadline) {
    try {
      await api("/info");
      return;
    } catch (err) {
      lastError = err;
      await new Promise((resolve) => setTimeout(resolve, 1_000));
    }
  }
  throw new Error(
    `daemon did not become ready within ${timeoutMs}ms: ${String(lastError)}`,
  );
}

/** Walk the wizard to `completed`, one forward step at a time. No-op if done. */
async function drainWizard(token: string): Promise<void> {
  for (let safety = 0; safety <= WIZARD_ORDER.length; safety += 1) {
    const status = await api<SetupStatus>("/setup/status");
    if (status.wizard_step === "completed") return;
    const idx = WIZARD_ORDER.indexOf(status.wizard_step);
    const next = WIZARD_ORDER[idx + 1] ?? "completed";
    await api("/setup/advance", {
      method: "POST",
      token,
      // wizard_mode is recorded at the dhcp step (step 3).
      body: JSON.stringify({
        to_step: next,
        ...(status.wizard_step === "dhcp" ? { wizard_mode: "primary" } : {}),
      }),
    });
  }
  throw new Error("drainWizard: exceeded maximum step transitions");
}

interface DdnsStatus {
  provider: string | null;
}

/**
 * Directory whose mere existence is the lock: `mkdirSync` is atomic even
 * across the two runner containers, because `reports/` is bind-mounted from
 * the SAME host directory into both `ui_runner` and `ui_runner_lan`
 * (compose.ui.yaml) — so this is a real cross-container mutex, not just an
 * in-process one.
 */
const ENROLLMENT_LOCK_DIR = "reports/.premium-enrollment.lock";

/** Poll interval while waiting for the other container to release the lock. */
const LOCK_POLL_MS = 250;
const LOCK_TIMEOUT_MS = 60_000;

/**
 * Run `fn` under a cross-container mutex (see `ENROLLMENT_LOCK_DIR`).
 * `ensurePremiumEnrollment` re-checks `/ddns/status` as the first thing it
 * does once inside the lock, so this is a standard check-lock-check: the
 * loser of the race sees the winner's completed enrollment and no-ops.
 */
async function withEnrollmentLock<T>(fn: () => Promise<T>): Promise<T> {
  // Ensure the parent exists first (idempotent, safe under concurrent
  // callers) — `mkdirSync(..., {recursive: true})` on the lock dir ITSELF
  // would silently succeed even when already held, defeating the lock, so
  // the actual lock acquisition below must stay non-recursive.
  mkdirSync("reports", { recursive: true });

  const deadline = Date.now() + LOCK_TIMEOUT_MS;
  for (;;) {
    try {
      mkdirSync(ENROLLMENT_LOCK_DIR);
      break;
    } catch (err) {
      if ((err as NodeJS.ErrnoException).code !== "EEXIST") throw err;
      if (Date.now() > deadline) {
        throw new Error(
          `timed out waiting for ${ENROLLMENT_LOCK_DIR} (held by the other runner container?)`,
        );
      }
      await new Promise((resolve) => setTimeout(resolve, LOCK_POLL_MS));
    }
  }
  try {
    return await fn();
  } finally {
    rmSync(ENROLLMENT_LOCK_DIR, { recursive: true, force: true });
  }
}

/**
 * Idempotent wardnet-cloud enrollment: walk the real enroll → register flow
 * (against `wardnet_cloud_mock`, see compose.ui.yaml's `[ddns_wardnet]`
 * overrides) so this box's `Entitlement::is_entitled()` flips true. Since
 * 366f942 ("premium-gate the mobile PWAs"), admin-app and user-app are
 * served the "premium required" page instead of the real app unless the
 * daemon is on the wardnet DDNS provider.
 *
 * Called from the `premium-setup` project only (see playwright.config.ts),
 * which both `admin-app` and `user-app` depend on — deliberately NOT folded
 * into `ensureAdminSetup()`, so `admin-site`/`admin-site-http` (never
 * premium-gated) don't pay for it. `premium-setup` still runs once per
 * runner container (`ui_runner` for admin-app, `ui_runner_lan` for
 * user-app), both against the one shared daemon, so the enroll/register
 * critical section runs under `withEnrollmentLock` — without it, two
 * concurrent `enroll()` calls would each mint and persist a fresh identity
 * (`ddns/mod.rs`), and whichever `register_network()` ran second could sign
 * with the other's identity, corrupting the daemon's DDNS/entitlement state.
 */
export async function ensurePremiumEnrollment(token: string): Promise<void> {
  await withEnrollmentLock(async () => {
    const status = await api<DdnsStatus>("/ddns/status", { token });
    if (status.provider === "wardnet") return;

    await api("/ddns/enrollment-code", {
      method: "POST",
      token,
      body: JSON.stringify({ email: "e2e@wardnet.test" }),
    });
    await api("/ddns/enroll", {
      method: "POST",
      token,
      // The mock doesn't validate the code — any value enrolls.
      body: JSON.stringify({ code: "000000" }),
    });
    await api("/ddns/register", {
      method: "POST",
      token,
      body: JSON.stringify({ slug: "e2e-premium", display_name: "E2E premium" }),
    });
  });
}

/**
 * Idempotent admin bootstrap: create the admin if the wizard is fresh,
 * log in, drain the wizard to `completed`, and return the session token.
 * The token is the value of the daemon's `wardnet_session` cookie
 * (crates/wardnetd-api/src/api/auth.rs), so the caller can build a browser
 * storageState directly from it. Does NOT enroll into the premium wardnet
 * DDNS provider — see `ensurePremiumEnrollment`, called separately by the
 * `premium-setup` project for the surfaces that actually need it.
 */
export async function ensureAdminSetup(): Promise<string> {
  const status = await api<SetupStatus>("/setup/status");
  if (status.wizard_step === "admin") {
    await api("/setup", {
      method: "POST",
      body: JSON.stringify({
        username: ADMIN_USERNAME,
        password: ADMIN_PASSWORD,
      }),
    });
  }
  const login = await api<{ token: string }>("/auth/login", {
    method: "POST",
    body: JSON.stringify({
      username: ADMIN_USERNAME,
      password: ADMIN_PASSWORD,
    }),
  });
  await drainWizard(login.token);
  return login.token;
}
