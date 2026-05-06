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
  findDeviceByIpRange,
  resolveViaAgent,
  waitForJob,
  waitForReady,
} from "./helpers.js";

// `dhcp.spec.ts` runs first under singleFork and leases test_debian an
// address in this pool; we look up the device from there rather than
// re-trigger `dhclient renew` (which races with the daemon's DHCP
// runner and times out).
const POOL_START = "10.91.0.100";
const POOL_END = "10.91.0.150";

const BLOCKLIST_URL = "http://10.91.0.200/dns-filter.txt";
const BLOCKLIST_NAME = "e2e-dns-filter-device-toggle";
const CRON_NEVER = "0 0 1 1 *";

// Real public domain — resolves via upstream when filtering is
// bypassed, NXDOMAIN when filtering is in effect. Same fixture
// strategy as dns-blocklists.spec.ts.
const BLOCKED_DOMAIN = "www.iana.org";

describe("dns filter — per-device kill switch", () => {
  let authed: AuthedClient;
  let dns: DnsService;
  let dnsFilter: DnsFilterService;
  let jobs: JobsService;
  let blocklistId: string | undefined;
  let deviceId: string | undefined;

  beforeAll(async () => {
    const client = new WardnetClient({ baseUrl: API_BASE_URL });
    await waitForReady(client);
    authed = await ensureAdminAndLogin(client);
    dns = new DnsService(authed);
    dnsFilter = new DnsFilterService(authed);
    jobs = new JobsService(authed);

    // Vitest doesn't guarantee dhcp/dns-config run before this spec under
    // singleFork; toggle on if needed (with retry — see helpers).
    await ensureDnsEnabled(authed);

    // Drop leftover blocklists from a prior failed run on the same
    // wardnet_state volume.
    const existing = await dnsFilter.listBlocklists(AD_BLOCKING_PROFILE_ID);
    for (const b of existing.blocklists) {
      if (b.name === BLOCKLIST_NAME || b.url === BLOCKLIST_URL) {
        await dnsFilter.deleteBlocklist(AD_BLOCKING_PROFILE_ID, b.id);
      }
    }

    // Vitest may run this spec before dhcp.spec.ts; ensure the agent has
    // an in-pool lease so the daemon has a device row to address.
    await ensureLeasedAgent(authed, TEST_DEBIAN_AGENT, "eth0", POOL_START, POOL_END);
    const device = await findDeviceByIpRange(authed, POOL_START, POOL_END);
    deviceId = device.id;

    // Reset to defaults: enabled=true, no explicit profiles, so the
    // device follows the default profile (Ad Blocking) and the
    // blocklist below is in scope.
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
  }, 120_000);

  afterAll(async () => {
    if (deviceId) {
      try {
        // Restore defaults so subsequent specs aren't observing the
        // post-toggle state that the last `it` left behind.
        await dnsFilter.updateDeviceSettings(deviceId, { enabled: true, profile_ids: [] });
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

  it("blocks → kill-switch off → resolves → re-enable → blocks", async () => {
    expect(deviceId).toBeDefined();

    // Phase 1: filtering is in effect (default state). The blocklist
    // we just installed should NXDOMAIN the canary.
    await expect
      .poll(
        async () => (await resolveViaAgent(TEST_DEBIAN_AGENT, BLOCKED_DOMAIN)).addrs.length,
        { interval: 250, timeout: 10_000 },
      )
      .toBe(0);

    // Phase 2: flip the per-device kill switch off. With enabled=false
    // the daemon's hot path returns Pass before consulting any
    // profile, regardless of what the default profile would do.
    const off = await dnsFilter.updateDeviceSettings(deviceId!, {
      enabled: false,
      profile_ids: [],
    });
    expect(off.settings.enabled).toBe(false);

    // Cached NXDOMAIN from Phase 1 would mask the toggle's effect on
    // the filter — DnsCache.get serves stale negative answers within
    // their TTL. Flush so the next query is a fresh filter pass.
    await dns.flushCache();

    await expect
      .poll(
        async () => (await resolveViaAgent(TEST_DEBIAN_AGENT, BLOCKED_DOMAIN)).addrs.length,
        { interval: 500, timeout: 15_000 },
      )
      .toBeGreaterThan(0);

    // Phase 3: flip back on. Should resume blocking. This is the round
    // trip that makes the spec a true acceptance gate for #221 — a
    // one-way toggle would not catch a bug where the runner forgets
    // to remove the device from the kill-switch map.
    const on = await dnsFilter.updateDeviceSettings(deviceId!, {
      enabled: true,
      profile_ids: [],
    });
    expect(on.settings.enabled).toBe(true);
    await dns.flushCache();

    await expect
      .poll(
        async () => (await resolveViaAgent(TEST_DEBIAN_AGENT, BLOCKED_DOMAIN)).addrs.length,
        { interval: 250, timeout: 10_000 },
      )
      .toBe(0);
  });
});
