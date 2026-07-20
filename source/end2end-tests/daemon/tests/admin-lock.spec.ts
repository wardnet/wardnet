import { afterAll, beforeAll, describe, expect, it } from "vitest";
import {
  DeviceService,
  WardnetClient,
  type Device,
  type DeviceMeResponse,
} from "@wardnet/js";

import {
  API_BASE_URL,
  AuthedClient,
  TEST_DEBIAN_AGENT,
  ensureAdminAndLogin,
  ensureLeasedAgent,
  findDeviceByIpOrNull,
  proxyToDaemon,
  waitForReady,
} from "./helpers.js";

// Admin-lock enforcement: once an admin locks a device, that device can no
// longer change its own routing rule. The daemon enforces this on the
// self-service path (`PUT /api/devices/me/rule`), which we drive through
// the client agent's proxy so the request is classified as the device.
const POOL_START = "10.91.0.100";
const POOL_END = "10.91.0.150";
const LAN_IFACE = "eth0";

describe("devices — admin-lock enforcement", () => {
  let authed: AuthedClient;
  let devices: DeviceService;
  let device: Device | null = null;
  let leasedIp = "";

  beforeAll(async () => {
    const client = new WardnetClient({ baseUrl: API_BASE_URL });
    await waitForReady(client);
    authed = await ensureAdminAndLogin(client);
    devices = new DeviceService(authed);

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
    // Leave the device unlocked and direct so later specs (vitest may
    // reorder under singleFork) don't inherit a locked device.
    if (device) {
      try {
        await devices.update(device.id, {
          admin_locked: false,
          routing_target: { type: "direct" },
        });
      } catch {
        // ignore
      }
    }
  });

  it("locks a device, blocks its self-service rule change with 403, then lifts the lock", async (ctx) => {
    if (!device) return ctx.skip();

    // Baseline: an unlocked device can set its own rule.
    const unlocked = await proxyToDaemon(TEST_DEBIAN_AGENT, {
      method: "PUT",
      path: "/api/devices/me/rule",
      sourceIp: leasedIp,
      body: { target: { type: "direct" } },
    });
    expect(unlocked.status).toBe(200);

    // Admin locks the device.
    await devices.update(device.id, { admin_locked: true });

    // The lock is reflected in the device's own self-service view.
    const me = await proxyToDaemon<DeviceMeResponse>(TEST_DEBIAN_AGENT, {
      path: "/api/devices/me",
      sourceIp: leasedIp,
    });
    expect(me.status).toBe(200);
    expect(me.body.admin_locked).toBe(true);

    // The device can no longer change its own routing rule — 403.
    const locked = await proxyToDaemon(TEST_DEBIAN_AGENT, {
      method: "PUT",
      path: "/api/devices/me/rule",
      sourceIp: leasedIp,
      body: { target: { type: "direct" } },
    });
    expect(locked.status).toBe(403);

    // Admin lifts the lock → the device regains control of its own rule.
    await devices.update(device.id, { admin_locked: false });
    const relocked = await proxyToDaemon(TEST_DEBIAN_AGENT, {
      method: "PUT",
      path: "/api/devices/me/rule",
      sourceIp: leasedIp,
      body: { target: { type: "direct" } },
    });
    expect(relocked.status).toBe(200);
  });
});
