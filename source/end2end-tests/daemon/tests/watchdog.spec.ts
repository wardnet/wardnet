import { beforeAll, describe, expect, it } from "vitest";
import { WardnetClient } from "@wardnet/js";

import { agentGet, agentPost, API_BASE_URL, waitForReady } from "./helpers.js";

// The daemon-side test agent shares the wardnetd container (same PID
// namespace), so it can signal the daemon process directly. Compose DNS
// resolves `wardnetd` to that container.
const TEST_AGENT_URL = "http://wardnetd:3001";

// The unauthenticated health endpoint is served at the host root (not under
// /api). Derive it from API_BASE_URL so a port/host override stays consistent.
const HEALTH_URL = `${API_BASE_URL.replace(/\/api\/?$/, "")}/health`;

interface PidResponse {
  pid: number;
  running: boolean;
}

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
  const res = await agentGet<PidResponse>(TEST_AGENT_URL, "/pid");
  expect(res.running).toBe(true);
  return res.pid;
}

async function getHealth(): Promise<{ status: number; body: HealthBody }> {
  const res = await fetch(HEALTH_URL);
  const body = (await res.json()) as HealthBody;
  return { status: res.status, body };
}

/**
 * Poll `/pid` until the daemon is running under a PID *different* from
 * `previousPid` — i.e. systemd restarted the unit and the new process wrote a
 * fresh pidfile. Returns the new PID.
 */
async function waitForRestart(
  previousPid: number,
  timeoutMs: number,
): Promise<number> {
  const deadline = Date.now() + timeoutMs;
  let last: unknown;
  while (Date.now() < deadline) {
    try {
      const res = await agentGet<PidResponse>(TEST_AGENT_URL, "/pid");
      last = res;
      if (res.running && res.pid !== previousPid) {
        return res.pid;
      }
    } catch (err) {
      // The pidfile vanishes between the old process dying and the new one
      // starting (RuntimeDirectory is torn down and recreated); keep polling.
      last = err;
    }
    await new Promise((resolve) => setTimeout(resolve, 1000));
  }
  throw new Error(
    `daemon did not restart under a new PID within ${timeoutMs}ms; last: ${JSON.stringify(
      last,
    )}`,
  );
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
    // The production daemon registers all four probes.
    expect(names).toEqual(["database", "dhcp", "dns", "liveness"]);
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
      const newPid = await waitForRestart(originalPid, 75_000);
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
