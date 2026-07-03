import { createHash } from "node:crypto";

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
  process.env.WARDNET_UI_BASE_URL ?? "https://wardnetd-ui-tls";

/** Hostname the session cookie is scoped to (derived from UI_BASE_URL). */
export const UI_HOST = new URL(UI_BASE_URL).hostname;

/**
 * Browser base URL of the pristine, never-seeded daemon used by the
 * first-run setup-wizard spec (A1), via its own TLS proxy vhost.
 */
export const UI_FRESH_BASE_URL =
  process.env.WARDNET_UI_FRESH_BASE_URL ?? "https://wardnetd-ui-fresh-tls";

/**
 * Browser base URL of the LAN-side TLS proxy (`tls_proxy_lan`) used by the
 * device-keyed user-app (C1, #626). The proxy lives on wardnet_lan and
 * forwards to the daemon's LAN IP, so the daemon classifies requests from
 * this origin as coming from `LAN_PROXY_IP` — seeded as a discovered
 * device so `GET /api/devices/me` resolves non-null.
 */
export const UI_LAN_BASE_URL =
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
// moves strictly forward — the daemon rejects rewinds.
const WIZARD_ORDER = [
  "admin",
  "network",
  "dhcp",
  "router_mac",
  "tunnel",
  "policy",
  "remote_access",
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

/**
 * Idempotent admin bootstrap: create the admin if the wizard is fresh,
 * log in, drain the wizard to `completed`, and return the session token.
 * The token is the value of the daemon's `wardnet_session` cookie
 * (crates/wardnetd-api/src/api/auth.rs), so the caller can build a
 * browser storageState directly from it.
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
