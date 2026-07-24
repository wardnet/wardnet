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
  pingViaAgent,
  readTunnelConfig,
  routeThroughTunnel,
  waitForReady,
} from "./helpers.js";

// E2E Stage 9 (issue #247): with a device routed through the tunnel, driving
// ICMP to the gateway's inner address moves real bytes over WireGuard, and the
// tunnel's rx/tx counters — surfaced by `list` — climb off zero.

const COUNTRY = "US";
// wg_gateway_1's wg0 inner address. tunnel-a.conf's AllowedIPs covers
// 10.9.1.0/24, so pinging this steers through the tunnel and the gateway
// answers on its own interface (no NAT needed).
const GATEWAY_A_INNER = "10.9.1.1";
const POOL_START = "10.91.0.100";
const POOL_END = "10.91.0.150";
const LAN_IFACE = "eth0";

describe("tunnel stats (issue #247)", () => {
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
      label: "e2e-stats",
      country_code: COUNTRY,
      config: readTunnelConfig("tunnel-a.conf"),
    });
    tunnelId = res.tunnel.id;
    interfaceName = res.tunnel.interface_name;
  }, 120_000);

  afterAll(async () => {
    await cleanupTunnelRouting(devices, tunnels, routedDeviceId, tunnelId);
  });

  it("reports non-zero rx/tx once traffic flows through the tunnel", async (ctx) => {
    const leasedIp = await ensureLeasedAgent(
      authed,
      TEST_DEBIAN_AGENT,
      LAN_IFACE,
      POOL_START,
      POOL_END,
    );
    const device = await findDeviceByIpRangeOrNull(authed, POOL_START, POOL_END);
    if (!device) {
      ctx.skip();
      return;
    }

    // Route the device through the tunnel and wait for the interface + rule so
    // pings actually take the tunnel path rather than being dropped as
    // un-routed.
    await routeThroughTunnel(devices, device.id, tunnelId, DAEMON_AGENT, interfaceName);
    routedDeviceId = device.id;

    // Drive ICMP through the tunnel and poll the persisted counters (the
    // health-check loop writes them back every stats_interval_secs, 5 s by
    // default) until both climb off zero. Ping the leased source so the
    // daemon's `ip rule from <ip>` steers the packets into the tunnel table.
    let bytesTx = 0;
    let bytesRx = 0;
    const deadline = Date.now() + 60_000;
    while (Date.now() < deadline) {
      await pingViaAgent(TEST_DEBIAN_AGENT, GATEWAY_A_INNER, {
        source: leasedIp,
        count: 3,
        timeout: 2,
      });
      const { tunnels: list } = await tunnels.list();
      const t = list.find((x) => x.id === tunnelId);
      bytesTx = t?.bytes_tx ?? 0;
      bytesRx = t?.bytes_rx ?? 0;
      if (bytesTx > 0 && bytesRx > 0) break;
      await new Promise((r) => setTimeout(r, 2_000));
    }

    expect(
      bytesTx,
      `expected non-zero tx counter (saw tx=${bytesTx} rx=${bytesRx})`,
    ).toBeGreaterThan(0);
    expect(
      bytesRx,
      `expected non-zero rx counter (saw tx=${bytesTx} rx=${bytesRx})`,
    ).toBeGreaterThan(0);
  }, 180_000);
});
