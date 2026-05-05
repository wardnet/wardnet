import { describe, it, expect, beforeAll } from "vitest";
import { SystemService, WardnetApiError, WardnetClient } from "@wardnet/js";

import {
  API_BASE_URL,
  AuthedClient,
  agentGet,
  ensureAdminAndLogin,
  waitForReady,
} from "./helpers.js";

// Test agent runs in the wardnetd container alongside the daemon and
// exposes `:3001`. Compose DNS resolves `wardnetd` to that container
// on the wardnet_mgmt bridge.
const TEST_AGENT_URL = "http://wardnetd:3001";

interface FileContentsResponse {
  path: string;
  exists: boolean;
  contents: string | null;
}

interface PostupgradeState {
  applied: Array<{ id: string; applied_at: string }>;
  failed: Array<{ id: string; error: string; at: string }>;
}

interface PkcheckResponse {
  action_id: string;
  exit_code: number;
  stdout: string;
  stderr: string;
}

/**
 * Poll `/power/systemctl-invocations` until the marker file exists
 * and contains a line matching `expectedArgs` (e.g. `"reboot --no-block"`).
 *
 * We poll because the daemon's `request_reboot` / `request_shutdown`
 * spawns a 500 ms-grace task; the 204 returns before the spawned
 * task fires `systemctl`. ~3 s is plenty.
 */
async function waitForSystemctlInvocation(
  expectedArgs: string,
  timeoutMs = 5_000,
): Promise<string> {
  const deadline = Date.now() + timeoutMs;
  let last: FileContentsResponse | undefined;
  while (Date.now() < deadline) {
    last = await agentGet<FileContentsResponse>(
      TEST_AGENT_URL,
      "/power/systemctl-invocations",
    );
    if (last.exists && last.contents?.includes(expectedArgs)) {
      return last.contents;
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(
    `systemctl was not invoked with ${JSON.stringify(expectedArgs)} within ${timeoutMs}ms; ` +
      `last response: ${JSON.stringify(last)}`,
  );
}

describe("Safe Reboot / Safe Shutdown", () => {
  const client = new WardnetClient({ baseUrl: API_BASE_URL });
  let admin: AuthedClient;

  beforeAll(async () => {
    await waitForReady(client);
    admin = await ensureAdminAndLogin(client);
  }, 120_000);

  it("applies the 0001_polkit_power_rule migration on first boot", async () => {
    const state = await agentGet<PostupgradeState>(
      TEST_AGENT_URL,
      "/postupgrade/state",
    );
    const ids = state.applied.map((e) => e.id);
    expect(ids).toContain("0001_polkit_power_rule");
    expect(state.failed).toEqual([]);
  });

  it("writes the polkit rule to the expected path with the expected content", async () => {
    const file = await agentGet<FileContentsResponse>(
      TEST_AGENT_URL,
      "/power/polkit-rule",
    );
    expect(file.exists).toBe(true);
    expect(file.contents).toContain('subject.user !== "wardnet"');
    expect(file.contents).toContain("org.freedesktop.login1.reboot");
    expect(file.contents).toContain(
      "org.freedesktop.login1.reboot-multiple-sessions",
    );
    expect(file.contents).toContain("org.freedesktop.login1.power-off");
    expect(file.contents).toContain(
      "org.freedesktop.login1.power-off-multiple-sessions",
    );
    expect(file.contents).toContain("polkit.Result.YES");
  });

  it.each([
    "org.freedesktop.login1.reboot",
    "org.freedesktop.login1.reboot-multiple-sessions",
    "org.freedesktop.login1.power-off",
    "org.freedesktop.login1.power-off-multiple-sessions",
  ])("polkit grants %s to the wardnet user", async (actionId) => {
    const res = await agentGet<PkcheckResponse>(
      TEST_AGENT_URL,
      `/power/pkcheck?action_id=${encodeURIComponent(actionId)}`,
    );
    expect(res).toMatchObject({
      action_id: actionId,
      exit_code: 0,
    });
  });

  it("POST /api/system/reboot returns 204 and invokes systemctl reboot --no-block", async () => {
    // Endpoint contract: 204 on accept.
    const res = await fetch(`${API_BASE_URL}/system/reboot`, {
      method: "POST",
      headers: { Authorization: `Bearer ${tokenOf(admin)}` },
    });
    expect(res.status).toBe(204);

    // The spawned task fires after a ~500 ms grace; assert the shim
    // captured it.
    const log = await waitForSystemctlInvocation("reboot --no-block");
    expect(log).toContain("reboot --no-block");
  });

  it("POST /api/system/shutdown returns 204 and invokes systemctl poweroff --no-block", async () => {
    const res = await fetch(`${API_BASE_URL}/system/shutdown`, {
      method: "POST",
      headers: { Authorization: `Bearer ${tokenOf(admin)}` },
    });
    expect(res.status).toBe(204);

    const log = await waitForSystemctlInvocation("poweroff --no-block");
    expect(log).toContain("poweroff --no-block");
  });

  it("rejects unauthenticated POST /api/system/reboot with 401", async () => {
    const res = await fetch(`${API_BASE_URL}/system/reboot`, {
      method: "POST",
    });
    expect(res.status).toBe(401);
  });

  it("rejects unauthenticated POST /api/system/shutdown with 401", async () => {
    const res = await fetch(`${API_BASE_URL}/system/shutdown`, {
      method: "POST",
    });
    expect(res.status).toBe(401);
  });

  it("SDK SystemService.reboot/shutdown succeeds against the live API", async () => {
    // Exercise the SDK code path rather than raw fetch — guards
    // against SDK-level regressions in the `postNoContent` helper.
    const system = new SystemService(admin);
    await system.reboot();
    await system.shutdown();
    // No throws ⇒ both returned 204.
  });

  it("surfaces a structured WardnetApiError for unauthenticated SDK calls", async () => {
    const anon = new SystemService(new WardnetClient({ baseUrl: API_BASE_URL }));
    await expect(anon.reboot()).rejects.toBeInstanceOf(WardnetApiError);
  });
});

/**
 * Pull the bearer token off an [`AuthedClient`]. The class doesn't
 * expose it publicly — the e2e helper is the only place that
 * constructs one — but the test occasionally needs it for raw
 * `fetch` calls that bypass `WardnetClient`. Cast keeps the
 * intrusion local.
 */
function tokenOf(authed: AuthedClient): string {
  return (authed as unknown as { token: string }).token;
}
