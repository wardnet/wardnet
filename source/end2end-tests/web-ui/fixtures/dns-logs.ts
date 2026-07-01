/**
 * DNS query-log seeding for the admin-site query-log spec (A5, #620).
 *
 * The query log only records queries the daemon actually resolves, so both the
 * live-tail and history coverage are seeded by driving *real* DNS queries
 * through the `test_debian` LAN client's `/dns/resolve` probe — the same probe
 * the daemon `dns-resolve` spec uses — pointed at the daemon's own LAN IP
 * (10.91.0.1). Querying the daemon (rather than the client's system resolver)
 * is what makes it log the request as a client query.
 *
 * `enableDnsAndQueryLog` guarantees the server + query logging are on first.
 * Like the other fixtures, this talks to the daemon over plain `fetch` against
 * the REST API rather than the source-only `@wardnet/js` SDK (see seed.ts for
 * why).
 */

import { api, ensureAdminSetup } from "./seed";
import { TEST_DEBIAN_AGENT } from "./dhcp";

/**
 * The daemon's LAN IP — the DNS server the agent probe queries, so the
 * wardnetd-ui daemon (not the client container's system resolver) is the one
 * that resolves and therefore logs the query.
 */
const DAEMON_LAN_IP = "10.91.0.1";

interface DnsConfig {
  enabled: boolean;
  query_log_enabled: boolean;
}

/**
 * Ensure the DNS server and query logging are both on so driven resolves get
 * persisted. Idempotent: reads the current config and only writes when a flag
 * is off. Server enable goes through the dedicated toggle endpoint; query
 * logging is a plain config field (`PUT /dns/config` accepts partial updates).
 */
export async function enableDnsAndQueryLog(): Promise<void> {
  const token = await ensureAdminSetup();
  const { config } = await api<{ config: DnsConfig }>("/dns/config", { token });
  if (!config.enabled) {
    await api("/dns/config/toggle", {
      method: "POST",
      token,
      body: JSON.stringify({ enabled: true }),
    });
  }
  if (!config.query_log_enabled) {
    await api("/dns/config", {
      method: "PUT",
      token,
      body: JSON.stringify({ query_log_enabled: true }),
    });
  }
}

/**
 * Drive a single DNS query for `name` from the `test_debian` LAN client against
 * the daemon (10.91.0.1). Resolves regardless of the answer — an NXDOMAIN or
 * upstream error is still logged — so callers can assert the domain shows up in
 * the query log. Throws only if the agent probe itself is unreachable.
 */
export async function resolveViaAgent(name: string): Promise<void> {
  const params = `name=${encodeURIComponent(name)}&server=${DAEMON_LAN_IP}`;
  const res = await fetch(`${TEST_DEBIAN_AGENT}/dns/resolve?${params}`);
  if (!res.ok) {
    throw new Error(
      `agent GET /dns/resolve?${params} failed: ${res.status} ${await res.text()}`,
    );
  }
}

interface QueryLogEntry {
  domain: string;
}

/**
 * Poll `/dns/log` until an entry whose domain contains `needle` is persisted,
 * so the history assertions don't race the daemon's write path. Throws on
 * timeout — a missing entry means the driven resolve wasn't logged, which is a
 * real failure, not a skip.
 */
export async function waitForQueryLog(
  needle: string,
  timeoutMs = 15_000,
): Promise<void> {
  const token = await ensureAdminSetup();
  const deadline = Date.now() + timeoutMs;
  const path = `/dns/log?domain=${encodeURIComponent(needle)}&limit=50`;
  let lastErr: unknown;
  while (Date.now() < deadline) {
    try {
      const { entries } = await api<{ entries: QueryLogEntry[] }>(path, {
        token,
      });
      if (entries.some((e) => e.domain.includes(needle))) return;
    } catch (err) {
      lastErr = err;
    }
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  throw new Error(
    `query log did not record a domain containing "${needle}" within ${timeoutMs}ms${
      lastErr ? `: ${String(lastErr)}` : ""
    }`,
  );
}
