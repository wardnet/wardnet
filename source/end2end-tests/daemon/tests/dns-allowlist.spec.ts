import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { DnsService, JobsService, WardnetClient } from "@wardnet/js";

import {
  API_BASE_URL,
  AuthedClient,
  TEST_DEBIAN_AGENT,
  ensureAdminAndLogin,
  resolveViaAgent,
  waitForJob,
  waitForReady,
} from "./helpers.js";

const BLOCKLIST_URL = "http://10.91.0.200/dns-allowlist.txt";
const BLOCKLIST_NAME = "e2e-dns-allowlist";
const CRON_NEVER = "0 0 1 1 *";

// Real public domain — required because the allow-overrides-block
// assertion needs the override path to forward upstream and produce a
// real answer. See fixtures/blocklists/dns-allowlist.txt.
const TARGET_DOMAIN = "example.com";

describe("dns allowlist", () => {
  let authed: AuthedClient;
  let dns: DnsService;
  let jobs: JobsService;
  let blocklistId: string | undefined;
  let allowlistEntryId: string | undefined;

  beforeAll(async () => {
    const client = new WardnetClient({ baseUrl: API_BASE_URL });
    await waitForReady(client);
    authed = await ensureAdminAndLogin(client);
    dns = new DnsService(authed);
    jobs = new JobsService(authed);

    if (!(await dns.getConfig()).config.enabled) {
      await dns.toggle({ enabled: true });
    }

    // Clean leftover allowlist entries / blocklists from prior runs
    // on the same wardnet_state volume.
    const allow = await dns.listAllowlist();
    for (const e of allow.entries) {
      if (e.domain === TARGET_DOMAIN) {
        await dns.deleteAllowlistEntry(e.id);
      }
    }
    const lists = await dns.listBlocklists();
    for (const b of lists.blocklists) {
      if (b.name === BLOCKLIST_NAME || b.url === BLOCKLIST_URL) {
        await dns.deleteBlocklist(b.id);
      }
    }

    const created = await dns.createBlocklist({
      name: BLOCKLIST_NAME,
      url: BLOCKLIST_URL,
      cron_schedule: CRON_NEVER,
      enabled: true,
    });
    blocklistId = created.blocklist.id;
    const dispatched = await dns.updateBlocklistNow(blocklistId);
    const job = await waitForJob(jobs, dispatched.job_id, 30_000);
    expect(job.status, `job error: ${job.error ?? "(none)"}`).toBe("SUCCEED");
  }, 90_000);

  afterAll(async () => {
    if (allowlistEntryId) {
      try {
        await dns.deleteAllowlistEntry(allowlistEntryId);
      } catch {
        // ignore
      }
    }
    if (blocklistId) {
      try {
        await dns.deleteBlocklist(blocklistId);
      } catch {
        // ignore
      }
    }
    try {
      await dns.flushCache();
    } catch {
      // ignore
    }
  });

  it("blocks the domain until an allowlist entry overrides", async () => {
    // Sanity check the blocklist is in effect — without this, a
    // false-pass ("allowlist works because nothing was blocked") is
    // possible if the fixture didn't load.
    await expect
      .poll(
        async () =>
          (await resolveViaAgent(TEST_DEBIAN_AGENT, TARGET_DOMAIN)).addrs
            .length,
        { interval: 250, timeout: 10_000 },
      )
      .toBe(0);

    const created = await dns.createAllowlistEntry({
      domain: TARGET_DOMAIN,
      reason: "e2e dns-allowlist spec",
    });
    allowlistEntryId = created.entry.id;

    // Cached NXDOMAIN from the block phase would mask the allowlist
    // override (DnsCache.get serves stale negative answers within
    // their TTL). Flush so the post-allowlist query is a fresh
    // filter pass + upstream forward.
    await dns.flushCache();

    await expect
      .poll(
        async () =>
          (await resolveViaAgent(TEST_DEBIAN_AGENT, TARGET_DOMAIN)).addrs
            .length,
        { interval: 500, timeout: 15_000 },
      )
      .toBeGreaterThan(0);
  });

  it("re-blocks the domain when the allowlist entry is removed", async () => {
    expect(allowlistEntryId, "previous test created an allowlist entry")
      .toBeDefined();

    await dns.deleteAllowlistEntry(allowlistEntryId!);
    allowlistEntryId = undefined;

    // Same caching subtlety as above, in reverse: a cached real
    // answer would be served regardless of the rebuilt filter.
    await dns.flushCache();

    await expect
      .poll(
        async () =>
          (await resolveViaAgent(TEST_DEBIAN_AGENT, TARGET_DOMAIN)).addrs
            .length,
        { interval: 250, timeout: 10_000 },
      )
      .toBe(0);
  });
});
