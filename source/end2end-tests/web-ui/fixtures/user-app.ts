import { type APIRequestContext, request } from "@playwright/test";

import {
  LAN_PROXY_IP,
  UI_LAN_BASE_URL,
  api,
  ensureAdminSetup,
} from "./seed.js";

/**
 * Fixture helpers for the user-app device-flow specs (C2, #627).
 *
 * The user-app is device-keyed: the daemon resolves the self-service
 * endpoints (`/api/devices/me/*`) from the request's source IP. The specs
 * run over the LAN-side TLS proxy (`tls_proxy_lan`), whose fixed IP
 * `LAN_PROXY_IP` the `seed-lan-device` setup seeds as a discovered device
 * (see `lan-device.setup.ts`). Two kinds of helper live here:
 *
 *  - Device-keyed mutations (`setMyCapture`, `setMyRule`) — driven through
 *    the LAN proxy so the daemon classifies them as coming from the seeded
 *    device, exactly as the browser's own requests are. A Playwright
 *    `request` context with `ignoreHTTPSErrors` trusts the self-signed
 *    proxy cert (Node's bare `fetch`, used for the admin calls below, can't).
 *  - Admin mutations (`getLanDeviceId`, `setAdminLocked`) — plain admin REST
 *    calls straight to the daemon (`api`, with a session token), used to set
 *    up the admin-locked routing case the device itself cannot reach.
 *
 * Specs use these to set their own preconditions and restore them after, so
 * behaviour never depends on file order (the user-app project runs
 * single-worker, but each spec stays self-contained regardless).
 */

interface DeviceRow {
  id: string;
  last_ip: string;
}

interface ListDevicesResponse {
  devices: DeviceRow[];
}

/** Routing target as the daemon's `RoutingTarget` serialises (issue #527). */
export type RoutingTarget =
  | { type: "direct" }
  | { type: "tunnel"; tunnel_id: string };

/** Resolve the daemon id of the seeded LAN-proxy device (admin). */
export async function getLanDeviceId(): Promise<string> {
  const token = await ensureAdminSetup();
  const { devices } = await api<ListDevicesResponse>("/devices", { token });
  const device = devices.find((d) => d.last_ip === LAN_PROXY_IP);
  if (!device) {
    throw new Error(
      `LAN-proxy device ${LAN_PROXY_IP} not found — the seed-lan-device ` +
        `project must run before the user-app specs`,
    );
  }
  return device.id;
}

/**
 * Admin: lock or unlock routing changes for a device
 * (`PUT /api/devices/{id}`, `admin_locked`). Locking makes the
 * device-keyed `set_rule_for_ip` return 403, which the user-app renders as
 * the read-only "locked by admin" state.
 */
export async function setAdminLocked(
  deviceId: string,
  locked: boolean,
): Promise<void> {
  const token = await ensureAdminSetup();
  await api(`/devices/${deviceId}`, {
    method: "PUT",
    token,
    body: JSON.stringify({ admin_locked: locked }),
  });
}

/** Run `fn` with a request context that trusts the self-signed LAN proxy. */
async function withLanProxy<T>(
  fn: (ctx: APIRequestContext) => Promise<T>,
): Promise<T> {
  const ctx = await request.newContext({ ignoreHTTPSErrors: true });
  try {
    return await fn(ctx);
  } finally {
    await ctx.dispose();
  }
}

/**
 * Device-keyed: flip the calling device's DNS-capture flag
 * (`PATCH /api/devices/me/dns-capture`). Resolved by the LAN proxy's source
 * IP, so this toggles the seeded device's own `dns_capture_enabled`.
 */
export async function setMyCapture(enabled: boolean): Promise<void> {
  await withLanProxy(async (ctx) => {
    const res = await ctx.patch(
      `${UI_LAN_BASE_URL}/api/devices/me/dns-capture`,
      { data: { enabled } },
    );
    if (!res.ok()) {
      throw new Error(
        `PATCH /devices/me/dns-capture → ${res.status()}: ${await res.text()}`,
      );
    }
  });
}

/**
 * Device-keyed: set the calling device's routing target
 * (`PUT /api/devices/me/rule`). Used to reset routing to Direct between
 * specs; fails with 403 if the device is admin-locked.
 */
export async function setMyRule(target: RoutingTarget): Promise<void> {
  await withLanProxy(async (ctx) => {
    const res = await ctx.put(`${UI_LAN_BASE_URL}/api/devices/me/rule`, {
      data: { target },
    });
    if (!res.ok()) {
      throw new Error(
        `PUT /devices/me/rule → ${res.status()}: ${await res.text()}`,
      );
    }
  });
}
