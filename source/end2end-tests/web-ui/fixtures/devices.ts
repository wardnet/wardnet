/**
 * Device seeding for the admin-site Devices specs (A6, #621).
 *
 * There is no create-device API — the daemon only materializes a device
 * row when its packet-capture discovery (`device/discovery.rs`
 * `process_observation`) observes LAN traffic for a new MAC. So
 * `seedDiscoveredDevice` makes the `test_debian` test-agent emit an ARP
 * frame (`POST /arp/send`, the same endpoint the daemon arp-failover spec
 * drives) with a synthetic, fully-controlled in-subnet MAC/IP, then polls
 * `/api/devices` until the daemon has discovered it.
 *
 * The routing-change spec also needs a tunnel in the routing selector
 * (the picker is disabled with zero tunnels). `seedTunnel` imports a
 * deterministic WireGuard `.conf` over the API — import only parses and
 * persists the definition (status `Down`, no interface brought up), so it
 * has no WireGuard kernel dependency in the compose stack.
 *
 * Like the other fixtures here, this talks to the daemon over plain
 * `fetch` against the REST API rather than the source-only `@wardnet/js`
 * SDK (see fixtures/seed.ts header for why).
 */

import { api, ensureAdminSetup } from "./seed";
import { TEST_DEBIAN_AGENT } from "./dhcp";
import {
  TUNNEL_CONFIG,
  type ListTunnelsResponse,
} from "./tunnels";

/**
 * Synthetic LAN client we conjure for the Devices specs. The MAC is
 * locally-administered (`02:…`) so it can't collide with `test_debian`'s
 * real NIC, and the IP sits inside the daemon's LAN subnet
 * (`10.91.0.0/24`) so discovery's subnet filter accepts the observation.
 * The daemon stores MACs canonical-lowercase (issue #312).
 */
export const SEED_DEVICE_MAC = "02:e2:e2:00:00:21";
export const SEED_DEVICE_IP = "10.91.0.123";
/** The daemon's LAN gateway IP — the ARP frame's target. */
const SEED_TARGET_IP = "10.91.0.1";

/** Subset of the daemon's Device we read back from `/api/devices`. */
interface SeededDevice {
  id: string;
  mac: string;
  last_ip: string;
}

interface ListDevicesResponse {
  devices: SeededDevice[];
}

/** POST JSON to a test-agent serve URL. Throws on non-2xx. */
async function agentPost(
  baseUrl: string,
  path: string,
  body: unknown,
): Promise<void> {
  const res = await fetch(`${baseUrl}${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    throw new Error(
      `agent POST ${baseUrl}${path} failed: ${res.status} ${await res.text()}`,
    );
  }
}

/** Make `test_debian` emit a small ARP burst announcing a device mac/ip. */
async function emitArp(mac: string, ip: string): Promise<void> {
  await agentPost(TEST_DEBIAN_AGENT, "/arp/send", {
    sender_mac: mac,
    sender_ip: ip,
    target_ip: SEED_TARGET_IP,
    interface: "eth0",
    count: 3,
    interval_ms: 100,
  });
}

/**
 * Conjure a discovered device for an arbitrary controlled mac/ip and return
 * its daemon-assigned id. The general form of `seedDiscoveredDevice` (which
 * pins the shared `SEED_DEVICE_*` identity); the zone specs reuse it to stand
 * up their own Trusted/Guest members. The mac must be locally-administered
 * (`02:…`) and the ip must sit inside the daemon's LAN subnet so discovery's
 * filter accepts the observation. Idempotent and hard-failing on timeout, for
 * the same reasons documented on `seedDiscoveredDevice`.
 */
export async function seedDiscoveredDeviceByMac(
  mac: string,
  ip: string,
  timeoutMs = 60_000,
): Promise<{ id: string; mac: string; ip: string }> {
  const token = await ensureAdminSetup();

  const found = async (): Promise<SeededDevice | undefined> => {
    const { devices } = await api<ListDevicesResponse>("/devices", { token });
    return devices.find((d) => d.mac === mac);
  };

  const existing = await found();
  if (existing) {
    return { id: existing.id, mac: existing.mac, ip: existing.last_ip };
  }

  const deadline = Date.now() + timeoutMs;
  let lastErr: unknown;
  while (Date.now() < deadline) {
    try {
      await emitArp(mac, ip);
      const match = await found();
      if (match) return { id: match.id, mac: match.mac, ip: match.last_ip };
    } catch (err) {
      lastErr = err;
    }
    await new Promise((resolve) => setTimeout(resolve, 3_000));
  }
  throw new Error(
    `device ${mac} (${ip}) was not discovered within ${timeoutMs}ms — ` +
      `packet capture may not be reaching wardnet_lan in this environment${
        lastErr ? `: ${String(lastErr)}` : ""
      }`,
  );
}

/**
 * Conjure a discovered device and return its daemon-assigned id (plus the
 * mac/ip the specs assert on). Emits an ARP frame from the LAN client,
 * then polls `/api/devices`, re-emitting every few seconds (the discovery
 * service batches observations and flushes on an interval, default 30 s),
 * until the device appears. Throws on timeout — a missing device is a
 * hard failure, not a skip: it means packet capture isn't reaching
 * `wardnet_lan` in this environment. Idempotent: a device already present
 * is returned immediately without re-seeding.
 */
export async function seedDiscoveredDevice(
  timeoutMs = 60_000,
): Promise<{ id: string; mac: string; ip: string }> {
  // Match on MAC alone — the device's stable identity. The daemon always
  // records last_ip from the ARP sender (SEED_DEVICE_IP), but keying on the
  // MAC avoids ever returning an unrelated device that happens to share the
  // IP, which the table/detail specs then filter and navigate by.
  return seedDiscoveredDeviceByMac(SEED_DEVICE_MAC, SEED_DEVICE_IP, timeoutMs);
}

/**
 * Ensure a tunnel exists so the device routing selector offers a
 * non-Direct option. Idempotent: returns the existing tunnel's label if
 * one with this label is already configured, otherwise imports the shared
 * deterministic `TUNNEL_CONFIG` (see ./tunnels). Returns the label so the
 * spec can match the selector option by its accessible name.
 */
export async function seedTunnel(
  label = "E2E Test Tunnel",
  countryCode = "de",
): Promise<string> {
  const token = await ensureAdminSetup();

  const { tunnels } = await api<ListTunnelsResponse>("/tunnels", { token });
  if (tunnels.some((t) => t.label === label)) return label;

  await api("/tunnels", {
    method: "POST",
    token,
    body: JSON.stringify({
      label,
      country_code: countryCode,
      config: TUNNEL_CONFIG,
    }),
  });
  return label;
}
