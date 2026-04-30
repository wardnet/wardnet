import { describe, it, expect, beforeAll } from "vitest";
import { DhcpService, WardnetClient } from "@wardnet/js";

import {
  API_BASE_URL,
  AuthedClient,
  TEST_DEBIAN_AGENT,
  agentGet,
  agentPost,
  ensureAdminAndLogin,
  ipToInt,
  ipv4Of,
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

  beforeAll(async () => {
    const client = new WardnetClient({ baseUrl: API_BASE_URL });
    await waitForReady(client);
    authed = await ensureAdminAndLogin(client);
    dhcp = new DhcpService(authed);

    // Narrow the pool to the plan-prescribed .100-.150 range. The
    // daemon's default pool runs to .250 (see dhcp/service.rs), which
    // would let dhcp-reservations.spec.ts collide with a dynamic
    // lease at .151+. Idempotent: re-applying the same config across
    // spec files is a no-op.
    const cfg = (await dhcp.getConfig()).config;
    await dhcp.updateConfig({
      pool_start: POOL_START,
      pool_end: POOL_END,
      subnet_mask: cfg.subnet_mask,
      upstream_dns: cfg.upstream_dns,
      lease_duration_secs: cfg.lease_duration_secs,
      ...(cfg.router_ip ? { router_ip: cfg.router_ip } : {}),
    });
  }, 120_000);

  it("issues a lease in the daemon's pool to test_debian", async () => {
    const ifaces = await agentGet<AgentInterfacesResponse>(
      TEST_DEBIAN_AGENT,
      "/interfaces?name=eth0",
    );
    const ip = ipv4Of(ifaces, "eth0");
    const mac = macOf(ifaces, "eth0");
    expect(ip, "test_debian eth0 has an IPv4 address").toBeDefined();
    expect(mac, "test_debian eth0 has a MAC").toBeDefined();

    // Inside the daemon's DHCP pool ...
    expect(ipToInt(ip!)).toBeGreaterThanOrEqual(ipToInt(POOL_START));
    expect(ipToInt(ip!)).toBeLessThanOrEqual(ipToInt(POOL_END));
    // ... and outside Docker's IPAM range (.2-.15). A .100+ address
    // can only have come from the daemon's DHCP server; Docker's
    // bridge IPAM is configured to never hand out anything that high.
    expect(ipToInt(ip!)).toBeGreaterThan(ipToInt(DOCKER_IPAM_END));

    const { leases } = await dhcp.listLeases();
    const lease = leases.find((l) => l.ip_address === ip);
    expect(lease, `lease for ${ip} present in /api/dhcp/leases`).toBeDefined();
    expect(lease!.status).toBe("active");
    expect(lease!.mac_address.toLowerCase()).toBe(mac);
  });

  it("renews the lease without re-allocating", async () => {
    const before = await agentGet<AgentInterfacesResponse>(
      TEST_DEBIAN_AGENT,
      "/interfaces?name=eth0",
    );
    const ipBefore = ipv4Of(before, "eth0");
    expect(ipBefore).toBeDefined();

    const renew = await agentPost<AgentDhcpRenewResponse>(
      TEST_DEBIAN_AGENT,
      "/dhcp/renew",
      { interface: "eth0" },
    );
    expect(renew.release_success, `dhclient stderr: ${renew.stderr}`)
      .toBe(true);
    expect(renew.renew_success, `dhclient stderr: ${renew.stderr}`)
      .toBe(true);

    const after = await agentGet<AgentInterfacesResponse>(
      TEST_DEBIAN_AGENT,
      "/interfaces?name=eth0",
    );
    const ipAfter = ipv4Of(after, "eth0");
    expect(ipAfter, "test_debian still has an IPv4 address after renew")
      .toBeDefined();
    // MAC stickiness: same MAC asks again, same IP comes back. A
    // re-allocation here would mean the daemon forgot the binding,
    // which would also break the reservations spec downstream.
    expect(ipAfter).toBe(ipBefore);
  });
});
