import { beforeAll, describe, expect, it } from "vitest";
import { WardnetClient } from "@wardnet/js";

import {
  agentPost,
  API_BASE_URL,
  daemonPid,
  waitForDaemonRestart,
  waitForReady,
} from "./helpers.js";

// The daemon-side test agent shares the wardnetd container (same PID
// namespace), so it can signal the daemon process directly. Compose DNS
// resolves `wardnetd` to that container.
const TEST_AGENT_URL = "http://wardnetd:3001";

// The unauthenticated health endpoint is served at the host root (not under
// /api). Derive it from API_BASE_URL so a port/host override stays consistent.
const HEALTH_URL = `${API_BASE_URL.replace(/\/api\/?$/, "")}/health`;

interface HealthComponent {
  name: string;
  status: "UP" | "DOWN";
  detail?: string;
}

interface HealthBody {
  status: "UP" | "DOWN";
  components: HealthComponent[];
}

async function getPid(): Promise<number> {
  const res = await daemonPid(TEST_AGENT_URL);
  expect(res.running).toBe(true);
  return res.pid;
}

async function getHealth(): Promise<{ status: number; body: HealthBody }> {
  const res = await fetch(HEALTH_URL);
  const body = (await res.json()) as HealthBody;
  return { status: res.status, body };
}

describe("Hardware/soft watchdog (issue #214)", () => {
  const client = new WardnetClient({ baseUrl: API_BASE_URL });

  beforeAll(async () => {
    await waitForReady(client);
  }, 120_000);

  it("serves an unauthenticated /health with 200 UP and the expected components", async () => {
    const { status, body } = await getHealth();
    expect(status).toBe(200);
    expect(body.status).toBe("UP");
    const names = body.components.map((c) => c.name).sort();
    // The production daemon registers all five probes. `dot` (the Private
    // DNS :853 listener, #912) is UP here because the feature is disabled by
    // default — the probe is desired-vs-actual, so a toggled-off listener is
    // healthy, not a failure.
    expect(names).toEqual(["database", "dhcp", "dns", "dot", "liveness"]);
    expect(body.components.every((c) => c.status === "UP")).toBe(true);
  });

  it(
    "systemd restarts the daemon after a freeze (SIGSTOP) via WatchdogSec",
    async () => {
      // A frozen process never *exits*, so Restart=always cannot recover it —
      // only the Type=notify + WatchdogSec=15 supervision can. Observing a
      // restart after SIGSTOP therefore proves the soft-watchdog transport
      // (READY=1 brought the unit to active(running); withheld WATCHDOG=1
      // pings tripped the timeout).
      const originalPid = await getPid();

      const frozen = await agentPost<{ pid: number; delivered: boolean }>(
        TEST_AGENT_URL,
        "/process/signal",
        { signal: "STOP" },
      );
      expect(frozen.delivered).toBe(true);
      expect(frozen.pid).toBe(originalPid);

      // WatchdogSec=15 + kill escalation + RestartSec + startup. Generous
      // ceiling to stay robust on a slow runner.
      const newPid = await waitForDaemonRestart(
        TEST_AGENT_URL,
        originalPid,
        75_000,
      );
      expect(newPid).not.toBe(originalPid);

      // The restarted daemon must come back fully healthy so subsequent specs
      // (and the compose healthcheck) see a working instance.
      await waitForReady(client, 60_000);
      const { status, body } = await getHealth();
      expect(status).toBe(200);
      expect(body.status).toBe("UP");
    },
    120_000,
  );
});
