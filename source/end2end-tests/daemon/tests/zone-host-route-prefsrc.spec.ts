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
 * more than borrowing an existing one does.
 *
 * This file must leave NO residue, and cannot lean on running late to get away
 * with any. Vitest orders files by size descending, not alphabetically, so this
 * one runs 4th of 34 — an earlier version of this comment claimed it sorted
 * last and was simply wrong. A member-isolated zone left behind installs
 * cross-subnet deny rules that every later spec then runs against, which is
 * what took `dns-filter-*` and `nordvpn-provider` down with it. Hence: the zone
 * is torn down first in `afterAll`, before any client-side call that might
 * stall, and every step is individually bounded.
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
  agentPost,
  DAEMON_AGENT,
  TEST_UBUNTU_AGENT,
  acquireLeaseInRange,
  clearHostRoutePrefsrc,
  daemonPid,
  daemonRoutes,
  ensureAdminAndLogin,
  findDeviceByIpOrNull,
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

const ZONE_NAME = "e2e-guest-isolated";

// TEMPORARY — DIAGNOSTIC ONLY, MUST BE REVERTED BEFORE MERGE.
//
// Three specs that all drive `test_debian` (dns-filter-device-toggle,
// dns-filter-profile-swap, nordvpn-provider) started failing on this branch.
// They pass on main, so either the daemon change regressed them or this spec
// disturbs the shared stack. The daemon logs show test_debian's device row
// flapping between its DHCP lease and its docker-IPAM address 22 times, with
// 13 dropped DHCPRELEASEs — but they do not show which side caused it.
//
// Skipping *this* spec while keeping every daemon change isolates the
// variable: if the suite goes green, the daemon is sound and the disturbance
// is this spec's DHCP churn; if it stays red, we have a real daemon
// regression to fix before anything ships. Do not merge in this state.
describe.skip("member-isolation host route preferred source (#1198)", () => {
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

    // Find the client's device row by whatever address it currently holds.
    //
    // Deliberately not "give it a lease inside the base pool first": that
    // needs `ensureLeasedAgent`, which rewrites the daemon's *global* DHCP
    // config, and even `acquireLeaseInRange` fails when an earlier spec has
    // left the client somewhere outside the pool — the last run died on
    // `renew_success=true, no in-pool IP yet`. This spec does not care which
    // address the client starts on, only that a device row exists to move
    // into the zone, so read what it actually has and look that up.
    const before = await agentGet<AgentInterfacesResponse>(
      TEST_UBUNTU_AGENT,
      "/interfaces",
    );
    const startingAddrs = (
      before.interfaces.find((i) => i.name === "eth0")?.addrs ?? []
    )
      .filter((a) => a.family === "inet")
      .map((a) => a.local);
    // The daemon only materialises a row once it observes LAN traffic.
    await resolveViaAgent(TEST_UBUNTU_AGENT, "example.com").catch(
      () => undefined,
    );
    // Where packet capture can't reach `wardnet_lan` no row ever appears — an
    // environment limitation the other kernel-state specs also skip on.
    for (const addr of startingAddrs) {
      guest = await findDeviceByIpOrNull(authed, addr, 20_000);
      if (guest) break;
    }
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
    // Three attempts, not five: each is bounded by the DHCP timeout plus a
    // status read, and this hook's whole budget has to cover the re-key poll
    // afterwards. A budget that is only reachable by arithmetic is how this
    // hook burned 300 s and leaked its zone into every later spec.
    zoneIp = await acquireLeaseInRange(
      TEST_UBUNTU_AGENT,
      "eth0",
      ZONE_RANGE_START,
      ZONE_RANGE_END,
      3,
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
    // Order matters. The zone is the only thing that harms other specs — it
    // installs cross-subnet deny rules — so drop it before anything that could
    // stall, and never let one failing step skip the next.
    if (zoneId) {
      // Move the device out first: a zone with members may refuse deletion.
      if (guest && originalZoneId) {
        await zones
          .assignDevice(guest.id, originalZoneId)
          .catch(() => undefined);
      }
      await zones.delete(zoneId).catch(() => undefined);
      zoneId = undefined;
    }
    // Only now the client-side restore, which is best-effort: it renews the
    // client off the zone subnet. If this stalls the damage is confined to
    // this client's address, not the whole suite's rule set.
    await agentPost(TEST_UBUNTU_AGENT, "/dhcp/renew", {
      interface: "eth0",
    }).catch(() => undefined);
  }, 180_000);

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
