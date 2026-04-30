import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { DhcpService, WardnetClient } from "@wardnet/js";

import {
  API_BASE_URL,
  AuthedClient,
  TEST_DEBIAN_AGENT,
  agentGet,
  agentPost,
  ensureAdminAndLogin,
  ipv4Of,
  macOf,
  waitForReady,
  type AgentDhcpRenewResponse,
  type AgentInterfacesResponse,
} from "./helpers.js";

// .151-.199 is the reserved-static range per the e2e plan: outside
// the dynamic pool (.100-.150) so a reservation can't accidentally
// collide with a concurrent dynamic lease.
const RESERVED_IP = "10.91.0.160";

describe("dhcp reservations", () => {
  let authed: AuthedClient;
  let dhcp: DhcpService;
  let createdReservationId: string | undefined;

  beforeAll(async () => {
    const client = new WardnetClient({ baseUrl: API_BASE_URL });
    await waitForReady(client);
    authed = await ensureAdminAndLogin(client);
    dhcp = new DhcpService(authed);
  }, 120_000);

  afterAll(async () => {
    // Best-effort cleanup so a re-run on the same compose stack
    // doesn't fail with "MAC already reserved". Failures here are
    // suppressed because they'd hide the real spec failure.
    if (createdReservationId) {
      try {
        await dhcp.deleteReservation(createdReservationId);
      } catch {
        // ignore
      }
    }
  });

  it("binds test_debian's MAC to a reserved IP on renew", async () => {
    const before = await agentGet<AgentInterfacesResponse>(
      TEST_DEBIAN_AGENT,
      "/interfaces?name=eth0",
    );
    const mac = macOf(before, "eth0");
    expect(mac, "test_debian eth0 has a MAC").toBeDefined();

    // If a previous run on the same volume left a reservation for
    // this MAC, drop it so createReservation() doesn't 409.
    const existing = await dhcp.listReservations();
    for (const r of existing.reservations) {
      if (r.mac_address.toLowerCase() === mac) {
        await dhcp.deleteReservation(r.id);
      }
    }

    const created = await dhcp.createReservation({
      mac_address: mac!,
      ip_address: RESERVED_IP,
      hostname: "test-debian-reserved",
    });
    createdReservationId = created.reservation.id;
    expect(created.reservation.ip_address).toBe(RESERVED_IP);

    // Force the client to release its dynamic lease and re-DISCOVER.
    // The daemon should now answer with the reserved IP because the
    // MAC matches an active reservation.
    const renew = await agentPost<AgentDhcpRenewResponse>(
      TEST_DEBIAN_AGENT,
      "/dhcp/renew",
      { interface: "eth0" },
    );
    // Strict: a release failure here would let dhclient re-use the
    // cached dynamic lease and silently mask the reservation flow.
    expect(renew.release_success, `dhclient stderr: ${renew.stderr}`)
      .toBe(true);
    expect(renew.renew_success, `dhclient stderr: ${renew.stderr}`)
      .toBe(true);

    const after = await agentGet<AgentInterfacesResponse>(
      TEST_DEBIAN_AGENT,
      "/interfaces?name=eth0",
    );
    expect(ipv4Of(after, "eth0")).toBe(RESERVED_IP);

    const { leases } = await dhcp.listLeases();
    const lease = leases.find((l) => l.ip_address === RESERVED_IP);
    expect(lease, `lease for ${RESERVED_IP} present`).toBeDefined();
    expect(lease!.mac_address.toLowerCase()).toBe(mac);
    expect(lease!.status).toBe("active");
  });

  it("removes the reservation from the listing on delete", async () => {
    expect(createdReservationId, "previous test created a reservation")
      .toBeDefined();

    await dhcp.deleteReservation(createdReservationId!);
    createdReservationId = undefined;

    const { reservations } = await dhcp.listReservations();
    expect(reservations.find((r) => r.ip_address === RESERVED_IP))
      .toBeUndefined();
  });
});
