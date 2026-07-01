/**
 * DNS-filter seeding helpers for the admin-site ad-blocking spec (A4, #619).
 *
 * The spec creates its filter profile through the UI (real coverage of the
 * create flow), so the only seeding done here is *cleanup*: profile names are
 * UNIQUE in the daemon schema, so a re-run against a persisted state volume
 * would collide on create. `deleteTestProfiles` removes any leftover test
 * profile up front, keeping the UI-create flow deterministic. Uses the same
 * plain-`fetch` `api`/`ensureAdminSetup` path as the other fixtures (see
 * seed.ts for why this harness avoids the source-only SDK).
 */

import { api, ensureAdminSetup } from "./seed";

/**
 * Blocklist URL the ad-blocking spec adds, served by the `blocklist_server`
 * compose service. Fetched by the daemon (not the browser), so it points at
 * that service's fixed mgmt-network IP — the daemon is its own DNS resolver,
 * so a Docker service name wouldn't resolve from inside it.
 */
export const BLOCKLIST_FIXTURE_URL =
  process.env.WARDNET_BLOCKLIST_FIXTURE_URL ?? "http://10.90.0.20/hosts.txt";

/** Name of the profile the ad-blocking spec creates through the UI. */
export const TEST_PROFILE_NAME = "e2e-adblock";

/**
 * Name of the profile the filter-lifecycle spec (A5, #620) creates, edits, and
 * deletes. Distinct from `TEST_PROFILE_NAME` so the two DNS-filter specs never
 * collide on the daemon's UNIQUE profile-name constraint when they share a
 * persisted state volume.
 */
export const PROFILE_MGMT_NAME = "e2e-profile-mgmt";

/**
 * The name the lifecycle spec renames the profile to during its edit step.
 * Cleaned up alongside `PROFILE_MGMT_NAME` so a mid-test failure (which could
 * leave the profile under either name) doesn't wedge the next run.
 */
export const PROFILE_MGMT_RENAMED = "e2e-profile-mgmt-renamed";

interface FilterProfile {
  id: string;
  name: string;
  builtin: boolean;
}

/**
 * Delete any non-builtin filter profile matching `name`, so the spec's
 * UI-create step starts from a clean slate. Idempotent: a missing profile
 * (already deleted) is a no-op; builtin profiles are never touched.
 */
export async function deleteTestProfiles(
  name = TEST_PROFILE_NAME,
): Promise<void> {
  const token = await ensureAdminSetup();
  const { profiles } = await api<{ profiles: FilterProfile[] }>(
    "/dns/filter/profiles",
    { token },
  );
  for (const p of profiles) {
    if (p.name === name && !p.builtin) {
      await api(`/dns/filter/profiles/${p.id}`, { method: "DELETE", token });
    }
  }
}
