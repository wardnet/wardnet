/**
 * Network-zone seeding helpers for the zone coverage specs (epic #244 → #740).
 *
 * The daemon seeds three system zones at migration — Trusted (the `is_default`
 * "home" anchor), IoT, and Guest (the `is_default_for_new` landing zone). These
 * helpers layer test state on top over plain `fetch` against the REST API, the
 * same path the other fixtures use (see seed.ts for why this harness avoids the
 * source-only `@wardnet/js` SDK):
 *
 *  - `seedZoneDevices` stands up two discovered LAN clients and pins one to
 *    Trusted and one to Guest, so every zone surface has a real member to show,
 *    reassign, and approve (the issue's "one device in Trusted, a second in
 *    Guest" seed). Because zone membership is otherwise sticky (set once at
 *    discovery), it re-pins on every call — so a spec that reassigns a device
 *    can rely on the next `beforeAll` restoring the canonical layout.
 *  - The zone/exception lookup + cleanup helpers keep the UI-create specs
 *    deterministic across a re-run against a persisted state volume.
 *
 * The admin lookups here hit the daemon directly (`api` + a session token), so
 * they work from either runner container — including the LAN-side runner that
 * drives the device-keyed user-app, whose own `/devices/me` device is resolved
 * separately (see user-app.ts / lan-device.setup.ts).
 */

import { api, ensureAdminSetup } from "./seed";
import { seedDiscoveredDeviceByMac } from "./devices";

/** Seeded system-zone names (migration `20260701000000_network_zones.sql`). */
export const TRUSTED_ZONE = "Trusted";
export const GUEST_ZONE = "Guest";
export const IOT_ZONE = "IoT";

/**
 * Manual zone the admin-site CRUD spec creates through the UI, then renames.
 * The renamed value is deliberately NOT a superstring of the original so a
 * substring row filter for one never matches the other.
 */
export const TEST_ZONE_NAME = "e2e-net-zone";
export const TEST_ZONE_NAME_EDITED = "e2e-zone-renamed";

/**
 * Two synthetic LAN clients the zone specs pin to Trusted / Guest. The MACs are
 * locally-administered (`02:…`) so they can't collide with `test_debian`'s real
 * NIC or the shared `SEED_DEVICE` (`…:00:00:21`), and the IPs sit inside the
 * daemon's LAN subnet (`10.91.0.0/24`) so discovery accepts them.
 */
export const TRUSTED_DEVICE_MAC = "02:e2:e2:00:00:31";
export const TRUSTED_DEVICE_IP = "10.91.0.131";
export const GUEST_DEVICE_MAC = "02:e2:e2:00:00:32";
export const GUEST_DEVICE_IP = "10.91.0.132";

interface Zone {
  id: string;
  name: string;
  provenance: "system" | "manual";
  is_default: boolean;
  is_default_for_new: boolean;
  member_count: number;
}

interface ExceptionEndpoint {
  kind: "zone" | "device";
  id: string;
}
interface ZoneException {
  id: string;
  from: ExceptionEndpoint;
  to: ExceptionEndpoint;
}

export interface SeededZoneDevice {
  id: string;
  mac: string;
  ip: string;
}

/**
 * List the network zones (with member counts). Callers already holding a
 * session token may pass it to avoid a redundant `ensureAdminSetup` login.
 */
export async function listZones(token?: string): Promise<Zone[]> {
  const session = token ?? (await ensureAdminSetup());
  const { zones } = await api<{ zones: Zone[] }>("/network/zones", {
    token: session,
  });
  return zones;
}

/** Resolve a zone by name, or throw if it doesn't exist. */
export async function getZoneByName(name: string, token?: string): Promise<Zone> {
  const zone = (await listZones(token)).find((z) => z.name === name);
  if (!zone) throw new Error(`network zone "${name}" not found`);
  return zone;
}

/** Resolve a zone id by name. */
export async function getZoneIdByName(
  name: string,
  token?: string,
): Promise<string> {
  return (await getZoneByName(name, token)).id;
}

/** Assign a device to the named zone (`PUT /devices/{id}/zone`). */
export async function assignDeviceToZone(
  deviceId: string,
  zoneName: string,
): Promise<void> {
  const token = await ensureAdminSetup();
  const zoneId = await getZoneIdByName(zoneName, token);
  await api(`/devices/${deviceId}/zone`, {
    method: "PUT",
    token,
    body: JSON.stringify({ zone_id: zoneId }),
  });
}

/**
 * Seed the two zone members and pin them to Trusted / Guest. Idempotent: a
 * device already discovered is reused, and both are re-assigned to their
 * canonical zone every call, so a spec that moved one mid-test doesn't leak
 * into the next. Returns both so specs can filter rows and navigate by id.
 */
export async function seedZoneDevices(): Promise<{
  trusted: SeededZoneDevice;
  guest: SeededZoneDevice;
}> {
  // Discover both in parallel: emitting both ARP bursts before polling lets a
  // single discovery flush materialise both, so this stays inside the same time
  // budget as the one-device `seedDiscoveredDevice` (matters in `beforeAll`).
  const [trusted, guest] = await Promise.all([
    seedDiscoveredDeviceByMac(TRUSTED_DEVICE_MAC, TRUSTED_DEVICE_IP),
    seedDiscoveredDeviceByMac(GUEST_DEVICE_MAC, GUEST_DEVICE_IP),
  ]);
  await assignDeviceToZone(trusted.id, TRUSTED_ZONE);
  await assignDeviceToZone(guest.id, GUEST_ZONE);
  return { trusted, guest };
}

/** Read the new-device quarantine (notification) toggle. */
export async function getQuarantine(): Promise<boolean> {
  const token = await ensureAdminSetup();
  const { enabled } = await api<{ enabled: boolean }>(
    "/network/quarantine-new-devices",
    { token },
  );
  return enabled;
}

/**
 * Delete any leftover manual zone the CRUD spec owns (either the created or the
 * renamed name), so its UI-create step starts clean on a re-run. System zones
 * are never touched; a zone with members can't be deleted, but the test zones
 * never gain any.
 */
export async function cleanupZones(): Promise<void> {
  const token = await ensureAdminSetup();
  const zones = await listZones();
  for (const z of zones) {
    if (
      z.provenance !== "system" &&
      (z.name === TEST_ZONE_NAME || z.name === TEST_ZONE_NAME_EDITED)
    ) {
      await api(`/network/zones/${z.id}`, { method: "DELETE", token });
    }
  }
}

/**
 * Delete any cross-zone exception between the two named zones (either
 * direction), so the exceptions-manager spec's UI-create step is deterministic.
 */
export async function cleanupZoneExceptions(
  fromZone: string,
  toZone: string,
): Promise<void> {
  const token = await ensureAdminSetup();
  const [a, b] = await Promise.all([
    getZoneIdByName(fromZone, token),
    getZoneIdByName(toZone, token),
  ]);
  const { exceptions } = await api<{ exceptions: ZoneException[] }>(
    "/network/zones/exceptions",
    { token },
  );
  const ids = new Set([a, b]);
  for (const ex of exceptions) {
    if (
      ex.from.kind === "zone" &&
      ex.to.kind === "zone" &&
      ids.has(ex.from.id) &&
      ids.has(ex.to.id)
    ) {
      await api(`/network/zones/exceptions/${ex.id}`, {
        method: "DELETE",
        token,
      });
    }
  }
}
