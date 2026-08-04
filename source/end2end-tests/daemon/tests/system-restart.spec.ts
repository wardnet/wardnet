import { beforeAll, describe, expect, it } from "vitest";
import { InfoService, SystemService, WardnetApiError, WardnetClient } from "@wardnet/js";

import {
  API_BASE_URL,
  AuthedClient,
  DAEMON_AGENT,
  daemonPid,
  ensureAdminAndLogin,
  waitForDaemonRestart,
  waitForReady,
} from "./helpers.js";

/**
 * `POST /api/system/restart` end-to-end (issue #249, E2E Stage 11).
 *
 * Order-sensitive by construction: it takes the daemon down mid-suite. It
 * lives in its own file so vitest's `fileParallelism: false` ordering keeps it
 * atomic, and the last assertion in the file is the recovery one — the daemon
 * is verified healthy again *before* control returns to the runner, so the
 * next spec file's `waitForReady` has nothing left to wait for. That is
 * deliberately stronger than a global setup hook: a failure to come back
 * surfaces here, attributed to the restart, instead of as a mystery timeout in
 * whatever file happened to run next.
 *
 * The daemon exits with code 0 after a ~500 ms grace; `Restart=always` +
 * `RestartSec=2s` in `wardnetd.service` bring it back. Two restarts in one
 * suite run (this one and `backup-roundtrip.spec.ts`) stay well inside the
 * `StartLimitBurst=5` / `StartLimitIntervalSec=30` budget, so the
 * `wardnetd-rollback.service` `OnFailure=` hook never arms.
 */
describe("system restart (issue #249)", () => {
  const client = new WardnetClient({ baseUrl: API_BASE_URL });
  let admin: AuthedClient;
  let system: SystemService;

  beforeAll(async () => {
    await waitForReady(client);
    admin = await ensureAdminAndLogin(client);
    system = new SystemService(admin);
  }, 120_000);

  it("rejects an unauthenticated restart request with 401", async () => {
    const anon = new SystemService(new WardnetClient({ baseUrl: API_BASE_URL }));
    const err = await anon.restart().catch((e: unknown) => e);
    expect(err).toBeInstanceOf(WardnetApiError);
    expect((err as WardnetApiError).status).toBe(401);

    // And the daemon is still up — a rejected request must not have scheduled
    // anything. This assertion is why the negative case runs first.
    const { running } = await daemonPid(DAEMON_AGENT);
    expect(running).toBe(true);
  });

  it(
    "restarts the daemon under a new PID and recovers /api/info",
    async () => {
      const before = await daemonPid(DAEMON_AGENT);
      expect(before.running).toBe(true);

      const uptimeBefore = (await new InfoService(client).getInfo())
        .uptime_seconds;

      // Returns 204 as soon as the restart is *scheduled* — the process is
      // still alive at this point, which is why the PID poll below is the
      // real signal.
      await system.restart();

      // ~500 ms grace + graceful shutdown of every runner + RestartSec=2s +
      // startup. A generous ceiling keeps a slow CI runner from reading as a
      // failure to restart.
      const newPid = await waitForDaemonRestart(DAEMON_AGENT, before.pid, 90_000);
      expect(newPid).not.toBe(before.pid);

      // The API must come back on its own — no manual intervention, no
      // compose restart.
      await waitForReady(client, 90_000);
      const info = await new InfoService(client).getInfo();

      // A fresh process restarts the uptime clock. Guarding on "lower than
      // before" rather than "near zero" avoids racing a slow poll loop that
      // only notices the new process seconds later.
      expect(info.uptime_seconds).toBeLessThan(uptimeBefore);
    },
    180_000,
  );

  it("serves authenticated admin requests again after the restart", async () => {
    // Sessions live in the database, so the token minted before the restart
    // survives it. Re-using `admin` rather than logging in again is the
    // assertion: a restart that dropped session state would 401 here.
    const status = await system.getStatus();
    expect(status.release_version).not.toBe("");

    // The daemon records its own exit, so the restart it just performed is
    // visible as a graceful shutdown rather than a crash.
    expect(status.last_shutdown.state).toBe("graceful");
    expect(status.last_shutdown.at).not.toBeNull();
  });
});
