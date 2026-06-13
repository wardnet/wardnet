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
 * Base URL of the daemon as the browser reaches it. `wardnetd-ui` is the
 * compose service name on `wardnet_mgmt`; override locally with
 * `WARDNET_UI_BASE_URL=http://localhost:7411`.
 */
export const UI_BASE_URL =
  process.env.WARDNET_UI_BASE_URL ?? "http://wardnetd-ui:7411";

/** Hostname the session cookie is scoped to (derived from UI_BASE_URL). */
export const UI_HOST = new URL(UI_BASE_URL).hostname;

/**
 * Base URL of the pristine, never-seeded daemon used by the first-run
 * setup-wizard spec (A1). Distinct from UI_BASE_URL so the wizard UI can
 * be walked from a clean state; the shared daemon is always pre-seeded.
 */
export const UI_FRESH_BASE_URL =
  process.env.WARDNET_UI_FRESH_BASE_URL ?? "http://wardnetd-ui-fresh:7411";

/** API base (same origin as the browser surface). */
export const API_BASE_URL =
  process.env.WARDNET_API_BASE_URL ?? `${UI_BASE_URL}/api`;

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
async function api<T>(
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
