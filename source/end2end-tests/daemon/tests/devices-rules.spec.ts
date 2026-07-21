import { afterAll, beforeAll, describe, expect, it } from "vitest";
import {
  DeviceService,
  ProviderService,
  TunnelService,
  WardnetClient,
  type Device,
} from "@wardnet/js";

import {
  API_BASE_URL,
  AuthedClient,
  DAEMON_AGENT,
  TEST_DEBIAN_AGENT,
  ensureAdminAndLogin,
  ensureLeasedAgent,
  findDeviceByIpOrNull,
  proxyToDaemon,
  waitForReady,
  waitForTunnelRule,
} from "./helpers.js";

// Per-device routing rules, asserted against the daemon's live `ip rule`
// set via the server-mode test agent in the wardnetd container.
//
// The daemon models routing targets as Direct / Tunnel / Default — there
// is no standalone "Block" target (device isolation is a Network Zone
// concern, not a routing rule), so the two kernel-observable outcomes are:
//   - Tunnel → `ip rule from <device_ip>/32 lookup <tunnel_table>`
//   - Direct → no per-device rule (the device uses the main table)
// This spec drives both and checks the rule appears / disappears. The
// tunnel is provisioned through the nordvpn_mock exactly as
// `nordvpn-provider.spec.ts` does (Stage 10, issue #248).
const PROVIDER_ID = "nordvpn";
const VALID_TOKEN = "valid-nordvpn-token";
const SETUP_COUNTRY = "US";
const POOL_START = "10.91.0.100";
const POOL_END = "10.91.0.150";
const LAN_IFACE = "eth0";

describe("devices — per-device routing rules vs. kernel state", () => {
  let authed: AuthedClient;
  let devices: DeviceService;
  let tunnels: TunnelService;
  let providers: ProviderService;
  let device: Device | null = null;
  let leasedIp = "";
  let tunnelId: string | undefined;

  beforeAll(async () => {
    const client = new WardnetClient({ baseUrl: API_BASE_URL });
    await waitForReady(client);
    authed = await ensureAdminAndLogin(client);
    devices = new DeviceService(authed);
    tunnels = new TunnelService(authed);
    providers = new ProviderService(authed);

    // Provision a tunnel against the mock provider. This does not depend on
    // LAN device discovery, so it runs unconditionally.
    const setup = await providers.setupTunnel(PROVIDER_ID, {
      credentials: { type: "token", token: VALID_TOKEN },
      country: SETUP_COUNTRY,
      label: "e2e-devices-rules",
    });
    tunnelId = setup.tunnel.id;

    leasedIp = await ensureLeasedAgent(
      authed,
      TEST_DEBIAN_AGENT,
      LAN_IFACE,
      POOL_START,
      POOL_END,
    );
    device = await findDeviceByIpOrNull(authed, leasedIp);
  }, 180_000);

  afterAll(async () => {
    // Restore direct routing and drop the tunnel so re-runs against the
    // persistent state volume start clean.
    if (device) {
      try {
        await devices.update(device.id, { routing_target: { type: "direct" } });
      } catch {
        // ignore
      }
    }
    if (tunnelId) {
      try {
        await tunnels.delete(tunnelId);
      } catch {
        // ignore
      }
    }
  });

  it("tunnel routing installs a per-device ip rule; self-service direct tears it down", async (ctx) => {
    if (!device || !tunnelId) return ctx.skip();

    // Admin assigns the device to the tunnel → the routing service brings
    // the tunnel up on demand and installs `ip rule from <device_ip>/32
    // lookup <tunnel_table>`. Rules are applied asynchronously off the
    // RoutingRuleChanged event, so poll. We match on shape (a /32 source at
    // a tunnel table) rather than a fixed IP: a client's last_ip can be its
    // lease or its docker-IPAM address, and the daemon rules whichever it
    // currently holds.
    await devices.update(device.id, {
      routing_target: { type: "tunnel", tunnel_id: tunnelId },
    });
    const rule = await waitForTunnelRule(DAEMON_AGENT, true);
    expect(rule).toBeDefined();
    expect(Number(rule?.table)).toBeGreaterThanOrEqual(100);

    // The device drives its own rule back to direct through the proxy, so
    // the daemon classifies the mutation by source IP (AuthContext::Device).
    // Bind the device's *current* address so the classification resolves
    // regardless of which of its IPs the daemon last observed.
    const current = await devices.getById(device.id);
    const res = await proxyToDaemon(TEST_DEBIAN_AGENT, {
      method: "PUT",
      path: "/api/devices/me/rule",
      sourceIp: current.device.last_ip,
      body: { target: { type: "direct" } },
    });
    expect(res.status).toBe(200);

    // Direct routing tears the per-device rule down — the device is back on
    // the main table.
    await waitForTunnelRule(DAEMON_AGENT, false);
  });
});
