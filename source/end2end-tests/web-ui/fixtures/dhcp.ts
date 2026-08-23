/**
 * DHCP seeding for the admin-site DHCP specs (A3, #618).
 *
 * Leases can't be created over the API — they only arise from real DHCP
 * traffic. So `seedDhcpLease` enables the daemon's DHCP server, narrows
 * the pool, then drives the `test_debian` LAN client (running
 * `wardnet-test-agent client serve`) through `dhclient` until it holds
 * an in-pool address. The agent + interface helpers are ported from the
 * daemon Vitest suite (`../../daemon/tests/helpers.ts`) — the web-ui
 * harness can't import that package, so the small surface we need is
 * duplicated here.
 */

import { api, ensureAdminSetup } from "./seed";

/** The LAN-client agent's serve URL (compose service name on wardnet_lan). */
export const TEST_DEBIAN_AGENT =
  // biome-ignore lint/correctness/noProcessGlobal: test harness, run by Node — never shipped to a browser
  process.env.WARDNET_TEST_DEBIAN_AGENT ?? "http://test_debian:3001";

/**
 * DHCP pool the spec narrows to. Both bounds sit above wardnet_lan's
 * docker IPAM range (.2-.15), so any leased `.100-.150` address can
 * only have come from the daemon's DHCP server, not docker.
 */
export const DHCP_POOL_START = "10.91.0.100";
export const DHCP_POOL_END = "10.91.0.150";

/** Subset of the daemon's DhcpConfig we read back before re-saving. */
interface DhcpConfig {
  enabled: boolean;
  subnet_mask: string;
  upstream_dns: string[];
  lease_duration_secs: number;
  router_ip: string | null;
}

/** Shape returned by `wardnet-test-agent client serve`'s /interfaces. */
interface AgentInterface {
  name: string;
  addrs: Array<{ family: string; local: string; prefixlen: number }>;
}

interface AgentInterfacesResponse {
  interfaces: AgentInterface[];
}

interface AgentDhcpRenewResponse {
  renew_success: boolean;
  stderr: string;
}

/** GET against a test-agent serve URL. Throws on non-2xx. */
async function agentGet<T>(baseUrl: string, path: string): Promise<T> {
  const res = await fetch(`${baseUrl}${path}`);
  if (!res.ok) {
    throw new Error(
      `agent GET ${baseUrl}${path} failed: ${res.status} ${await res.text()}`,
    );
  }
  return (await res.json()) as T;
}

/** POST JSON to a test-agent serve URL. Throws on non-2xx. */
async function agentPost<T>(
  baseUrl: string,
  path: string,
  body: unknown,
): Promise<T> {
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
  return (await res.json()) as T;
}

/** Dotted-quad IPv4 → 32-bit int. Multiply-and-add stays positive where
 *  bitwise ops would coerce to signed 32-bit. Mirrors `ipv4ToInt` in
 *  `@wardnet/js`, re-implemented here because this harness deliberately
 *  avoids importing the SDK (see the seed.ts header for why). */
export function ipToInt(ip: string): number {
  return ip.split(".").reduce((acc, n) => acc * 256 + Number(n), 0);
}

/**
 * The IPv4 on `name` whose value sits within `[start, end]`, or
 * undefined. The client keeps both its docker-IPAM address and the
 * daemon-issued lease on the interface at once, so a "first inet wins"
 * pick can return the wrong one.
 */
function ipv4InRange(
  ifaces: AgentInterfacesResponse,
  name: string,
  startInclusive: string,
  endInclusive: string,
): string | undefined {
  const lo = ipToInt(startInclusive);
  const hi = ipToInt(endInclusive);
  return ifaces.interfaces
    .find((i) => i.name === name)
    ?.addrs.filter((a) => a.family === "inet")
    .map((a) => a.local)
    .find((ip) => {
      const v = ipToInt(ip);
      return v >= lo && v <= hi;
    });
}

/**
 * Drives the agent's /dhcp/renew until an address in `[poolStart,
 * poolEnd]` lands on `iface`, or fails after `attempts`. Retries because
 * dhclient races the daemon's DHCP-runner spawn after the enable toggle
 * on a cold stack.
 */
export async function acquireLeaseInRange(
  agent: string,
  iface: string,
  poolStart: string,
  poolEnd: string,
  attempts = 5,
): Promise<string> {
  let lastErr: unknown;
  for (let i = 0; i < attempts; i++) {
    try {
      const renew = await agentPost<AgentDhcpRenewResponse>(
        agent,
        "/dhcp/renew",
        { interface: iface },
      );
      if (renew.renew_success) {
        const ifaces = await agentGet<AgentInterfacesResponse>(
          agent,
          `/interfaces?name=${iface}`,
        );
        const ip = ipv4InRange(ifaces, iface, poolStart, poolEnd);
        if (ip) return ip;
      }
      lastErr = new Error(
        `attempt ${i + 1}: renew_success=${renew.renew_success}, no in-pool IP yet — stderr: ${renew.stderr}`,
      );
    } catch (err) {
      lastErr = err;
    }
    await new Promise((resolve) => setTimeout(resolve, 1_500));
  }
  throw new Error(
    `could not acquire lease in ${poolStart}-${poolEnd} on ${agent}/${iface}: ${String(lastErr)}`,
  );
}

/**
 * Make a real DHCP lease exist so the admin-site leases table is
 * populated. Enables the daemon DHCP server, narrows the pool above
 * docker's IPAM block, then drives the test_debian client through
 * dhclient until it holds an in-pool address. Returns the leased IP.
 * Idempotent: a client already holding an in-pool lease just renews.
 */
export async function seedDhcpLease(): Promise<string> {
  const token = await ensureAdminSetup();

  const { config } = await api<{ config: DhcpConfig }>("/dhcp/config", {
    token,
  });
  if (!config.enabled) {
    await api("/dhcp/config/toggle", {
      method: "POST",
      token,
      body: JSON.stringify({ enabled: true }),
    });
  }
  // updateConfig is idempotent — re-setting to the same values is safe.
  await api("/dhcp/config", {
    method: "PUT",
    token,
    body: JSON.stringify({
      pool_start: DHCP_POOL_START,
      pool_end: DHCP_POOL_END,
      subnet_mask: config.subnet_mask,
      upstream_dns: config.upstream_dns,
      lease_duration_secs: config.lease_duration_secs,
      ...(config.router_ip ? { router_ip: config.router_ip } : {}),
    }),
  });

  return acquireLeaseInRange(
    TEST_DEBIAN_AGENT,
    "eth0",
    DHCP_POOL_START,
    DHCP_POOL_END,
  );
}

/**
 * Create a static reservation via the API so the admin-site DHCP table
 * is non-empty (the table renders a placeholder with no toolbar — and
 * thus no "Add reservation" button — when it has zero entries). Needs
 * no LAN client, so it's a cheap, order-independent precondition.
 * Idempotent: a 409 (reservation already exists) is treated as success.
 */
export async function seedReservation(
  macAddress: string,
  ipAddress: string,
): Promise<void> {
  const token = await ensureAdminSetup();
  try {
    await api("/dhcp/reservations", {
      method: "POST",
      token,
      body: JSON.stringify({ mac_address: macAddress, ip_address: ipAddress }),
    });
  } catch (err) {
    // A repeat run hits "reservation already exists" (409) — fine.
    if (!String(err).includes("409")) throw err;
  }
}
