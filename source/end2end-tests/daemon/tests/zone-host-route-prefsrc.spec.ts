/**
 * Member-isolation host-route preferred source — issue #1198.
 *
 * A device in a member-isolated, subnetted zone gets a `/32` host route so the
 * Pi has an on-link path to it. That `/32` is more specific than the zone's
 * `/24`, so it wins the route lookup — and if it carries no `RTA_PREFSRC` it
 * drops the `src <gateway>` hint the `/24` supplied. Source selection for
 * locally-generated traffic then falls back to the LAN interface's primary
 * address, the daemon's DNS replies to that device leave with the wrong source,
 * postrouting `masquerade` rewrites the address back and reallocates the source
 * port out of netfilter's reserved sub-512 pool, and no client's connected UDP
 * socket accepts the answer. Every device in the zone loses DNS and reports
 * "no internet".
 *
 * This is the shape the bug took in production: a phone joined, landed in a
 * member-isolated guest zone, took a lease from that zone's pool, and had no
 * working DNS from the moment it associated.
 *
 * None of it is reachable from a unit test — `rtnetlink` has no mockable
 * boundary, so what the kernel actually stores, and whether `NLM_F_REPLACE`
 * repairs a route an older build installed, can only be asserted here.
 *
 * The device's address comes from the zone's own DHCP pool rather than being
 * hand-picked, so the in-subnet guard on the host route is exercised for real.
 *
 * It drives `test_ubuntu` rather than a dedicated client. An earlier version
 * added a third container for isolation, but that is the one change that
 * correlates with the rest of the suite failing to lease (a run without it was
 * green, three with it were not) — a third client perturbs the shared stack
 * more than borrowing an existing one does. This file sorts last, so the zone
 * move and re-lease happen after every other spec has finished with the
 * client, and `afterAll` puts it back on the base LAN in its original zone.
 */

import { afterAll, beforeAll, describe, expect, it } from "vitest";
import {
  DeviceService,
  NetworkZonesService,
  SystemService,
  WardnetClient,
  type Device,
} from "@wardnet/js";

import {
  API_BASE_URL,
  type AgentInterfacesResponse,
  AuthedClient,
  agentGet,
  DAEMON_AGENT,
  TEST_UBUNTU_AGENT,
  acquireLeaseInRange,
  clearHostRoutePrefsrc,
  daemonPid,
  daemonRoutes,
  ensureAdminAndLogin,
  findDeviceByIpRangeOrNull,
  hostRouteFor,
  pollUntil,
  resolveViaAgent,
  waitForDaemonRestart,
  waitForHostRouteSrc,
  waitForReady,
} from "./helpers.js";

// The isolated zone's own subnet. Deliberately outside the e2e LAN
// (10.91.0.0/24) so a lease from this range proves the device really moved
// into the zone's scope rather than keeping its old address.
const ZONE_CIDR = "10.44.1.0/24";
const ZONE_GATEWAY = "10.44.1.1";
const ZONE_RANGE_START = "10.44.1.2";
const ZONE_RANGE_END = "10.44.1.254";

// Base-LAN pool used to get the guest client discovered in the first place.
//
// Must be the pool every other spec uses: `ensureLeasedAgent` rewrites the
// daemon's *global* DHCP config, so inventing a range here would leave the
// shared stack serving it — and the clients other specs lease would stop
// getting addresses in the range they expect. (.160/.170 are also
// dhcp-reservations' and backup-roundtrip's reserved addresses.)
const LAN_POOL_START = "10.91.0.100";
const LAN_POOL_END = "10.91.0.150";

const ZONE_NAME = "e2e-guest-isolated";

describe("member-isolation host route preferred source (#1198)", () => {
  let authed: AuthedClient;
  let zones: NetworkZonesService;
  let devices: DeviceService;
  let system: SystemService;

  /** Null when the daemon never discovered the guest client (see below). */
  let guest: Device | null = null;
  let zoneId: string | undefined;
  let originalZoneId: string | undefined;
  /** The address the guest holds inside the zone subnet. */
  let zoneIp: string | undefined;
  let lanIface: string | undefined;

  beforeAll(async () => {
    const client = new WardnetClient({ baseUrl: API_BASE_URL });
    await waitForReady(client);
    authed = await ensureAdminAndLogin(client);
    zones = new NetworkZonesService(authed);
    devices = new DeviceService(authed);
    system = new SystemService(authed);

    // Get the guest client onto the LAN so the daemon discovers it.
    //
    // Deliberately NOT `ensureLeasedAgent`: that rewrites the daemon's *global*
    // DHCP config, and this spec has no business changing the pool the rest of
    // the suite leases from. `acquireLeaseInRange` only drives this one client.
    await acquireLeaseInRange(
      TEST_UBUNTU_AGENT,
      "eth0",
      LAN_POOL_START,
      LAN_POOL_END,
      5,
    );
    // The daemon only materialises a device row once it observes LAN traffic.
    await resolveViaAgent(TEST_UBUNTU_AGENT, "example.com").catch(
      () => undefined,
    );
    // Where packet capture can't reach `wardnet_lan` no row ever appears — an
    // environment limitation the other kernel-state specs also skip on.
    guest = await findDeviceByIpRangeOrNull(
      authed,
      LAN_POOL_START,
      LAN_POOL_END,
      45_000,
    );
    if (!guest) return;
    originalZoneId = guest.zone_id;

    const created = await zones.create({
      name: ZONE_NAME,
      isolation_stance: "isolate_members",
      allowed_targets: ["direct", "tunnel"],
      member_isolation: true,
      admin_ui_reachable: true,
      subnet: { cidr: ZONE_CIDR },
    });
    zoneId = created.zone.id;

    await zones.assignDevice(guest.id, zoneId);

    // Re-lease: the daemon serves a subnetted zone's members from that zone's
    // own scope, so the renew is what moves the guest into 10.44.1.0/24.
    zoneIp = await acquireLeaseInRange(
      TEST_UBUNTU_AGENT,
      "eth0",
      ZONE_RANGE_START,
      ZONE_RANGE_END,
      5,
    );

    // The host route keys on the device row's `last_ip`, and that only moves
    // once the daemon *observes traffic sourced from* the new address.
    //
    // The client image adds the lease as a *secondary* IPv4 and keeps its
    // docker-IPAM address (compose's service DNS depends on it), so traffic
    // aimed at the daemon's LAN address routes out of the docker subnet and
    // sources from the docker address — discovery would re-key onto that one
    // forever. Aiming the lookup at the *zone gateway* instead puts the
    // destination inside the zone subnet, so the kernel picks the zone lease as
    // the source and the daemon finally observes the device at its zone
    // address. Match on the subnet rather than the exact address dhclient
    // reported: what matters is that the device is keyed inside the zone.
    let addrs = "(not read)";
    const rekeyed = await pollUntil(
      async () => {
        await resolveViaAgent(TEST_UBUNTU_AGENT, "example.com", {
          server: ZONE_GATEWAY,
        }).catch(() => undefined);
        // Record what the client actually holds, so a failure says whether the
        // lease landed on the interface at all rather than leaving it to be
        // inferred from the daemon's side.
        addrs = await agentGet<AgentInterfacesResponse>(
          TEST_UBUNTU_AGENT,
          "/interfaces",
        )
          .then((i) =>
            JSON.stringify(
              i.interfaces.find((x) => x.name === "eth0")?.addrs ?? [],
            ),
          )
          .catch((e: unknown) => `(unreadable: ${String(e)})`);
        return (await devices.getById(guest!.id)).device;
      },
      (d) => d.last_ip.startsWith("10.44.1."),
      {
        timeoutMs: 90_000,
        intervalMs: 3_000,
        describe: (last) =>
          `device never re-keyed into ${ZONE_CIDR}: daemon has ` +
          `last_ip=${last?.last_ip}, dhclient reported ${zoneIp}, ` +
          `client eth0 addrs=${addrs}`,
      },
    );
    // Assert against the address the daemon actually keyed on.
    zoneIp = rekeyed.last_ip;
  }, 300_000);

  afterAll(async () => {
    // Member isolation installs cross-subnet drops and moves the client off the
    // base LAN; both would follow every later spec in the shared stack.
    try {
      if (guest && originalZoneId) {
        await zones.assignDevice(guest.id, originalZoneId);
      }
      if (zoneId) await zones.delete(zoneId);
      // Put the client back on the base LAN so it stops holding a zone address.
      await acquireLeaseInRange(
        TEST_UBUNTU_AGENT,
        "eth0",
        LAN_POOL_START,
        LAN_POOL_END,
        5,
      ).catch(() => undefined);
    } catch {
      // Best-effort: cleanup failure must not mask a real assertion failure.
    }
  }, 120_000);

  it("gives a leased member the zone gateway as preferred source", async (ctx) => {
    if (!guest || !zoneIp) return ctx.skip();

    const route = await waitForHostRouteSrc(DAEMON_AGENT, zoneIp, ZONE_GATEWAY);

    expect(route.src).toBe(ZONE_GATEWAY);
    expect(route.dev).toBeTruthy();
    // The compose fixture pins LAN_INTERFACE=eth0 but warns it can land
    // elsewhere, so read the interface the daemon actually used.
    lanIface = route.dev;
  });

  it("repairs a prefsrc-less route left by an older build on restart", async (ctx) => {
    if (!guest || !zoneIp || !lanIface) return ctx.skip();

    // Fabricate the pre-fix state: same /32, no preferred source.
    const cleared = await clearHostRoutePrefsrc(DAEMON_AGENT, zoneIp, lanIface);
    expect(cleared.success, `ip route change failed: ${cleared.stderr}`).toBe(
      true,
    );

    // Confirm the fabrication took — otherwise the assertion below would pass
    // against a route that was never broken.
    await waitForHostRouteSrc(DAEMON_AGENT, zoneIp, undefined, 15_000);

    const before = await daemonPid(DAEMON_AGENT);
    await system.restart();
    await waitForDaemonRestart(DAEMON_AGENT, before.pid);

    // Startup reconcile must re-assert the route. `add_host_route` replaces
    // rather than skipping when one already exists, which is what lets it
    // repair a stale route in place instead of leaving it alone.
    const healed = await waitForHostRouteSrc(
      DAEMON_AGENT,
      zoneIp,
      ZONE_GATEWAY,
    );
    expect(healed.src).toBe(ZONE_GATEWAY);
  }, 180_000);

  it("leaves no host route in the zone subnet without a source hint", async (ctx) => {
    if (!guest || !zoneIp) return ctx.skip();

    // The invariant itself, independent of which path installed the route: any
    // /32 the daemon owns inside the zone subnet must name a source.
    const { routes } = await daemonRoutes(DAEMON_AGENT);
    const orphans = routes.filter(
      (r) =>
        r.dst.startsWith("10.44.1.") &&
        !r.dst.includes("/") &&
        r.dst !== ZONE_GATEWAY &&
        !r.src,
    );
    expect(
      orphans,
      `host routes without a preferred source: ${JSON.stringify(orphans)}`,
    ).toEqual([]);
  });

  it("keeps the member's host route on the LAN interface", async (ctx) => {
    if (!guest || !zoneIp || !lanIface) return ctx.skip();

    const { routes } = await daemonRoutes(DAEMON_AGENT);
    expect(hostRouteFor(routes, zoneIp)?.dev).toBe(lanIface);
  });
});
