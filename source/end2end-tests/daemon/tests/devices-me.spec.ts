import { beforeAll, describe, expect, it } from "vitest";
import { WardnetClient, type Device, type DeviceMeResponse } from "@wardnet/js";

import {
  API_BASE_URL,
  AuthedClient,
  TEST_DEBIAN_AGENT,
  TEST_UBUNTU_AGENT,
  ensureAdminAndLogin,
  ensureLeasedAgent,
  findDeviceByIpOrNull,
  proxyToDaemon,
  waitForReady,
} from "./helpers.js";

// `/api/devices/me` is self-service: the daemon identifies the caller by
// the source IP of the TCP connection (no forwarded headers trusted). The
// test runner sits on the management network, so it can't drive these
// flows directly — each LAN client forwards the request to the daemon via
// its agent's `POST /proxy`, binding its own leased IP as the source.
const POOL_START = "10.91.0.100";
const POOL_END = "10.91.0.150";
const LAN_IFACE = "eth0";

describe("devices — /devices/me source-IP classification", () => {
  let authed: AuthedClient;
  let debianIp = "";
  let ubuntuIp = "";
  let debian: Device | null = null;
  let ubuntu: Device | null = null;

  beforeAll(async () => {
    const client = new WardnetClient({ baseUrl: API_BASE_URL });
    await waitForReady(client);
    authed = await ensureAdminAndLogin(client);

    debianIp = await ensureLeasedAgent(
      authed,
      TEST_DEBIAN_AGENT,
      LAN_IFACE,
      POOL_START,
      POOL_END,
    );
    ubuntuIp = await ensureLeasedAgent(
      authed,
      TEST_UBUNTU_AGENT,
      LAN_IFACE,
      POOL_START,
      POOL_END,
    );

    debian = await findDeviceByIpOrNull(authed, debianIp);
    ubuntu = await findDeviceByIpOrNull(authed, ubuntuIp);
  }, 180_000);

  it("classifies each client as its own device by source IP", async (ctx) => {
    if (!debian || !ubuntu) return ctx.skip();

    const debianMe = await proxyToDaemon<DeviceMeResponse>(TEST_DEBIAN_AGENT, {
      path: "/api/devices/me",
      sourceIp: debianIp,
    });
    expect(debianMe.status).toBe(200);
    expect(debianMe.body.device?.id).toBe(debian.id);
    expect(debianMe.body.device?.last_ip).toBe(debianIp);

    const ubuntuMe = await proxyToDaemon<DeviceMeResponse>(TEST_UBUNTU_AGENT, {
      path: "/api/devices/me",
      sourceIp: ubuntuIp,
    });
    expect(ubuntuMe.status).toBe(200);
    expect(ubuntuMe.body.device?.id).toBe(ubuntu.id);
    expect(ubuntuMe.body.device?.last_ip).toBe(ubuntuIp);

    // The whole point of source-IP classification: two callers on the same
    // LAN resolve to two different devices, keyed purely on who connected.
    expect(debianMe.body.device?.id).not.toBe(ubuntuMe.body.device?.id);
  });

  it("reports the caller's admin-lock state and available tunnels shape", async (ctx) => {
    if (!debian) return ctx.skip();

    const res = await proxyToDaemon<DeviceMeResponse>(TEST_DEBIAN_AGENT, {
      path: "/api/devices/me",
      sourceIp: debianIp,
    });
    expect(res.status).toBe(200);
    // admin_locked defaults false for a freshly discovered device; the
    // admin-lock spec drives the true path. available_tunnels is always an
    // array (empty until a tunnel is provisioned).
    expect(res.body.admin_locked).toBe(false);
    expect(Array.isArray(res.body.available_tunnels)).toBe(true);
  });
});
