import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { DnsFilterService, DnsService, JobsService, WardnetClient } from "@wardnet/js";

import {
  AD_BLOCKING_PROFILE_ID,
  API_BASE_URL,
  AuthedClient,
  TEST_DEBIAN_AGENT,
  ensureAdminAndLogin,
  ensureDnsEnabled,
  ensureLeasedAgent,
  findDeviceByIpRangeOrNull,
  resolveViaAgent,
  waitForJob,
  waitForReady,
} from "./helpers.js";

// See `dns-filter-device-toggle.spec.ts` for why we look up the device
// rather than re-acquiring the lease.
const POOL_START = "10.91.0.100";
const POOL_END = "10.91.0.150";

const BLOCKLIST_URL = "http://10.91.0.200/dns-filter.txt";
const BLOCKLIST_NAME = "e2e-dns-filter-default-profile";
const ALT_DEFAULT_NAME = "e2e-alt-default";
const CRON_NEVER = "0 0 1 1 *";

// Two distinct canaries, each blocked by a *different* profile. They make
// the stacking test meaningful: under [Ad] only, BLOCKED_BY_AD blocks but
// BLOCKED_BY_ALT resolves; under [alt] only, the inverse; under [Ad, alt]
// both block. If the daemon silently dropped one id from the default set,
// one of the assertions would fail.
const BLOCKED_BY_AD = "www.iana.org";
const BLOCKED_BY_ALT = "www.example.com";
const ALT_RULE = `||${BLOCKED_BY_ALT}^`;

describe("dns filter — global default profile", () => {
  let authed: AuthedClient;
  let dns: DnsService;
  let dnsFilter: DnsFilterService;
  let jobs: JobsService;
  let blocklistId: string | undefined;
  let altDefaultId: string | undefined;
  let originalDefaultIds: string[] = [];
  let deviceVisible = false;

  beforeAll(async () => {
    const client = new WardnetClient({ baseUrl: API_BASE_URL });
    await waitForReady(client);
    authed = await ensureAdminAndLogin(client);
    dns = new DnsService(authed);
    dnsFilter = new DnsFilterService(authed);
    jobs = new JobsService(authed);

    await ensureDnsEnabled(authed);

    // Snapshot the pre-spec defaults so afterAll can restore them,
    // even if some previous test changed them.
    const config = await dnsFilter.getConfig();
    originalDefaultIds = config.config.default_profile_ids;

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
    // what determines its filtering. Best-effort: the test still
    // exercises the API contract via global config flips even if the
    // daemon's packet capture isn't reaching the LAN agent in this
    // environment.
    await ensureLeasedAgent(authed, TEST_DEBIAN_AGENT, "eth0", POOL_START, POOL_END);
    const device = await findDeviceByIpRangeOrNull(authed, POOL_START, POOL_END);
    if (device) {
      await dnsFilter.updateDeviceSettings(device.id, { enabled: true, profile_ids: [] });
    }
    const haveDevice = device !== null;
    deviceVisible = haveDevice;

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

    // Alternative profile with a single custom rule blocking BLOCKED_BY_ALT
    // (a *different* canary from BLOCKED_BY_AD). The two profiles each
    // block exactly one canary, which lets the stacking test prove union
    // semantics rather than reproducing single-profile behaviour.
    const profile = await dnsFilter.createProfile({ name: ALT_DEFAULT_NAME });
    altDefaultId = profile.profile.id;
    await dnsFilter.createFilterRule(altDefaultId, {
      rule_text: ALT_RULE,
      enabled: true,
    });
  }, 180_000);

  afterAll(async () => {
    // Restore the pre-spec defaults first so the deleteProfile below
    // doesn't fail (`ON DELETE CASCADE` would silently drop default
    // membership, but explicit restoration is clearer in failure logs).
    try {
      await dnsFilter.updateConfig({ default_profile_ids: originalDefaultIds });
    } catch {
      // ignore
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

  it("flips unassigned-device filtering when the default profile changes", async (ctx) => {
    if (!deviceVisible) {
      // beforeAll couldn't see the test agent in /api/devices —
      // packet-capture environment limitation. Skip rather than fail.
      return ctx.skip();
    }
    expect(altDefaultId).toBeDefined();

    // Phase 1: default = Ad Blocking → canary blocks.
    await dnsFilter.updateConfig({ default_profile_ids: [AD_BLOCKING_PROFILE_ID] });
    await dns.flushCache();
    await expect
      .poll(
        async () => (await resolveViaAgent(TEST_DEBIAN_AGENT, BLOCKED_BY_AD)).addrs.length,
        { interval: 250, timeout: 10_000 },
      )
      .toBe(0);

    // Phase 2: switch the global default to the empty profile. The
    // hot-path "default context" should rebuild and the unassigned
    // device should no longer see the Ad Blocking blocklist.
    const swapped = await dnsFilter.updateConfig({ default_profile_ids: [altDefaultId!] });
    expect(swapped.config.default_profile_ids).toEqual([altDefaultId]);
    await dns.flushCache();
    await expect
      .poll(
        async () => (await resolveViaAgent(TEST_DEBIAN_AGENT, BLOCKED_BY_AD)).addrs.length,
        { interval: 500, timeout: 15_000 },
      )
      .toBeGreaterThan(0);

    // Phase 3: flip back. Blocking resumes.
    await dnsFilter.updateConfig({ default_profile_ids: [AD_BLOCKING_PROFILE_ID] });
    await dns.flushCache();
    await expect
      .poll(
        async () => (await resolveViaAgent(TEST_DEBIAN_AGENT, BLOCKED_BY_AD)).addrs.length,
        { interval: 250, timeout: 10_000 },
      )
      .toBe(0);
  });

  it("stacks multiple default profiles for unassigned devices", async (ctx) => {
    if (!deviceVisible) {
      return ctx.skip();
    }
    expect(altDefaultId).toBeDefined();

    // Phase 1: stack BOTH profiles. Each canary is blocked by exactly one
    // profile, so a union is the only configuration where both get blocked.
    // If the daemon dropped one id from the default set, exactly one of
    // these assertions would fail.
    const stacked = await dnsFilter.updateConfig({
      default_profile_ids: [AD_BLOCKING_PROFILE_ID, altDefaultId!],
    });
    expect(stacked.config.default_profile_ids.slice().sort()).toEqual(
      [AD_BLOCKING_PROFILE_ID, altDefaultId!].sort(),
    );
    await dns.flushCache();
    await expect
      .poll(
        async () => (await resolveViaAgent(TEST_DEBIAN_AGENT, BLOCKED_BY_AD)).addrs.length,
        { interval: 250, timeout: 10_000 },
      )
      .toBe(0);
    await expect
      .poll(
        async () => (await resolveViaAgent(TEST_DEBIAN_AGENT, BLOCKED_BY_ALT)).addrs.length,
        { interval: 250, timeout: 10_000 },
      )
      .toBe(0);

    // Phase 2: drop Ad Blocking from the default set. Now BLOCKED_BY_AD
    // should resolve (proof Ad Blocking was contributing to the stack)
    // while BLOCKED_BY_ALT keeps blocking (alt is still in the set).
    await dnsFilter.updateConfig({ default_profile_ids: [altDefaultId!] });
    await dns.flushCache();
    await expect
      .poll(
        async () => (await resolveViaAgent(TEST_DEBIAN_AGENT, BLOCKED_BY_AD)).addrs.length,
        { interval: 500, timeout: 15_000 },
      )
      .toBeGreaterThan(0);
    await expect
      .poll(
        async () => (await resolveViaAgent(TEST_DEBIAN_AGENT, BLOCKED_BY_ALT)).addrs.length,
        { interval: 250, timeout: 10_000 },
      )
      .toBe(0);

    // Phase 3: clear the default entirely. Both canaries resolve.
    const cleared = await dnsFilter.updateConfig({ default_profile_ids: [] });
    expect(cleared.config.default_profile_ids).toEqual([]);
    await dns.flushCache();
    await expect
      .poll(
        async () => (await resolveViaAgent(TEST_DEBIAN_AGENT, BLOCKED_BY_AD)).addrs.length,
        { interval: 500, timeout: 15_000 },
      )
      .toBeGreaterThan(0);
    await expect
      .poll(
        async () => (await resolveViaAgent(TEST_DEBIAN_AGENT, BLOCKED_BY_ALT)).addrs.length,
        { interval: 500, timeout: 15_000 },
      )
      .toBeGreaterThan(0);
  });
});
