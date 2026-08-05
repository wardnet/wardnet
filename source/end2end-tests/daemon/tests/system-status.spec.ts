import { beforeAll, describe, expect, it } from "vitest";
import { InfoService, SystemService, WardnetApiError, WardnetClient } from "@wardnet/js";

import {
  API_BASE_URL,
  AuthedClient,
  ensureAdminAndLogin,
  waitForReady,
} from "./helpers.js";

/**
 * `GET /api/system/status` against the live daemon (issue #249, E2E Stage 11).
 *
 * Read-only throughout — no daemon state is mutated, so this file is safe in
 * any position of the suite. It asserts the *shape and internal consistency*
 * of the snapshot rather than exact values: uptime, CPU, and memory move
 * between reads, and the device count depends on which DHCP specs have run
 * before this one under vitest's shared-stack model.
 */
describe("system status (issue #249)", () => {
  const client = new WardnetClient({ baseUrl: API_BASE_URL });
  let admin: AuthedClient;
  let system: SystemService;

  beforeAll(async () => {
    await waitForReady(client);
    admin = await ensureAdminAndLogin(client);
    system = new SystemService(admin);
  }, 120_000);

  it("reports the daemon's versions, and they match /api/info", async () => {
    const status = await system.getStatus();
    // /api/info is the unauthenticated surface over the same two constants
    // (`version::VERSION` / `RELEASE_VERSION`). They are baked in at compile
    // time, so any divergence means the two handlers read different sources.
    const info = await new InfoService(client).getInfo();

    expect(status.version).toBe(info.version);
    expect(status.release_version).toBe(info.release_version);
    // The diagnostic version is git-derived and carries dev suffixes; the
    // release version is pure CalVer from the workspace CALVER file.
    expect(status.version).not.toBe("");
    expect(status.release_version).toMatch(/^\d{4}\.\d{2}\.\d{2}/);
  });

  it("reports a monotonically advancing uptime", async () => {
    const first = await system.getStatus();
    expect(first.uptime_seconds).toBeGreaterThanOrEqual(0);

    // The compose stack waits on wardnetd's healthcheck before the runner
    // starts, so a second read a second later must not go backwards. Strict
    // growth is not asserted — uptime is whole seconds and the two reads can
    // land inside the same one.
    await new Promise((resolve) => setTimeout(resolve, 1_500));
    const second = await system.getStatus();
    expect(second.uptime_seconds).toBeGreaterThanOrEqual(first.uptime_seconds);
  });

  it("reports device and tunnel counts consistent with the collections", async () => {
    const status = await system.getStatus();

    expect(Number.isInteger(status.device_count)).toBe(true);
    expect(status.device_count).toBeGreaterThanOrEqual(0);
    expect(Number.isInteger(status.tunnel_count)).toBe(true);
    expect(status.tunnel_count).toBeGreaterThanOrEqual(0);
    // Active tunnels are a subset of configured tunnels — an invariant that
    // holds no matter which tunnel specs have run.
    expect(status.tunnel_active_count).toBeLessThanOrEqual(status.tunnel_count);
    expect(status.tunnel_active_count).toBeGreaterThanOrEqual(0);
  });

  it("reports plausible host resource metrics", async () => {
    const status = await system.getStatus();

    // The DB file exists by definition — the request that returned this
    // response was served from it.
    expect(status.db_size_bytes).toBeGreaterThan(0);
    expect(status.cpu_usage_percent).toBeGreaterThanOrEqual(0);
    expect(status.cpu_usage_percent).toBeLessThanOrEqual(100);
    expect(status.memory_total_bytes).toBeGreaterThan(0);
    expect(status.memory_used_bytes).toBeGreaterThan(0);
    expect(status.memory_used_bytes).toBeLessThanOrEqual(status.memory_total_bytes);
    expect(status.disk_total_bytes).toBeGreaterThan(0);
    expect(status.disk_free_bytes).toBeLessThanOrEqual(status.disk_total_bytes);
  });

  it("classifies the previous shutdown", async () => {
    const { last_shutdown: last } = await system.getStatus();

    // `unknown` on the very first boot of a fresh volume; `graceful` or
    // `unclean` once a prior run has been recorded — which specs that restart
    // the daemon (system-restart, backup-roundtrip, watchdog) produce. All
    // three are legal here; what must hold is the `at`-vs-state invariant.
    expect(["unknown", "graceful", "unclean"]).toContain(last.state);
    if (last.state === "unknown") {
      expect(last.at).toBeNull();
    } else {
      expect(last.at).not.toBeNull();
      expect(Number.isNaN(Date.parse(last.at!))).toBe(false);
    }
  });

  it("rejects an unauthenticated read with 401", async () => {
    const anon = new SystemService(new WardnetClient({ baseUrl: API_BASE_URL }));
    const err = await anon.getStatus().catch((e: unknown) => e);
    expect(err).toBeInstanceOf(WardnetApiError);
    expect((err as WardnetApiError).status).toBe(401);
  });
});
