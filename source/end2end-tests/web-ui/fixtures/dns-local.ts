/**
 * Local-DNS seeding helpers for the admin-site local-DNS spec (A5, #620).
 *
 * The spec creates its records/zones/forwarding rules *through the UI* (real
 * coverage of the create flows), so the only seeding here is *cleanup*: a
 * re-run against a persisted state volume would otherwise accumulate leftover
 * rows (and record/zone names are effectively unique per fixture). Each helper
 * deletes any leftover entity the spec creates, up front, so the UI-create
 * flow starts from a clean slate. Uses the plain-`fetch` `api`/`ensureAdminSetup`
 * path as the other fixtures (see seed.ts for why this harness avoids the
 * source-only SDK).
 */

import { api, ensureAdminSetup } from "./seed";

/** Custom A record the local-DNS spec creates through the UI. */
export const TEST_RECORD_DOMAIN = "e2e-record.test";
/** Authoritative zone the local-DNS spec creates (single label, non-public). */
export const TEST_ZONE_NAME = "e2e-zone";
/** Conditional-forwarding domain the local-DNS spec creates. */
export const TEST_FORWARD_DOMAIN = "e2e-forward.test";

interface CustomDnsRecord {
  id: string;
  domain: string;
  source: string;
}
interface DnsZone {
  id: string;
  name: string;
  source: string;
}
interface ForwardingRule {
  id: string;
  domain: string;
}

/**
 * Delete any leftover records/zones/forwarding rules the local-DNS spec owns,
 * so its UI-create steps are deterministic. Idempotent: missing entities are a
 * no-op; only manual/non-system rows matching the test names are touched.
 */
export async function cleanupLocalDns(): Promise<void> {
  const token = await ensureAdminSetup();

  const { records } = await api<{ records: CustomDnsRecord[] }>(
    "/dns/local/records",
    { token },
  );
  for (const r of records) {
    if (r.domain === TEST_RECORD_DOMAIN && r.source === "manual") {
      await api(`/dns/local/records/${r.id}`, { method: "DELETE", token });
    }
  }

  const { rules } = await api<{ rules: ForwardingRule[] }>(
    "/dns/local/forwarding",
    { token },
  );
  for (const rule of rules) {
    if (rule.domain === TEST_FORWARD_DOMAIN) {
      await api(`/dns/local/forwarding/${rule.id}`, {
        method: "DELETE",
        token,
      });
    }
  }

  // Delete zones last: records may reference a zone, and the daemon keeps a
  // zone's records (they become unzoned) on delete, so order only matters for
  // tidiness. System zones (the seeded `.lan`) are never touched.
  const { zones } = await api<{ zones: DnsZone[] }>("/dns/local/zones", {
    token,
  });
  for (const z of zones) {
    if (z.name === TEST_ZONE_NAME && z.source !== "system") {
      await api(`/dns/local/zones/${z.id}`, { method: "DELETE", token });
    }
  }
}
