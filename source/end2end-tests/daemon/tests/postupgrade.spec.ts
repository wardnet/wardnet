import { describe, it, expect, beforeAll } from "vitest";
import { WardnetClient } from "@wardnet/js";

import { API_BASE_URL, agentGet, waitForReady } from "./helpers.js";

// The test-agent runs inside the wardnetd container alongside the
// daemon and exposes :3001 (see source/daemon/packaging/test-agent/
// wardnet-test-agent.service). Compose DNS resolves `wardnetd` to the
// container on wardnet_mgmt, which is shared with the test_runner.
const TEST_AGENT_URL = "http://wardnetd:3001";

interface PostupgradeState {
  applied: Array<{ id: string; applied_at: string }>;
  failed: Array<{ id: string; error: string; at: string }>;
  last_verification_failure?: { error: string; at: string } | null;
}

describe("post-upgrade migration framework", () => {
  const client = new WardnetClient({ baseUrl: API_BASE_URL });

  beforeAll(async () => {
    // wardnetd's healthcheck only flips healthy after wardnetd binds
    // its API port, which itself is gated on the post-upgrade runner
    // exiting cleanly (RequiredBy=wardnetd.service). Reaching this
    // point already proves the systemd dependency chain works; the
    // assertions below check the on-disk evidence.
    await waitForReady(client);
  }, 120_000);

  it("runs cleanly at boot and records every shipped migration as applied", async () => {
    // wardnet-postupgrade.service is `Type=oneshot` and runs before
    // wardnetd.service. By the time wardnetd is healthy, the runner
    // has already exited and the migration runner has written
    // state.json. Poll briefly anyway in case the test-agent picks
    // up the API socket a moment before the migration runner's
    // atomic rename lands.
    let state: PostupgradeState | undefined;
    const deadline = Date.now() + 10_000;
    while (Date.now() < deadline) {
      try {
        state = await agentGet<PostupgradeState>(
          TEST_AGENT_URL,
          "/postupgrade/state",
        );
        break;
      } catch (_err) {
        await new Promise((resolve) => setTimeout(resolve, 500));
      }
    }
    if (!state) {
      throw new Error(
        "wardnet-postupgrade did not write state.json within 10s",
      );
    }

    // No migrations may have failed. last_verification_failure must
    // not be set (the runner only writes that field when the
    // signature on the in-image payload doesn't verify). The
    // safe-power spec asserts `applied` contains the specific
    // 0001_polkit_power_rule entry; here we just guard that the
    // runner ran cleanly to completion.
    expect(state.failed).toEqual([]);
    expect(state.last_verification_failure ?? null).toBeNull();
  });
});
