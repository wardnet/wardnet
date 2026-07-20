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
  waitForDeviceIpRule,
  waitForReady,
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

  it("assigning a device to a tunnel installs a per-device ip rule, and direct removes it", async (ctx) => {
    if (!device || !tunnelId) return ctx.skip();

    // Assign → the routing service brings the tunnel up on demand and
    // installs `ip rule from <leasedIp>/32 lookup <tunnel_table>`. Rules are
    // applied asynchronously off the RoutingRuleChanged event, so poll.
    await devices.update(device.id, {
      routing_target: { type: "tunnel", tunnel_id: tunnelId },
    });
    const rule = await waitForDeviceIpRule(DAEMON_AGENT, leasedIp, true);
    expect(rule).toBeDefined();
    // Wardnet-managed per-device rules point at a dedicated tunnel table
    // (tables >= 100); the kernel default rules use main/default/local.
    const table = Number(rule?.table);
    expect(Number.isNaN(table)).toBe(false);
    expect(table).toBeGreaterThanOrEqual(100);

    // Switch back to direct → the per-device rule is torn down.
    await devices.update(device.id, { routing_target: { type: "direct" } });
    await waitForDeviceIpRule(DAEMON_AGENT, leasedIp, false);
  });

  it("self-service setMyRule(direct) leaves the device on the main table", async (ctx) => {
    if (!device) return ctx.skip();

    // Ensure a known non-direct starting point so the assertion is a real
    // transition, not a no-op on an already-direct device.
    await devices.update(device.id, {
      routing_target: { type: "tunnel", tunnel_id: tunnelId! },
    });
    await waitForDeviceIpRule(DAEMON_AGENT, leasedIp, true);

    // The device drives its own rule back to direct through the proxy, so
    // the daemon classifies the mutation by source IP (AuthContext::Device).
    const res = await proxyToDaemon(TEST_DEBIAN_AGENT, {
      method: "PUT",
      path: "/api/devices/me/rule",
      sourceIp: leasedIp,
      body: { target: { type: "direct" } },
    });
    expect(res.status).toBe(200);

    await waitForDeviceIpRule(DAEMON_AGENT, leasedIp, false);
  });
});
