import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { TunnelService, WardnetClient, type CreateTunnelResponse } from "@wardnet/js";

import {
  API_BASE_URL,
  AuthedClient,
  DAEMON_AGENT,
  ensureAdminAndLogin,
  readTunnelConfig,
  waitForReady,
  wgShow,
} from "./helpers.js";

// E2E Stage 9 (issue #247): import a WireGuard tunnel from a fixture .conf and
// prove the pure lifecycle — `create` lands it in `list`, a second import is
// tracked independently on its own interface, and `delete` clears it — all
// without bringing the interface up (no device is routed through it here).
//
// The Endpoints below are wg_gateway_1 / wg_gateway_2 on wardnet_wan, but this
// file never dials them: it exercises the config/DB path only. Bring-up,
// counters, and fallback live in the sibling tunnel-* specs.
//
// Both tunnels are imported once in beforeAll and referenced by name, so the
// tests don't form an implicit ordered chain through shared mutable state.

const COUNTRY = "US";

describe("tunnel import lifecycle (issue #247)", () => {
  const client = new WardnetClient({ baseUrl: API_BASE_URL });
  let authed: AuthedClient;
  let tunnels: TunnelService;
  let importA: CreateTunnelResponse;
  let importB: CreateTunnelResponse;

  beforeAll(async () => {
    await waitForReady(client);
    authed = await ensureAdminAndLogin(client);
    tunnels = new TunnelService(authed);
    importA = await tunnels.create({
      label: "e2e-import-a",
      country_code: COUNTRY,
      config: readTunnelConfig("tunnel-a.conf"),
    });
    importB = await tunnels.create({
      label: "e2e-import-b",
      country_code: COUNTRY,
      config: readTunnelConfig("tunnel-b.conf"),
    });
  }, 120_000);

  afterAll(async () => {
    // Best-effort teardown so re-runs against the persistent state volume start
    // clean and later tunnel specs see a known-empty tunnel list.
    for (const res of [importA, importB]) {
      try {
        await tunnels.delete(res.tunnel.id);
      } catch {
        // already gone
      }
    }
  });

  it("parses the .conf into a tunnel and lists it", async () => {
    expect(importA.tunnel.id).toBeTruthy();
    expect(importA.tunnel.label).toBe("e2e-import-a");
    expect(importA.tunnel.country_code).toBe(COUNTRY);
    // Endpoint is parsed straight out of the [Peer] section.
    expect(importA.tunnel.endpoint).toBe("10.92.0.54:51820");
    // An interface is allocated at import time, but nothing is on the wire yet.
    expect(importA.tunnel.interface_name).toMatch(/^wg_ward\d+$/);
    expect(importA.tunnel.status).toBe("down");

    const { tunnels: list } = await tunnels.list();
    const found = list.find((t) => t.id === importA.tunnel.id);
    expect(found).toBeDefined();
    expect(found?.label).toBe("e2e-import-a");
  }, 30_000);

  it("does not bring the interface up on import", async () => {
    // No device targets the tunnel, so on-demand bring-up hasn't fired — the
    // kernel interface should not exist.
    const wg = await wgShow(DAEMON_AGENT, importA.tunnel.interface_name);
    expect(wg.exists).toBe(false);
  }, 30_000);

  it("tracks a second import independently, on its own interface", async () => {
    expect(importB.tunnel.endpoint).toBe("10.92.0.55:51820");

    const { tunnels: list } = await tunnels.list();
    const ids = list.map((t) => t.id);
    expect(ids).toContain(importA.tunnel.id);
    expect(ids).toContain(importB.tunnel.id);

    // Each import gets its own interface.
    expect(importA.tunnel.interface_name).not.toBe(importB.tunnel.interface_name);
  }, 30_000);

  it("clears a tunnel from the list on delete", async () => {
    await tunnels.delete(importA.tunnel.id);

    const { tunnels: list } = await tunnels.list();
    expect(list.some((t) => t.id === importA.tunnel.id)).toBe(false);
    // The other import is untouched.
    expect(list.some((t) => t.id === importB.tunnel.id)).toBe(true);
  }, 30_000);
});
