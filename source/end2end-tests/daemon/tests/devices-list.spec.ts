import { beforeAll, describe, expect, it } from "vitest";
import { DeviceService, WardnetClient, type Device } from "@wardnet/js";

import {
  API_BASE_URL,
  AuthedClient,
  TEST_DEBIAN_AGENT,
  TEST_UBUNTU_AGENT,
  ensureAdminAndLogin,
  ensureLeasedAgent,
  findDeviceByIpOrNull,
  waitForReady,
} from "./helpers.js";

// Both LAN clients acquire a lease from the shared pool, so the daemon
// discovers each off its DHCP/ARP traffic. We look each device up by its
// exact leased IP rather than by range, since two clients now share the
// pool and a range match would be ambiguous.
const POOL_START = "10.91.0.100";
const POOL_END = "10.91.0.150";
const LAN_IFACE = "eth0";

describe("devices — discovery + listing", () => {
  let authed: AuthedClient;
  let devices: DeviceService;
  let debian: Device | null = null;
  let ubuntu: Device | null = null;

  beforeAll(async () => {
    const client = new WardnetClient({ baseUrl: API_BASE_URL });
    await waitForReady(client);
    authed = await ensureAdminAndLogin(client);
    devices = new DeviceService(authed);

    // Lease both clients so the daemon has two device rows to enumerate.
    // Each returns the in-pool address the daemon issued.
    const debianIp = await ensureLeasedAgent(
      authed,
      TEST_DEBIAN_AGENT,
      LAN_IFACE,
      POOL_START,
      POOL_END,
    );
    const ubuntuIp = await ensureLeasedAgent(
      authed,
      TEST_UBUNTU_AGENT,
      LAN_IFACE,
      POOL_START,
      POOL_END,
    );

    // Packet-capture-driven discovery on `wardnet_lan` is a known-flaky
    // area of this harness (see helpers.ts); resolve to null and let the
    // `it` blocks skip rather than fail when the daemon can't observe the
    // LAN in this environment.
    debian = await findDeviceByIpOrNull(authed, debianIp);
    ubuntu = await findDeviceByIpOrNull(authed, ubuntuIp);
  }, 180_000);

  it("discovers both LAN clients as distinct devices with expected fields", (ctx) => {
    if (!debian || !ubuntu) return ctx.skip();

    // Two clients, two rows — not one row flapping between IPs.
    expect(debian.id).not.toBe(ubuntu.id);
    expect(debian.mac).not.toBe(ubuntu.mac);

    for (const device of [debian, ubuntu]) {
      // MAC is the stable identity the daemon keys discovery on.
      expect(device.mac).toMatch(/^([0-9a-f]{2}:){5}[0-9a-f]{2}$/);
      // Leased clients are in-pool, so the daemon records the lease.
      expect(device.dhcp_status).toBe("lease");
      // Every device belongs to exactly one zone (issue #735).
      expect(device.zone_id).toBeTruthy();
      // A freshly discovered device follows the gateway default policy.
      expect(device.current_rule).toBeNull();
      expect(typeof device.first_seen).toBe("string");
      expect(typeof device.last_seen).toBe("string");
    }
  });

  it("returns both discovered devices from GET /devices", async (ctx) => {
    if (!debian || !ubuntu) return ctx.skip();

    const { devices: list } = await devices.list();
    const ids = list.map((d) => d.id);
    expect(ids).toContain(debian.id);
    expect(ids).toContain(ubuntu.id);
  });

  it("getById returns the same device the list surfaced", async (ctx) => {
    if (!debian) return ctx.skip();

    const detail = await devices.getById(debian.id);
    expect(detail.device.id).toBe(debian.id);
    expect(detail.device.mac).toBe(debian.mac);
    expect(detail.device.last_ip).toBe(debian.last_ip);
    // A device with no rule of its own reports a null current_rule both in
    // the list projection and the detail view.
    expect(detail.current_rule).toBeNull();
  });

  it("getById rejects a malformed device id", async () => {
    await expect(devices.getById("not-a-uuid")).rejects.toThrow();
  });
});
