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

// Static IP of the blocklist_server fixture (compose.yaml). Pinned by
// IP rather than service hostname so the daemon's HTTP client never
// has to round-trip through Docker DNS to fetch the file.
const BLOCKLIST_URL = "http://10.91.0.200/dns-blocklists.txt";
const BLOCKLIST_NAME = "e2e-dns-blocklists";
// Annual cron — picked so the runner's periodic check in
// dns/runner.rs cannot fire mid-spec and re-download the file
// against its own ad-hoc clock. Manual updateBlocklistNow() is what
// drives state in the spec.
const CRON_NEVER = "0 0 1 1 *";

// Real public domain so block (NXDOMAIN) vs delete (real answer)
// can be distinguished from the resolver response alone. See
// fixtures/blocklists/dns-blocklists.txt for why this beats a
// fictitious .test domain.
const BLOCKED_DOMAIN = "www.iana.org";
const PASSTHROUGH_DOMAIN = "example.org";

describe("dns blocklists", () => {
  let authed: AuthedClient;
  let dns: DnsService;
  let jobs: JobsService;
  let blocklistId: string | undefined;

  beforeAll(async () => {
    const client = new WardnetClient({ baseUrl: API_BASE_URL });
    await waitForReady(client);
    authed = await ensureAdminAndLogin(client);
    dns = new DnsService(authed);
    jobs = new JobsService(authed);

    if (!(await dns.getConfig()).config.enabled) {
      await dns.toggle({ enabled: true });
    }

    // Drop any blocklist left over from a prior run on the same
    // wardnet_state volume — createBlocklist doesn't enforce name
    // uniqueness, but stale entries would still serve their old
    // domains via the in-memory filter and skew assertions.
    const existing = await dns.listBlocklists();
    for (const b of existing.blocklists) {
      if (b.name === BLOCKLIST_NAME || b.url === BLOCKLIST_URL) {
        await dns.deleteBlocklist(b.id);
      }
    }
  }, 60_000);

  afterAll(async () => {
    if (blocklistId) {
      try {
        await dns.deleteBlocklist(blocklistId);
      } catch {
        // ignore
      }
    }
    // Cached NXDOMAIN entries from the blocking phase would leak
    // into other specs; flush so subsequent resolves see the real
    // upstream answer for BLOCKED_DOMAIN.
    try {
      await dns.flushCache();
    } catch {
      // ignore
    }
  });

  it("fetches the blocklist and starts blocking listed domains", async () => {
    const created = await dns.createBlocklist({
      name: BLOCKLIST_NAME,
      url: BLOCKLIST_URL,
      cron_schedule: CRON_NEVER,
      enabled: true,
    });
    blocklistId = created.blocklist.id;
    expect(created.blocklist.entry_count).toBe(0);

    const dispatched = await dns.updateBlocklistNow(blocklistId);
    const job = await waitForJob(jobs, dispatched.job_id, 30_000);
    expect(job.status, `job error: ${job.error ?? "(none)"}`).toBe("SUCCEED");
    expect(job.percentage_done).toBe(100);

    const refreshed = (await dns.listBlocklists()).blocklists.find(
      (b) => b.id === blocklistId,
    );
    expect(refreshed, "blocklist still listed after refresh").toBeDefined();
    // Three domains in the fixture file — see
    // fixtures/blocklists/dns-blocklists.txt.
    expect(refreshed!.entry_count).toBe(3);
    expect(refreshed!.last_error).toBeNull();
    expect(refreshed!.last_updated).not.toBeNull();

    // Filter rebuild is async (DnsBlocklistUpdated → runner rebuilds
    // DnsFilter on the next event-bus tick). Poll the resolver until
    // the new state is visible, with a short ceiling so a real
    // regression still fails fast.
    await expect
      .poll(
        async () =>
          (await resolveViaAgent(TEST_DEBIAN_AGENT, BLOCKED_DOMAIN)).addrs
            .length,
        { interval: 250, timeout: 10_000 },
      )
      .toBe(0);

    // Sanity: a non-listed domain still resolves via upstream. This
    // separates "blocklist working" from "DNS server broken".
    const passthrough = await resolveViaAgent(
      TEST_DEBIAN_AGENT,
      PASSTHROUGH_DOMAIN,
    );
    expect(
      passthrough.addrs.length,
      `${PASSTHROUGH_DOMAIN} should still resolve through upstream`,
    ).toBeGreaterThan(0);
  });

  it("stops blocking once the blocklist is deleted", async () => {
    expect(blocklistId, "previous test created a blocklist").toBeDefined();

    await dns.deleteBlocklist(blocklistId!);
    blocklistId = undefined;

    // Cached NXDOMAINs from the blocking phase would mask the
    // delete's effect on the filter; flush so the next query is a
    // fresh forward to upstream.
    await dns.flushCache();

    await expect
      .poll(
        async () =>
          (await resolveViaAgent(TEST_DEBIAN_AGENT, BLOCKED_DOMAIN)).addrs
            .length,
        { interval: 500, timeout: 15_000 },
      )
      .toBeGreaterThan(0);

    const after = await dns.listBlocklists();
    expect(after.blocklists.find((b) => b.url === BLOCKLIST_URL))
      .toBeUndefined();
  });
});
