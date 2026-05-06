import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { DnsFilterService, DnsService, JobsService, WardnetClient } from "@wardnet/js";

import {
  AD_BLOCKING_PROFILE_ID,
  API_BASE_URL,
  AuthedClient,
  TEST_DEBIAN_AGENT,
  acquireLeaseInRange,
  ensureAdminAndLogin,
  findDeviceByIp,
  resolveViaAgent,
  waitForJob,
  waitForReady,
} from "./helpers.js";

const POOL_START = "10.91.0.100";
const POOL_END = "10.91.0.150";

const BLOCKLIST_URL = "http://10.91.0.200/dns-filter.txt";
const BLOCKLIST_NAME = "e2e-dns-filter-default-profile";
const ALT_DEFAULT_NAME = "e2e-alt-default";
const CRON_NEVER = "0 0 1 1 *";

const BLOCKED_DOMAIN = "www.iana.org";

describe("dns filter — global default profile", () => {
  let authed: AuthedClient;
  let dns: DnsService;
  let dnsFilter: DnsFilterService;
  let jobs: JobsService;
  let blocklistId: string | undefined;
  let altDefaultId: string | undefined;
  let originalDefaultId: string | null = null;

  beforeAll(async () => {
    const client = new WardnetClient({ baseUrl: API_BASE_URL });
    await waitForReady(client);
    authed = await ensureAdminAndLogin(client);
    dns = new DnsService(authed);
    dnsFilter = new DnsFilterService(authed);
    jobs = new JobsService(authed);

    if (!(await dns.getConfig()).config.enabled) {
      await dns.toggle({ enabled: true });
    }

    // Snapshot the pre-spec default so afterAll can restore it,
    // even if some previous test changed it.
    const config = await dnsFilter.getConfig();
    originalDefaultId = config.config.default_profile_id;

    // Drop leftover state from a prior failed run.
    const existingLists = await dnsFilter.listBlocklists(AD_BLOCKING_PROFILE_ID);
    for (const b of existingLists.blocklists) {
      if (b.name === BLOCKLIST_NAME || b.url === BLOCKLIST_URL) {
        await dnsFilter.deleteBlocklist(AD_BLOCKING_PROFILE_ID, b.id);
      }
    }
    const existingProfiles = await dnsFilter.listProfiles();
    for (const p of existingProfiles.profiles) {
      if (!p.builtin && p.name === ALT_DEFAULT_NAME) {
        await dnsFilter.deleteProfile(p.id);
      }
    }

    // The spec exercises *unassigned* devices — make sure the test
    // device has no explicit profile_ids so the global default is
    // what determines its filtering.
    const ip = await acquireLeaseInRange(TEST_DEBIAN_AGENT, "eth0", POOL_START, POOL_END);
    const device = await findDeviceByIp(authed, ip);
    await dnsFilter.updateDeviceSettings(device.id, { enabled: true, profile_ids: [] });

    // Make the canary blockable under Ad Blocking (the seed default).
    const created = await dnsFilter.createBlocklist(AD_BLOCKING_PROFILE_ID, {
      name: BLOCKLIST_NAME,
      url: BLOCKLIST_URL,
      cron_schedule: CRON_NEVER,
      enabled: true,
    });
    blocklistId = created.blocklist.id;
    const dispatched = await dnsFilter.refreshBlocklist(AD_BLOCKING_PROFILE_ID, blocklistId);
    const job = await waitForJob(jobs, dispatched.job_id, 30_000);
    expect(job.status, `job error: ${job.error ?? "(none)"}`).toBe("SUCCEED");

    // Empty alternative profile — when the global default points at
    // it, an unassigned device sees no rules and resolves freely.
    const profile = await dnsFilter.createProfile({ name: ALT_DEFAULT_NAME });
    altDefaultId = profile.profile.id;
  }, 120_000);

  afterAll(async () => {
    // Restore the pre-spec default first so the deleteProfile below
    // doesn't fail (`ON DELETE SET NULL` would silently drop the
    // pointer, but explicit restoration is clearer in failure logs).
    if (originalDefaultId !== null) {
      try {
        await dnsFilter.updateConfig({ default_profile_id: originalDefaultId });
      } catch {
        // ignore
      }
    }
    if (altDefaultId) {
      try {
        await dnsFilter.deleteProfile(altDefaultId);
      } catch {
        // ignore
      }
    }
    if (blocklistId) {
      try {
        await dnsFilter.deleteBlocklist(AD_BLOCKING_PROFILE_ID, blocklistId);
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

  it("flips unassigned-device filtering when the default profile changes", async () => {
    expect(altDefaultId).toBeDefined();

    // Phase 1: default = Ad Blocking → canary blocks.
    await dnsFilter.updateConfig({ default_profile_id: AD_BLOCKING_PROFILE_ID });
    await dns.flushCache();
    await expect
      .poll(
        async () => (await resolveViaAgent(TEST_DEBIAN_AGENT, BLOCKED_DOMAIN)).addrs.length,
        { interval: 250, timeout: 10_000 },
      )
      .toBe(0);

    // Phase 2: switch the global default to the empty profile. The
    // hot-path "default context" should rebuild and the unassigned
    // device should no longer see the Ad Blocking blocklist.
    const swapped = await dnsFilter.updateConfig({ default_profile_id: altDefaultId! });
    expect(swapped.config.default_profile_id).toBe(altDefaultId);
    await dns.flushCache();
    await expect
      .poll(
        async () => (await resolveViaAgent(TEST_DEBIAN_AGENT, BLOCKED_DOMAIN)).addrs.length,
        { interval: 500, timeout: 15_000 },
      )
      .toBeGreaterThan(0);

    // Phase 3: flip back. Blocking resumes.
    await dnsFilter.updateConfig({ default_profile_id: AD_BLOCKING_PROFILE_ID });
    await dns.flushCache();
    await expect
      .poll(
        async () => (await resolveViaAgent(TEST_DEBIAN_AGENT, BLOCKED_DOMAIN)).addrs.length,
        { interval: 250, timeout: 10_000 },
      )
      .toBe(0);
  });
});
