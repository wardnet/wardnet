import { afterAll, beforeAll, describe, expect, it } from "vitest";
import {
  DeviceService,
  ProviderService,
  TunnelService,
  WardnetClient,
} from "@wardnet/js";
import {
  API_BASE_URL,
  AuthedClient,
  TEST_DEBIAN_AGENT,
  ensureAdminAndLogin,
  ensureLeasedAgent,
  findDeviceByIpRangeOrNull,
  pingViaAgent,
  waitForReady,
} from "./helpers.js";

// E2E Stage 10 (issue #248): exercise the VPN provider integration end to end
// against the nordvpn_mock container, then bring a *real* WireGuard tunnel up
// to the wg_gateway and prove a LAN device's traffic egresses through it.
//
// Topology (see compose.yaml):
//   nordvpn_mock  10.92.0.52  — mock api.nordvpn.com (daemon config points here)
//   wg_gateway    10.92.0.53  — WireGuard exit peer (hostname the mock returns)
//                 10.93.0.1   — masquerades tunnel traffic onto wardnet_internet
//   internet_target 10.93.0.10 — reachable ONLY through the tunnel

const PROVIDER_ID = "nordvpn";
// Must match nordvpn_mock's NORDVPN_VALID_TOKEN (default in server.mjs).
const VALID_TOKEN = "valid-nordvpn-token";
const INVALID_TOKEN = "definitely-not-a-valid-token";
// Present in fixtures/nordvpn/countries.json and the single server fixture.
const SETUP_COUNTRY = "US";
// wg_gateway's WAN IP — the hostname advertised in fixtures/nordvpn/servers.json
// and therefore the WireGuard endpoint the daemon dials.
const GATEWAY_HOSTNAME = "10.92.0.53";
// Lives alone on wardnet_internet; reachable from the LAN only via the tunnel.
const INTERNET_TARGET_IP = "10.93.0.10";
const POOL_START = "10.91.0.100";
const POOL_END = "10.91.0.150";
const LAN_IFACE = "eth0";

describe("nordvpn provider integration", () => {
  let authed: AuthedClient;
  let providers: ProviderService;
  let tunnels: TunnelService;
  let devices: DeviceService;
  let createdTunnelId: string | undefined;

  beforeAll(async () => {
    const client = new WardnetClient({ baseUrl: API_BASE_URL });
    await waitForReady(client);
    authed = await ensureAdminAndLogin(client);
    providers = new ProviderService(authed);
    tunnels = new TunnelService(authed);
    devices = new DeviceService(authed);
  }, 120_000);

  afterAll(async () => {
    // Best-effort teardown so re-runs against the persistent state volume
    // start clean: unroute any device on the tunnel, then delete it.
    if (createdTunnelId) {
      try {
        const { devices: routed } = await tunnels.listDevices(createdTunnelId);
        for (const d of routed) {
          try {
            await devices.update(d.id, { routing_target: { type: "direct" } });
          } catch {
            // ignore
          }
        }
      } catch {
        // ignore
      }
      try {
        await tunnels.delete(createdTunnelId);
      } catch {
        // ignore
      }
    }
  });

  it("registers the NordVPN provider and exposes it via /api/providers", async () => {
    const { providers: list } = await providers.list();
    const nord = list.find((p) => p.id === PROVIDER_ID);
    expect(nord).toBeDefined();
    expect(nord?.auth_methods).toContain("token");
  });

  it("accepts a valid token and rejects an invalid one", async () => {
    const ok = await providers.validateCredentials(PROVIDER_ID, {
      credentials: { type: "token", token: VALID_TOKEN },
    });
    expect(ok.valid).toBe(true);

    // The mock answers /v1/users/services with 401 for a bad token, which the
    // provider maps to `valid: false` (not an error).
    const bad = await providers.validateCredentials(PROVIDER_ID, {
      credentials: { type: "token", token: INVALID_TOKEN },
    });
    expect(bad.valid).toBe(false);
  });

  it("lists countries from the mock fixtures", async () => {
    const { countries } = await providers.listCountries(PROVIDER_ID);
    expect(countries.length).toBeGreaterThan(0);
    expect(countries.map((c) => c.code)).toContain(SETUP_COUNTRY);
  });

  it("sets up a tunnel from provider credentials", async () => {
    const res = await providers.setupTunnel(PROVIDER_ID, {
      credentials: { type: "token", token: VALID_TOKEN },
      country: SETUP_COUNTRY,
      label: "e2e-nordvpn",
    });
    createdTunnelId = res.tunnel.id;
    expect(res.tunnel.id).toBeTruthy();
    // The resolved server (and thus the WireGuard endpoint) is the gateway.
    expect(res.server.hostname).toBe(GATEWAY_HOSTNAME);

    const { tunnels: all } = await tunnels.list();
    expect(all.some((t) => t.id === createdTunnelId)).toBe(true);
  }, 60_000);

  it("routes a LAN device through the tunnel to the tunnel-only target", async (ctx) => {
    const tunnelId = createdTunnelId;
    if (!tunnelId) {
      ctx.skip();
      return;
    }

    // Lease test_debian so the daemon knows it by an in-pool IP, then locate
    // the device row. LAN device discovery via packet capture is a known-flaky
    // area of this harness (see helpers.ts); skip rather than fail when the
    // daemon can't observe the LAN in this environment.
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

    // Negative control: with no tunnel rule the daemon has no route to the
    // isolated internet segment (and Docker inter-network isolation blocks any
    // host-bridged path), so the target is unreachable. Ping the leased source
    // so the assertion targets exactly the IP the daemon will route.
    const before = await pingViaAgent(TEST_DEBIAN_AGENT, INTERNET_TARGET_IP, {
      source: leasedIp,
      count: 2,
      timeout: 2,
    });
    expect(before.received).toBe(0);

    // Assign the device to the tunnel. The routing service brings the tunnel
    // up on demand and installs `ip rule from <leasedIp>` + masquerade on the
    // WireGuard interface.
    await devices.update(device.id, {
      routing_target: { type: "tunnel", tunnel_id: tunnelId },
    });

    // The WireGuard handshake and route programming take a moment; poll until
    // ICMP replies come back through the tunnel.
    let lastReceived = 0;
    let reachable = false;
    const deadline = Date.now() + 30_000;
    while (Date.now() < deadline) {
      const p = await pingViaAgent(TEST_DEBIAN_AGENT, INTERNET_TARGET_IP, {
        source: leasedIp,
        count: 2,
        timeout: 2,
      });
      lastReceived = p.received;
      if (p.received > 0) {
        reachable = true;
        break;
      }
      await new Promise((r) => setTimeout(r, 2_000));
    }
    expect(
      reachable,
      `expected ICMP replies from ${INTERNET_TARGET_IP} through the tunnel (last received=${lastReceived})`,
    ).toBe(true);
  }, 120_000);
});
