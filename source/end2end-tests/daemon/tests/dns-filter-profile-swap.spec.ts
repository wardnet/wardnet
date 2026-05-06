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
const BLOCKLIST_NAME = "e2e-dns-filter-profile-swap";
const EMPTY_PROFILE_NAME = "e2e-empty-profile";
const CRON_NEVER = "0 0 1 1 *";

const BLOCKED_DOMAIN = "www.iana.org";

describe("dns filter — per-device profile swap", () => {
  let authed: AuthedClient;
  let dns: DnsService;
  let dnsFilter: DnsFilterService;
  let jobs: JobsService;
  let blocklistId: string | undefined;
  let emptyProfileId: string | undefined;
  let deviceId: string | undefined;

  beforeAll(async () => {
    const client = new WardnetClient({ baseUrl: API_BASE_URL });
    await waitForReady(client);
    authed = await ensureAdminAndLogin(client);
    dns = new DnsService(authed);
    dnsFilter = new DnsFilterService(authed);
    jobs = new JobsService(authed);

    await ensureDnsEnabled(authed);

    // Clean leftover blocklists / custom profiles from prior runs so
    // they don't combine with the new ones to produce the wrong
    // resolver answer.
    const existingLists = await dnsFilter.listBlocklists(AD_BLOCKING_PROFILE_ID);
    for (const b of existingLists.blocklists) {
      if (b.name === BLOCKLIST_NAME || b.url === BLOCKLIST_URL) {
        await dnsFilter.deleteBlocklist(AD_BLOCKING_PROFILE_ID, b.id);
      }
    }
    const existingProfiles = await dnsFilter.listProfiles();
    for (const p of existingProfiles.profiles) {
      if (!p.builtin && p.name === EMPTY_PROFILE_NAME) {
        await dnsFilter.deleteProfile(p.id);
      }
    }

    // Empty profile — independent of device discovery; create it so the
    // builtin-protection test below can run even when packet capture
    // doesn't reach the agent.
    const profile = await dnsFilter.createProfile({ name: EMPTY_PROFILE_NAME });
    emptyProfileId = profile.profile.id;
    expect(profile.profile.builtin).toBe(false);

    await ensureLeasedAgent(authed, TEST_DEBIAN_AGENT, "eth0", POOL_START, POOL_END);
    const device = await findDeviceByIpRangeOrNull(authed, POOL_START, POOL_END);
    if (!device) {
      // Daemon's packet capture isn't reaching wardnet_lan in this env.
      // The profile-creation path was already exercised; the per-device
      // round-trip below will skip.
      return;
    }
    deviceId = device.id;
    await dnsFilter.updateDeviceSettings(deviceId, { enabled: true, profile_ids: [] });

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
  }, 180_000);

  afterAll(async () => {
    if (deviceId) {
      try {
        // Clear explicit assignments so the device falls back to the
        // default profile for subsequent specs.
        await dnsFilter.updateDeviceSettings(deviceId, { enabled: true, profile_ids: [] });
      } catch {
        // ignore
      }
    }
    if (emptyProfileId) {
      try {
        await dnsFilter.deleteProfile(emptyProfileId);
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

  it("blocks under default → swap to empty profile → resolves → swap back → blocks", async (ctx) => {
    if (!deviceId) {
      return ctx.skip();
    }
    expect(emptyProfileId).toBeDefined();

    // Phase 1: device follows the default profile (Ad Blocking),
    // which holds the blocklist. Canary should NXDOMAIN.
    await expect
      .poll(
        async () => (await resolveViaAgent(TEST_DEBIAN_AGENT, BLOCKED_DOMAIN)).addrs.length,
        { interval: 250, timeout: 10_000 },
      )
      .toBe(0);

    // Phase 2: assign the device to the empty profile. The hot path
    // sees an explicit non-empty `profile_ids`, so it stops falling
    // through to the default and uses only the empty profile —
    // nothing is blocked.
    const swapped = await dnsFilter.updateDeviceSettings(deviceId!, {
      enabled: true,
      profile_ids: [emptyProfileId!],
    });
    expect(swapped.settings.profile_ids).toEqual([emptyProfileId]);

    await dns.flushCache();
    await expect
      .poll(
        async () => (await resolveViaAgent(TEST_DEBIAN_AGENT, BLOCKED_DOMAIN)).addrs.length,
        { interval: 500, timeout: 15_000 },
      )
      .toBeGreaterThan(0);

    // Phase 3: clear explicit assignments. Device falls back to the
    // default profile, blocking resumes.
    await dnsFilter.updateDeviceSettings(deviceId!, { enabled: true, profile_ids: [] });
    await dns.flushCache();
    await expect
      .poll(
        async () => (await resolveViaAgent(TEST_DEBIAN_AGENT, BLOCKED_DOMAIN)).addrs.length,
        { interval: 250, timeout: 10_000 },
      )
      .toBe(0);
  });

  it("refuses to delete a builtin profile", async () => {
    // Per architecture decision #4 in the issue plan: builtins return
    // 409 on DELETE. The web UI relies on this to gate the delete
    // affordance — exercising it here keeps the contract honest.
    await expect(dnsFilter.deleteProfile(AD_BLOCKING_PROFILE_ID)).rejects.toMatchObject({
      status: 409,
    });
  });
});
