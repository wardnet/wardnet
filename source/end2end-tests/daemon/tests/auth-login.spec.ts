import { describe, it, expect, beforeAll } from "vitest";
import { AuthService, SetupService, WardnetClient } from "@wardnet/js";

import {
  ADMIN_PASSWORD,
  ADMIN_USERNAME,
  API_BASE_URL,
  ensureAdminAndLogin,
  waitForReady,
} from "./helpers.js";

/**
 * Auth-failure paths, over the wire.
 *
 * The happy path (login → bearer → authed call) is exercised all over
 * the suite via `ensureAdminAndLogin`. What's missing is the negative
 * space: a bad password, a call carrying a token that doesn't name a
 * session, and the one-shot nature of setup. Those rejections are
 * load-bearing — they're the difference between "the admin API is locked
 * down" and "it looks locked down until someone gets the header wrong" —
 * so pin them here rather than trusting the unit tests in `auth/tests/`
 * to stand in for the assembled HTTP surface.
 *
 * The missing-credentials case (no header at all) is owned by
 * `auth-coverage.spec.ts`, which hits every representative admin surface
 * with no auth; the bad-bearer case below is the complement it doesn't
 * cover — credentials present but invalid.
 *
 * `beforeAll` runs the idempotent bootstrap so an admin definitely
 * exists; that makes the wrong-password case a genuine "existing admin,
 * wrong secret" (401 from credential check) rather than a "no admin at
 * all" accident that could pass for the wrong reason.
 */
describe("auth — login and rejection paths", () => {
  const client = new WardnetClient({ baseUrl: API_BASE_URL });
  const auth = new AuthService(client);

  beforeAll(async () => {
    await waitForReady(client);
    await ensureAdminAndLogin(client);
  }, 180_000);

  it("rejects a wrong password with 401", async () => {
    await expect(
      auth.login({
        username: ADMIN_USERNAME,
        password: `${ADMIN_PASSWORD}-not-it`,
      }),
    ).rejects.toMatchObject({ name: "WardnetApiError", status: 401 });
  });

  it("rejects an admin endpoint called with a bad bearer with 401", async () => {
    // A malformed/expired token reaches `validate_session`/`validate_api_key`
    // and finds nothing, so it must dead-end on the same 401 as no token at
    // all — a caller can't be allowed to probe for which tokens exist.
    // `/devices` stands in for "any admin-guarded read".
    const res = await fetch(`${API_BASE_URL}/devices`, {
      headers: { Authorization: "Bearer not-a-real-token" },
    });
    expect(res.status).toBe(401);
  });

  it("reports setup as already completed once an admin exists", async () => {
    // Setup is one-shot: `beforeAll` guaranteed an admin, so a second
    // POST /api/setup must be refused. The daemon guards on
    // `admins.exists()` and answers 409 with a "setup already completed"
    // detail — assert both so a future refactor that swaps the status or
    // drops the message trips here.
    const setup = new SetupService(client);
    await expect(
      setup.setup({ username: "intruder", password: "password123" }),
    ).rejects.toMatchObject({
      status: 409,
      body: { detail: expect.stringContaining("already completed") },
    });
  });
});
