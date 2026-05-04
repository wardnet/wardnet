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

  it("runs cleanly at boot and writes an empty state.json", async () => {
    const state = await agentGet<PostupgradeState>(
      TEST_AGENT_URL,
      "/postupgrade/state",
    );

    // Empty migration list ships with this PR — no entries should
    // have been applied or failed. last_verification_failure must
    // not be set (the runner only writes that field when the
    // signature on the in-image payload doesn't verify).
    expect(state.applied).toEqual([]);
    expect(state.failed).toEqual([]);
    expect(state.last_verification_failure ?? null).toBeNull();
  });
});
