import { describe, it, expect, beforeAll } from "vitest";
import { DhcpService, WardnetClient } from "@wardnet/js";

import {
  API_BASE_URL,
  AuthedClient,
  TEST_DEBIAN_AGENT,
  acquireLeaseInRange,
  agentGet,
  agentPost,
  ensureAdminAndLogin,
  ipToInt,
  ipv4InRange,
  macOf,
  waitForReady,
  type AgentDhcpRenewResponse,
  type AgentInterfacesResponse,
} from "./helpers.js";

const POOL_START = "10.91.0.100";
const POOL_END = "10.91.0.150";
const DOCKER_IPAM_END = "10.91.0.15";

describe("dhcp", () => {
  let authed: AuthedClient;
  let dhcp: DhcpService;
  let leasedIp: string;

  beforeAll(async () => {
    const client = new WardnetClient({ baseUrl: API_BASE_URL });
    await waitForReady(client);
    authed = await ensureAdminAndLogin(client);
    dhcp = new DhcpService(authed);

    // The daemon ships with DHCP disabled; flip it on before any
    // client tries to acquire a lease. Idempotent across spec files.
    const cfg = (await dhcp.getConfig()).config;
    if (!cfg.enabled) {
      await dhcp.toggle({ enabled: true });
    }
    // Narrow the pool to the plan-prescribed .100-.150 range. The
    // default runs to .250 and would let dynamic leases collide with
    // the .151-.199 reservation range used by the next spec.
    await dhcp.updateConfig({
      pool_start: POOL_START,
      pool_end: POOL_END,
      subnet_mask: cfg.subnet_mask,
      upstream_dns: cfg.upstream_dns,
      lease_duration_secs: cfg.lease_duration_secs,
      ...(cfg.router_ip ? { router_ip: cfg.router_ip } : {}),
    });

    // Drive the agent to acquire its first lease. The container
    // entrypoint deliberately does not run dhclient at startup —
    // doing so would race the toggle above and fail container start.
    leasedIp = await acquireLeaseInRange(
      TEST_DEBIAN_AGENT,
      "eth0",
      POOL_START,
      POOL_END,
    );
  }, 120_000);

  it("issues a lease in the daemon's pool to test_debian", async () => {
    expect(ipToInt(leasedIp)).toBeGreaterThanOrEqual(ipToInt(POOL_START));
    expect(ipToInt(leasedIp)).toBeLessThanOrEqual(ipToInt(POOL_END));
    // Outside Docker's IPAM range — a .100+ address can only have come
    // from the daemon's DHCP server.
    expect(ipToInt(leasedIp)).toBeGreaterThan(ipToInt(DOCKER_IPAM_END));

    const ifaces = await agentGet<AgentInterfacesResponse>(
      TEST_DEBIAN_AGENT,
      "/interfaces?name=eth0",
    );
    const mac = macOf(ifaces, "eth0");
    expect(mac, "test_debian eth0 has a MAC").toBeDefined();

    const { leases } = await dhcp.listLeases();
    const lease = leases.find((l) => l.ip_address === leasedIp);
    expect(lease, `lease for ${leasedIp} present in /api/dhcp/leases`)
      .toBeDefined();
    expect(lease!.status).toBe("active");
    expect(lease!.mac_address.toLowerCase()).toBe(mac);
  });

  it("renews the lease without re-allocating", async () => {
    const renew = await agentPost<AgentDhcpRenewResponse>(
      TEST_DEBIAN_AGENT,
      "/dhcp/renew",
      { interface: "eth0" },
    );
    expect(renew.renew_success, `dhclient stderr: ${renew.stderr}`)
      .toBe(true);

    const ifaces = await agentGet<AgentInterfacesResponse>(
      TEST_DEBIAN_AGENT,
      "/interfaces?name=eth0",
    );
    const ipAfter = ipv4InRange(ifaces, "eth0", POOL_START, POOL_END);
    expect(ipAfter, "test_debian still has an in-pool IPv4 after renew")
      .toBeDefined();
    // MAC stickiness: same MAC asks again, same IP comes back.
    expect(ipAfter).toBe(leasedIp);
  });
});
