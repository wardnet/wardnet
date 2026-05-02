import { beforeAll, describe, expect, it } from "vitest";
import { DnsService, WardnetClient } from "@wardnet/js";

import {
  API_BASE_URL,
  AuthedClient,
  TEST_DEBIAN_AGENT,
  ensureAdminAndLogin,
  resolveViaAgent,
  waitForReady,
} from "./helpers.js";

// example.org is RFC 2606 reserved-for-documentation and resolves to
// stable public addresses. Pinned here (vs example.com used in the
// allowlist spec) so cache-state assertions stay isolated from the
// allowlist spec's blocklist toggling.
const RESOLVE_TARGET = "example.org";

describe("dns resolve", () => {
  let authed: AuthedClient;
  let dns: DnsService;

  beforeAll(async () => {
    const client = new WardnetClient({ baseUrl: API_BASE_URL });
    await waitForReady(client);
    authed = await ensureAdminAndLogin(client);
    dns = new DnsService(authed);

    if (!(await dns.getConfig()).config.enabled) {
      await dns.toggle({ enabled: true });
    }

    // Start with an empty cache so the first resolve is a guaranteed
    // miss. flushCache also resets the hit/miss counters that drive
    // cache_hit_rate.
    await dns.flushCache();
  }, 60_000);

  it("forwards a query and caches the answer", async () => {
    const first = await resolveViaAgent(TEST_DEBIAN_AGENT, RESOLVE_TARGET);
    expect(
      first.addrs.length,
      `expected at least one A record for ${RESOLVE_TARGET}; got ${JSON.stringify(first)}`,
    ).toBeGreaterThan(0);
    // Sanity check the answer parses as IPv4 — guards against the
    // probe accidentally returning dig's diagnostic banner instead of
    // the +short answer body.
    expect(first.addrs[0]).toMatch(/^\d{1,3}(\.\d{1,3}){3}$/);

    const afterFirst = await dns.status();
    expect(
      afterFirst.cache_size,
      "first query should populate one cache entry",
    ).toBeGreaterThan(0);

    // Same name, same record type — has to come back from the cache
    // (DnsCache.get bumps hits) provided the entry hasn't expired.
    const second = await resolveViaAgent(TEST_DEBIAN_AGENT, RESOLVE_TARGET);
    expect(second.addrs).toEqual(first.addrs);

    const afterSecond = await dns.status();
    expect(
      afterSecond.cache_hit_rate,
      "second query should land a cache hit",
    ).toBeGreaterThan(0);
  });
});
