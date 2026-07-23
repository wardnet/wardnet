import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { DeviceService, TunnelService, WardnetClient } from "@wardnet/js";

import {
  API_BASE_URL,
  AuthedClient,
  DAEMON_AGENT,
  TEST_DEBIAN_AGENT,
  cleanupTunnelRouting,
  ensureAdminAndLogin,
  ensureLeasedAgent,
  findDeviceByIpRangeOrNull,
  readTunnelConfig,
  routeThroughTunnel,
  waitForReady,
  waitForTunnelRule,
  waitForTunnelStatus,
  waitForWgInterface,
} from "./helpers.js";

// E2E Stage 9 (issue #247): the folded-in Milestone 1l "tunnel-down + fallback"
// case. A device routed through a live tunnel is switched back to direct
// routing when the tunnel goes down — here by deleting it, which exercises the
// same `switch_tunnel_rules_to_direct` path a tear-down takes. The daemon fails
// open: the per-device rule and interface disappear and the device's resolved
// rule reverts from `tunnel` to `direct` rather than losing connectivity
// outright.
//
// This asserts the routing-state transition, which is the fallback contract.
// The data-plane proof that traffic actually egresses through the tunnel while
// it is up lives in tunnel-stats.spec.ts.

const COUNTRY = "US";
const POOL_START = "10.91.0.100";
const POOL_END = "10.91.0.150";
const LAN_IFACE = "eth0";

describe("tunnel down + fallback (issue #247)", () => {
  const client = new WardnetClient({ baseUrl: API_BASE_URL });
  let authed: AuthedClient;
  let tunnels: TunnelService;
  let devices: DeviceService;
  let tunnelId: string;
  let interfaceName: string;
  let routedDeviceId: string | undefined;

  beforeAll(async () => {
    await waitForReady(client);
    authed = await ensureAdminAndLogin(client);
    tunnels = new TunnelService(authed);
    devices = new DeviceService(authed);

    const res = await tunnels.create({
      label: "e2e-fallback",
      country_code: COUNTRY,
      config: readTunnelConfig("tunnel-a.conf"),
    });
    tunnelId = res.tunnel.id;
    interfaceName = res.tunnel.interface_name;
  }, 120_000);

  afterAll(async () => {
    await cleanupTunnelRouting(devices, tunnels, routedDeviceId, tunnelId);
  });

  it("falls the device back to direct when its tunnel goes down", async (ctx) => {
    await ensureLeasedAgent(authed, TEST_DEBIAN_AGENT, LAN_IFACE, POOL_START, POOL_END);
    const device = await findDeviceByIpRangeOrNull(authed, POOL_START, POOL_END);
    if (!device) {
      ctx.skip();
      return;
    }

    // Route the device through the tunnel and confirm it is live: the interface
    // is up, the per-device rule is installed, the resolved rule is the tunnel,
    // and the health-check loop has seen a handshake.
    await routeThroughTunnel(devices, device.id, tunnelId, DAEMON_AGENT, interfaceName);
    routedDeviceId = device.id;
    const routed = await devices.getById(device.id);
    expect(routed.current_rule?.type).toBe("tunnel");
    await waitForTunnelStatus(tunnels, tunnelId, "up", 60_000);

    // Tunnel goes down. delete_tunnel switches every affected device to direct
    // (fail-open) so they don't lose connectivity outright.
    await tunnels.delete(tunnelId);

    // Fallback: the per-device rule and the interface are gone...
    await waitForTunnelRule(DAEMON_AGENT, false);
    const gone = await waitForWgInterface(DAEMON_AGENT, interfaceName, false);
    expect(gone.exists).toBe(false);

    // ...and the device's resolved rule has reverted from tunnel to direct.
    const after = await devices.getById(device.id);
    expect(after.current_rule?.type).toBe("direct");
  }, 180_000);
});
