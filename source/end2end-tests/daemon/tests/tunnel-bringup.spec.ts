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
  wgPeerPublicKey,
  wgShow,
} from "./helpers.js";

// E2E Stage 9 (issue #247): assigning a device to an imported tunnel triggers
// on-demand bring-up. The kernel-state agent confirms `wg show <iface>` reports
// wg_gateway_1 as a peer and the handshake completes; deleting the tunnel then
// tears the interface back down.
//
// wg_gateway_1 (10.92.0.54 on wardnet_wan) terminates the tunnel imported from
// fixtures/tunnels/tunnel-a.conf; the gateway public key from that config is
// what the daemon's peer list must carry once the interface is up. We read it
// from the fixture rather than pinning it here, so the key lives in one place.

const COUNTRY = "US";
const POOL_START = "10.91.0.100";
const POOL_END = "10.91.0.150";
const LAN_IFACE = "eth0";

describe("tunnel bring-up (issue #247)", () => {
  const client = new WardnetClient({ baseUrl: API_BASE_URL });
  let authed: AuthedClient;
  let tunnels: TunnelService;
  let devices: DeviceService;
  let tunnelId: string;
  let interfaceName: string;
  let expectedPeerKey: string;
  // Set by the bring-up test once a device is routed; gates the tear-down test
  // so both skip together when LAN discovery doesn't reach the device.
  let routedDeviceId: string | null = null;

  beforeAll(async () => {
    await waitForReady(client);
    authed = await ensureAdminAndLogin(client);
    tunnels = new TunnelService(authed);
    devices = new DeviceService(authed);

    const config = readTunnelConfig("tunnel-a.conf");
    expectedPeerKey = wgPeerPublicKey(config);
    const res = await tunnels.create({
      label: "e2e-bringup",
      country_code: COUNTRY,
      config,
    });
    tunnelId = res.tunnel.id;
    interfaceName = res.tunnel.interface_name;
  }, 120_000);

  afterAll(async () => {
    // The tear-down test usually deletes the tunnel; cleanup catches the 404 and
    // returns the device to direct if the test bailed before doing so itself.
    await cleanupTunnelRouting(devices, tunnels, routedDeviceId, tunnelId);
  });

  it("brings the interface up when a device is routed through the tunnel", async (ctx) => {
    // Lease test_debian and locate its device row. LAN device discovery via
    // packet capture is a known-flaky area of this harness (see helpers.ts);
    // skip rather than fail when the daemon can't observe the LAN here.
    await ensureLeasedAgent(authed, TEST_DEBIAN_AGENT, LAN_IFACE, POOL_START, POOL_END);
    const device = await findDeviceByIpRangeOrNull(authed, POOL_START, POOL_END);
    if (!device) {
      ctx.skip();
      return;
    }
    routedDeviceId = device.id;

    // Nothing is routed through the tunnel yet, so the interface is absent.
    expect((await wgShow(DAEMON_AGENT, interfaceName)).exists).toBe(false);

    // Assigning the device to the tunnel triggers on-demand bring-up; the
    // helper waits for the interface and the per-device rule to land.
    await routeThroughTunnel(devices, device.id, tunnelId, DAEMON_AGENT, interfaceName);

    const wg = await wgShow(DAEMON_AGENT, interfaceName);
    expect(wg.exists).toBe(true);
    expect(wg.listening_port ?? 0).toBeGreaterThan(0);
    // The peer the daemon dials is wg_gateway_1 — its public key must appear.
    const peerKeys = (wg.peers ?? []).map((p) => p.public_key);
    expect(peerKeys).toContain(expectedPeerKey);

    // The health-check loop flips the tunnel to `up` once it sees the handshake.
    await waitForTunnelStatus(tunnels, tunnelId, "up", 60_000);
  }, 180_000);

  it("tears the interface down when the tunnel is deleted", async (ctx) => {
    if (!routedDeviceId) {
      ctx.skip();
      return;
    }

    // Deleting the tunnel switches the routed device back to direct and tears
    // the WireGuard interface down.
    await tunnels.delete(tunnelId);

    const gone = await waitForWgInterface(DAEMON_AGENT, interfaceName, false);
    expect(gone.exists).toBe(false);
    await waitForTunnelRule(DAEMON_AGENT, false);
  }, 60_000);
});
