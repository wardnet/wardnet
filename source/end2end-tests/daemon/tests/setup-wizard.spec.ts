import { describe, it, expect, beforeAll } from "vitest";
import {
  AuthService,
  NetworkService,
  SetupService,
  SystemService,
  WardnetClient,
} from "@wardnet/js";

import {
  ADMIN_PASSWORD,
  ADMIN_USERNAME,
  API_BASE_URL,
  AuthedClient,
  waitForReady,
} from "./helpers.js";

/**
 * End-to-end coverage for the first-run setup wizard.
 *
 * This spec runs in the same process as every other spec under a
 * single forked vitest worker (see `vitest.config.ts`), so it's NOT
 * the first thing to touch the daemon — `dhcp-*.spec.ts` files
 * sort earlier and call `ensureAdminAndLogin` (which now drains the
 * wizard to `completed`). Every test in here is therefore written to
 * be valid in *any* wizard state, with the actual primary-path
 * walk runtime-skipped unless the daemon happens to be fresh.
 *
 * The deterministic primary-path coverage is in
 * `auth/tests/wizard.rs` (`advance_wizard_persists_step_and_mode`).
 * What this spec adds at the e2e layer is "the over-the-wire
 * surface still answers the way the daemon-level tests imply it
 * should" — so a future regression in serialization, auth wiring,
 * or routing matches the failure shape we assert here.
 */
describe("setup wizard", () => {
  const client = new WardnetClient({ baseUrl: API_BASE_URL });
  const setup = new SetupService(client);
  let authed: AuthedClient;

  beforeAll(async () => {
    await waitForReady(client);
  }, 120_000);

  it("returns a coherent setup_completed / wizard_step shape", async () => {
    const status = await setup.getStatus();
    expect(status.wizard_step).toBeTruthy();
    // The API derives `setup_completed` from `wizard_step === "completed"`.
    // Pin the invariant so a future refactor that splits them surfaces
    // here.
    expect(status.setup_completed).toBe(status.wizard_step === "completed");
  });

  it("ensures an admin exists and logs in", async () => {
    // Idempotent: if no admin exists yet (fresh daemon, or this spec
    // is running first in process order), create one. Otherwise just
    // log in with the process-scoped credentials shared via
    // `helpers.ts`.
    const status = await setup.getStatus();
    if (status.wizard_step === "admin") {
      await setup.setup({ username: ADMIN_USERNAME, password: ADMIN_PASSWORD });
    }
    const login = await new AuthService(client).login({
      username: ADMIN_USERNAME,
      password: ADMIN_PASSWORD,
    });
    authed = new AuthedClient(API_BASE_URL, login.token);
  });

  it("rejects a second admin creation with 409", async () => {
    // Admin must exist now — either created by the previous test or
    // pre-existing from another spec. POST /api/setup checks
    // `admins.exists()` directly post-fix, so the 409 here is the
    // canonical "setup of step 1 is done" signal.
    let caught: unknown;
    try {
      await setup.setup({ username: "extra", password: "password123" });
    } catch (e) {
      caught = e;
    }
    expect(caught).toBeDefined();
    expect((caught as { status?: number }).status).toBe(409);
  });

  it("walks the primary path through to completed", async (ctx) => {
    // Only meaningful from a fresh "admin" state — when other specs
    // have already drained the wizard, every advance to an earlier
    // step would 400 as a rewind. The deterministic walk is in
    // `wizard.rs::advance_wizard_persists_step_and_mode`; skip cleanly
    // here so this spec stays useful in fresh runs without failing
    // the typical mixed run.
    const start = await setup.getStatus();
    if (start.wizard_step !== "admin" && start.wizard_step !== "network") {
      ctx.skip();
      return;
    }

    const authedSetup = new SetupService(authed);
    const order = ["network", "dhcp", "router_mac", "tunnel", "policy", "completed"] as const;
    let mode: "primary" | "locked_router" | undefined;

    // Walk only from the current step forward — same-step advances
    // are idempotent so we can safely include `network` even when we
    // start there.
    const startIdx = start.wizard_step === "admin" ? 0 : 1;
    for (const step of order.slice(startIdx)) {
      const next = await authedSetup.advance({
        to_step: step,
        wizard_mode: step === "dhcp" ? "primary" : undefined,
      });
      expect(next.wizard_step).toBe(step);
      if (next.wizard_mode) mode = next.wizard_mode;
    }
    expect(mode).toBe("primary");

    const final = await setup.getStatus();
    expect(final.wizard_step).toBe("completed");
    expect(final.setup_completed).toBe(true);
    expect(final.wizard_mode).toBe("primary");
  });

  it("rejects rewinding the wizard with 400", async (ctx) => {
    // Need a wizard that's at least one step past `admin` to have
    // somewhere to rewind from. After the walk above (and after most
    // other spec files run), the wizard is at `completed` — perfect.
    // If we somehow start at `admin`, no rewind target exists.
    const status = await setup.getStatus();
    if (status.wizard_step === "admin") {
      ctx.skip();
      return;
    }
    const authedSetup = new SetupService(authed);
    let caught: unknown;
    try {
      await authedSetup.advance({ to_step: "admin" });
    } catch (e) {
      caught = e;
    }
    expect(caught).toBeDefined();
    expect((caught as { status?: number }).status).toBe(400);
  });

  it("exposes the LAN interface state via GET /api/network/status", async () => {
    const status = await new NetworkService(authed).getStatus();
    expect(status.interface).toBeTruthy();
    expect(status.ip).toMatch(/^\d+\.\d+\.\d+\.\d+$/);
    expect(["static", "dhcp", "unknown"]).toContain(status.dhcp_source);
  });

  it("returns the persisted default policy", async () => {
    const policy = await new SystemService(authed).getDefaultPolicy();
    // Bootstrap migration seeds from config.toml — defaults to "direct".
    expect(policy.policy).toBe("direct");
  });

  it("publishes locked_router as a valid wizard_mode in the OpenAPI schema", async () => {
    // Fetch the live OpenAPI doc from the daemon and assert the public
    // schema still exposes both `wizard_mode` variants. Catches the
    // case where a future commit drops the variant from
    // wardnet_common::api::WizardMode without anyone noticing the
    // public API contract changed.
    const spec = await authed.request<{ components?: { schemas?: Record<string, unknown> } }>(
      "/openapi.json",
    );
    const schemas = spec.components?.schemas ?? {};
    const wizardMode = schemas["WizardMode"] as { enum?: string[] } | undefined;
    expect(wizardMode).toBeTruthy();
    expect(wizardMode?.enum).toEqual(expect.arrayContaining(["primary", "locked_router"]));
  });
});
