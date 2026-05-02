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

const BLOCKLIST_URL = "http://10.91.0.200/dns-rules.txt";
const BLOCKLIST_NAME = "e2e-dns-rules";
const CRON_NEVER = "0 0 1 1 *";

// Real public domain whose block-vs-allow can be distinguished from
// the resolver answer alone. iana.org isn't used in any other Stage 7
// spec so the @@-allow override doesn't fight a stale rule cached
// elsewhere.
const OVERRIDE_DOMAIN = "iana.org";

// Fictitious target for the $dnsrewrite assertion. The IP is a TEST-NET-2
// (RFC 5737) address — guaranteed unreachable, so any answer matching
// it can only have come from our rewrite rule.
const REWRITE_DOMAIN = "rewrite-target.fixture.test";
const REWRITE_IP = "198.51.100.99";

describe("dns custom filter rules", () => {
  let authed: AuthedClient;
  let dns: DnsService;
  let jobs: JobsService;
  let blocklistId: string | undefined;
  let rewriteRuleId: string | undefined;
  let allowRuleId: string | undefined;

  beforeAll(async () => {
    const client = new WardnetClient({ baseUrl: API_BASE_URL });
    await waitForReady(client);
    authed = await ensureAdminAndLogin(client);
    dns = new DnsService(authed);
    jobs = new JobsService(authed);

    if (!(await dns.getConfig()).config.enabled) {
      await dns.toggle({ enabled: true });
    }

    // Drop leftover state from a prior run — name match for our
    // blocklist, rule_text match for our custom rules.
    const existingLists = await dns.listBlocklists();
    for (const b of existingLists.blocklists) {
      if (b.name === BLOCKLIST_NAME || b.url === BLOCKLIST_URL) {
        await dns.deleteBlocklist(b.id);
      }
    }
    const existingRules = await dns.listFilterRules();
    for (const r of existingRules.rules) {
      if (
        r.rule_text.includes(REWRITE_DOMAIN) ||
        r.rule_text.includes(OVERRIDE_DOMAIN)
      ) {
        await dns.deleteFilterRule(r.id);
      }
    }
  }, 60_000);

  afterAll(async () => {
    for (const id of [rewriteRuleId, allowRuleId]) {
      if (id) {
        try {
          await dns.deleteFilterRule(id);
        } catch {
          // ignore
        }
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

  it("applies a $dnsrewrite custom rule", async () => {
    const created = await dns.createFilterRule({
      rule_text: `||${REWRITE_DOMAIN}^$dnsrewrite=${REWRITE_IP}`,
      enabled: true,
      comment: "e2e dns-rules: rewrite",
    });
    rewriteRuleId = created.rule.id;
    expect(created.rule.enabled).toBe(true);

    // The rewrite path bypasses upstream entirely — the daemon
    // synthesizes an answer record. Asserting on the literal IP is
    // the unambiguous signal that our custom rule fired (vs the
    // domain coincidentally resolving to something else).
    await expect
      .poll(
        async () => {
          const r = await resolveViaAgent(TEST_DEBIAN_AGENT, REWRITE_DOMAIN);
          return r.addrs;
        },
        { interval: 250, timeout: 10_000 },
      )
      .toEqual([REWRITE_IP]);
  });

  it("@@-rule overrides a blocklist match", async () => {
    // Stand up the blocklist that puts OVERRIDE_DOMAIN into the
    // blocked set; verify the block before installing the @@-rule.
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

    await expect
      .poll(
        async () =>
          (await resolveViaAgent(TEST_DEBIAN_AGENT, OVERRIDE_DOMAIN)).addrs
            .length,
        { interval: 250, timeout: 10_000 },
      )
      .toBe(0);

    const allow = await dns.createFilterRule({
      rule_text: `@@||${OVERRIDE_DOMAIN}^`,
      enabled: true,
      comment: "e2e dns-rules: allow override",
    });
    allowRuleId = allow.rule.id;

    // Negative-cache flush: the prior block populated a cached
    // NXDOMAIN that would otherwise be served before the rebuilt
    // filter even runs.
    await dns.flushCache();

    await expect
      .poll(
        async () =>
          (await resolveViaAgent(TEST_DEBIAN_AGENT, OVERRIDE_DOMAIN)).addrs
            .length,
        { interval: 500, timeout: 15_000 },
      )
      .toBeGreaterThan(0);
  });
});
