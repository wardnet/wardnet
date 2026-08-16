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
 * "no internet" — which is exactly how this surfaced in production.
 *
 * Two things are asserted, both unreachable from a unit test because
 * `rtnetlink` has no mockable boundary:
 *
 * 1. **Contents** — the `/32` the kernel actually stores carries the zone
 *    gateway as its preferred source.
 * 2. **Healing** — a `/32` rewritten *without* one (what older daemons left
 *    behind) is repaired by reconcile on the next restart. That is the upgrade
 *    path: without it an operator upgrading to fix a live outage keeps every
 *    broken route until each device happens to re-DHCP.
 *
 * Deliberately minimal footprint on the shared stack. The zone claims the e2e
 * LAN's own subnet, so its gateway is `10.91.0.1` — an address the daemon
 * already holds, which both keeps the client in-subnet without re-leasing and
 * avoids the kernel's `EINVAL` on a non-local `RTA_PREFSRC`. An earlier version
 * added a dedicated client and drove it through a re-lease into a separate zone
 * subnet; that was more faithful to how a phone joins, but adding a third
 * client shifted docker's IPAM assignments and the release/renew churn broke
 * lease acquisition for the other specs sharing this daemon. No DHCP
 * configuration is touched here, and no container added.
 */

import { afterAll, beforeAll, describe, expect, it } from "vitest";
import {
  NetworkZonesService,
  SystemService,
  WardnetClient,
  type Device,
} from "@wardnet/js";

import {
  API_BASE_URL,
  AuthedClient,
  DAEMON_AGENT,
  clearHostRoutePrefsrc,
  daemonPid,
  daemonRoutes,
  ensureAdminAndLogin,
  findDeviceByIpRangeOrNull,
  hostRouteFor,
  waitForDaemonRestart,
  waitForHostRouteSrc,
  waitForReady,
} from "./helpers.js";

// The e2e LAN itself. The daemon holds .1 here, so a zone claiming this subnet
// has a gateway that is already a local address.
const ZONE_CIDR = "10.91.0.0/24";
const ZONE_GATEWAY = "10.91.0.1";

// Where the other specs' clients hold their leases. We only read a device from
// this range — never re-lease, never touch the DHCP config.
const LEASE_RANGE_START = "10.91.0.100";
const LEASE_RANGE_END = "10.91.0.150";

const ZONE_NAME = "e2e-prefsrc-isolated";

describe("member-isolation host route preferred source (#1198)", () => {
  let authed: AuthedClient;
  let zones: NetworkZonesService;
  let system: SystemService;

  /** Null when the daemon never discovered a LAN client (see below). */
  let device: Device | null = null;
  let zoneId: string | undefined;
  let originalZoneId: string | undefined;
  let deviceIp: string | undefined;
  let lanIface: string | undefined;

  beforeAll(async () => {
    const client = new WardnetClient({ baseUrl: API_BASE_URL });
    await waitForReady(client);
    authed = await ensureAdminAndLogin(client);
    zones = new NetworkZonesService(authed);
    system = new SystemService(authed);

    // Whichever client the earlier specs already drove through DHCP. Where
    // packet capture can't reach `wardnet_lan` no device row ever appears — an
    // environment limitation the other kernel-state specs also skip on.
    device = await findDeviceByIpRangeOrNull(
      authed,
      LEASE_RANGE_START,
      LEASE_RANGE_END,
    );
    if (!device) return;
    deviceIp = device.last_ip;
    originalZoneId = device.zone_id;

    const created = await zones.create({
      name: ZONE_NAME,
      isolation_stance: "isolate_members",
      allowed_targets: ["direct", "tunnel"],
      member_isolation: true,
      // Leave the admin surfaces reachable: the gate would otherwise TCP-reset
      // this device's traffic to the Pi, which is not what we're testing.
      admin_ui_reachable: true,
      subnet: { cidr: ZONE_CIDR },
    });
    zoneId = created.zone.id;

    // Assigning the device is what makes it a member and installs the /32.
    await zones.assignDevice(device.id, zoneId);
  }, 120_000);

  afterAll(async () => {
    // Member isolation adds isolation rules for the whole e2e LAN; put the
    // device and the zone back before handing the stack on.
    try {
      if (device && originalZoneId) {
        await zones.assignDevice(device.id, originalZoneId);
      }
      if (zoneId) await zones.delete(zoneId);
    } catch {
      // Best-effort: cleanup failure must not mask a real assertion failure.
    }
  }, 120_000);

  it("gives a member's /32 the zone gateway as preferred source", async (ctx) => {
    if (!device || !deviceIp) return ctx.skip();

    const route = await waitForHostRouteSrc(
      DAEMON_AGENT,
      deviceIp,
      ZONE_GATEWAY,
    );

    expect(route.src).toBe(ZONE_GATEWAY);
    expect(route.dev).toBeTruthy();
    // The compose fixture pins LAN_INTERFACE=eth0 but warns it can land
    // elsewhere, so read the interface the daemon actually used.
    lanIface = route.dev;
  }, 120_000);

  it("repairs a prefsrc-less route left by an older build on restart", async (ctx) => {
    if (!device || !deviceIp || !lanIface) return ctx.skip();

    // Fabricate the pre-fix state: same /32, no preferred source.
    const cleared = await clearHostRoutePrefsrc(
      DAEMON_AGENT,
      deviceIp,
      lanIface,
    );
    expect(cleared.success, `ip route change failed: ${cleared.stderr}`).toBe(
      true,
    );

    // Confirm the fabrication took — otherwise the assertion below would pass
    // against a route that was never broken.
    await waitForHostRouteSrc(DAEMON_AGENT, deviceIp, undefined, 15_000);

    const before = await daemonPid(DAEMON_AGENT);
    await system.restart();
    await waitForDaemonRestart(DAEMON_AGENT, before.pid);

    // Startup reconcile must re-assert the route. `add_host_route` replaces
    // rather than skipping when one already exists, which is what lets it
    // repair a stale route in place instead of leaving it alone.
    const healed = await waitForHostRouteSrc(
      DAEMON_AGENT,
      deviceIp,
      ZONE_GATEWAY,
    );
    expect(healed.src).toBe(ZONE_GATEWAY);
  }, 180_000);

  it("leaves no member host route without a source hint", async (ctx) => {
    if (!device || !deviceIp) return ctx.skip();

    // The invariant itself, independent of which code path installed the
    // route: a /32 the daemon owns for a member must name a source.
    const { routes } = await daemonRoutes(DAEMON_AGENT);
    const route = hostRouteFor(routes, deviceIp);
    expect(route, `no /32 host route for ${deviceIp}`).toBeDefined();
    expect(route?.src, `host route without a preferred source: ${route?.dst}`)
      .toBeDefined();
  });

  it("keeps the member's host route on the LAN interface", async (ctx) => {
    if (!device || !deviceIp || !lanIface) return ctx.skip();

    const { routes } = await daemonRoutes(DAEMON_AGENT);
    expect(hostRouteFor(routes, deviceIp)?.dev).toBe(lanIface);
  });
});
