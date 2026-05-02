import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { DnsService, WardnetClient } from "@wardnet/js";

import {
  API_BASE_URL,
  AuthedClient,
  ensureAdminAndLogin,
  waitForReady,
} from "./helpers.js";

// Cloudflare's public resolver — picked because it's the same default
// the daemon falls back to when no upstreams are configured (see
// build_resolver in wardnetd/src/dns/server.rs), so the round-trip
// here doesn't change runtime behaviour for any spec running after.
const UPSTREAM_ADDRESS = "1.1.1.1";

describe("dns config", () => {
  let authed: AuthedClient;
  let dns: DnsService;
  let initialEnabled: boolean;

  beforeAll(async () => {
    const client = new WardnetClient({ baseUrl: API_BASE_URL });
    await waitForReady(client);
    authed = await ensureAdminAndLogin(client);
    dns = new DnsService(authed);

    initialEnabled = (await dns.getConfig()).config.enabled;
  }, 60_000);

  afterAll(async () => {
    // Leave DNS enabled so downstream specs (dns-resolve, blocklists,
    // ...) can rely on a running server without re-toggling. The
    // dhcp specs follow the same convention.
    try {
      const cfg = (await dns.getConfig()).config;
      if (!cfg.enabled) {
        await dns.toggle({ enabled: true });
      }
    } catch {
      // ignore — best-effort, real failure surfaces from the spec body
    }
    void initialEnabled;
  });

  it("toggles the DNS server on and off", async () => {
    // Disable first so we observe a true off→on transition regardless
    // of which DNS spec ran before this one.
    if ((await dns.getConfig()).config.enabled) {
      await dns.toggle({ enabled: false });
    }

    let off = await dns.status();
    expect(off.enabled).toBe(false);
    expect(off.running).toBe(false);

    const onResp = await dns.toggle({ enabled: true });
    expect(onResp.config.enabled).toBe(true);

    const on = await dns.status();
    expect(on.enabled).toBe(true);
    // The /api/dns/config/toggle handler starts the server inline (see
    // wardnetd-api/src/api/dns.rs) so `running` flips synchronously
    // with the toggle response — no settling delay needed.
    expect(on.running).toBe(true);
    expect(on.cache_capacity).toBe(onResp.config.cache_size);

    const offAgain = await dns.toggle({ enabled: false });
    expect(offAgain.config.enabled).toBe(false);
    const final = await dns.status();
    expect(final.running).toBe(false);
  });

  it("round-trips config updates through getConfig", async () => {
    const before = (await dns.getConfig()).config;

    const updated = await dns.updateConfig({
      cache_size: 5_000,
      cache_ttl_min_secs: 30,
      cache_ttl_max_secs: 3_600,
      upstream_servers: [
        {
          address: UPSTREAM_ADDRESS,
          name: "cloudflare-1",
          protocol: "udp",
        },
      ],
      ad_blocking_enabled: true,
    });
    expect(updated.config.cache_size).toBe(5_000);
    expect(updated.config.cache_ttl_min_secs).toBe(30);
    expect(updated.config.cache_ttl_max_secs).toBe(3_600);
    expect(updated.config.upstream_servers).toEqual([
      {
        address: UPSTREAM_ADDRESS,
        name: "cloudflare-1",
        protocol: "udp",
      },
    ]);
    expect(updated.config.ad_blocking_enabled).toBe(true);

    // Independent re-read confirms persistence (not just the
    // synchronous response shape).
    const refetched = (await dns.getConfig()).config;
    expect(refetched.cache_size).toBe(5_000);
    expect(refetched.upstream_servers).toEqual(updated.config.upstream_servers);

    // Restore the cache_size to the previous value so a later
    // config-sensitive spec (or rerun on the same volume) sees the
    // pre-test capacity. ad_blocking_enabled / TTLs are left at the
    // updated values — they're harmless for downstream specs.
    await dns.updateConfig({ cache_size: before.cache_size });
  });

  it("flushCache returns a count and a message", async () => {
    // Turn DNS on so the server-side cache exists; flushing while
    // disabled is also legal but the call exercises a less interesting
    // path (no live server to ask).
    if (!(await dns.getConfig()).config.enabled) {
      await dns.toggle({ enabled: true });
    }
    const flush = await dns.flushCache();
    expect(typeof flush.entries_cleared).toBe("number");
    expect(flush.entries_cleared).toBeGreaterThanOrEqual(0);
    expect(flush.message.length).toBeGreaterThan(0);

    const after = await dns.status();
    expect(after.cache_size).toBe(0);
  });
});
