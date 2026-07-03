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
      // Reset to forwarding unconditionally: the resolution_mode
      // round-trip below flips to recursive, and if its assertions throw
      // before the inline restore runs, the shared daemon would be left
      // recursing from the root for every downstream spec.
      if (cfg.resolution_mode !== "forwarding") {
        await dns.updateConfig({ resolution_mode: "forwarding" });
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
      dns_filtering_enabled: true,
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
    expect(updated.config.dns_filtering_enabled).toBe(true);

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

  it("round-trips Stage 4 security settings (DNSSEC, rebinding, rate limit, DoT)", async () => {
    const before = (await dns.getConfig()).config;

    const updated = await dns.updateConfig({
      dnssec_enabled: true,
      rebinding_protection: false,
      rate_limit_per_second: 50,
      upstream_servers: [
        {
          address: UPSTREAM_ADDRESS,
          name: "cloudflare-dot",
          protocol: "tls",
          tls_server_name: "cloudflare-dns.com",
        },
      ],
    });
    expect(updated.config.dnssec_enabled).toBe(true);
    expect(updated.config.rebinding_protection).toBe(false);
    expect(updated.config.rate_limit_per_second).toBe(50);
    expect(updated.config.upstream_servers[0]?.tls_server_name).toBe(
      "cloudflare-dns.com",
    );

    const refetched = (await dns.getConfig()).config;
    expect(refetched.dnssec_enabled).toBe(true);
    expect(refetched.upstream_servers[0]?.protocol).toBe("tls");

    // Restore safe defaults + UDP upstream so downstream DNS specs
    // (dns-resolve, blocklists, ...) see plain, unthrottled resolution.
    await dns.updateConfig({
      dnssec_enabled: false,
      rebinding_protection: before.rebinding_protection,
      rate_limit_per_second: 0,
      upstream_servers: [
        { address: UPSTREAM_ADDRESS, name: "cloudflare-1", protocol: "udp" },
      ],
    });
  });

  it("round-trips Stage 5 resolution_mode (forwarding ↔ recursive)", async () => {
    // Default is forwarding; flip to recursive and confirm persistence,
    // then restore to forwarding so downstream DNS specs (dns-resolve,
    // blocklists, ...) keep resolving via the configured upstream rather
    // than recursing from the root servers.
    const updated = await dns.updateConfig({ resolution_mode: "recursive" });
    expect(updated.config.resolution_mode).toBe("recursive");

    const refetched = (await dns.getConfig()).config;
    expect(refetched.resolution_mode).toBe("recursive");

    const restored = await dns.updateConfig({ resolution_mode: "forwarding" });
    expect(restored.config.resolution_mode).toBe("forwarding");
  });

  it("rejects a DoT/DoH upstream without a TLS server name", async () => {
    let status: number | undefined;
    try {
      await dns.updateConfig({
        upstream_servers: [
          { address: UPSTREAM_ADDRESS, name: "bad-dot", protocol: "tls" },
        ],
      });
    } catch (e) {
      status = (e as { status?: number }).status;
    }
    expect(status).toBe(400);
  });

  it("round-trips forwarder routing modes and guards single-server selection", async () => {
    // Known two-server pool to select within.
    await dns.updateConfig({
      resolution_mode: "forwarding",
      upstream_servers: [
        { address: "1.1.1.1", name: "Cloudflare", protocol: "udp" },
        { address: "8.8.8.8", name: "Google", protocol: "udp" },
      ],
    });

    // Fastest mode round-trips.
    const fastest = await dns.updateConfig({
      forwarder_selection_mode: "fastest",
    });
    expect(fastest.config.forwarder_selection_mode).toBe("fastest");
    expect(fastest.config.single_upstream ?? null).toBeNull();

    // Single-server mode pinned to Cloudflare.
    const single = await dns.updateConfig({
      forwarder_selection_mode: "single",
      single_upstream: "1.1.1.1",
    });
    expect(single.config.forwarder_selection_mode).toBe("single");
    expect(single.config.single_upstream).toBe("1.1.1.1");
    expect((await dns.getConfig()).config.single_upstream).toBe("1.1.1.1");

    // Selecting an address that isn't in the list is rejected.
    let badSelectStatus: number | undefined;
    try {
      await dns.updateConfig({
        forwarder_selection_mode: "single",
        single_upstream: "9.9.9.9",
      });
    } catch (e) {
      badSelectStatus = (e as { status?: number }).status;
    }
    expect(badSelectStatus).toBe(400);

    // Removing the currently-selected server (while still single) is rejected.
    let removeStatus: number | undefined;
    try {
      await dns.updateConfig({
        upstream_servers: [{ address: "8.8.8.8", name: "Google", protocol: "udp" }],
      });
    } catch (e) {
      removeStatus = (e as { status?: number }).status;
    }
    expect(removeStatus).toBe(400);

    // The rejection must not have mutated persisted state (validation runs
    // before any write): the selected server is still in the pool and chosen.
    const afterReject = (await dns.getConfig()).config;
    expect(afterReject.upstream_servers.map((s) => s.address)).toContain(
      "1.1.1.1",
    );
    expect(afterReject.forwarder_selection_mode).toBe("single");
    expect(afterReject.single_upstream).toBe("1.1.1.1");

    // status exposes per-upstream latency (one entry per address; values
    // may still be null on the very first tick).
    const status = await dns.status();
    expect(Array.isArray(status.upstream_latencies)).toBe(true);

    // Back to failover (the default) clears the selection so downstream specs
    // use the full pool.
    const failover = await dns.updateConfig({
      forwarder_selection_mode: "failover",
    });
    expect(failover.config.forwarder_selection_mode).toBe("failover");
    expect(failover.config.single_upstream ?? null).toBeNull();
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
