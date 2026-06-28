import { expect, test } from "@playwright/test";

import {
  BLOCKLIST_FIXTURE_URL,
  TEST_PROFILE_NAME,
  deleteTestProfiles,
} from "../../fixtures/dns-filter.js";

/**
 * Admin-site ad-blocking coverage (A4, #619) — the day-zero feature.
 *
 * Ad blocking is configured per DNS filter profile, so this spec creates
 * a profile *through the UI* (real coverage of the create flow), then
 * exercises the three source kinds on its detail page: add a blocklist
 * and force an update that the daemon actually fetches from the
 * `blocklist_server` compose fixture, add an allowlist entry, and add a
 * custom rule. Profile management (list, default-assignment, delete) is
 * covered separately by A5.
 *
 * Runs in the `admin-site` project (seeded daemon + admin storageState).
 * Selectors follow the suite's `data-testid` convention (README.md).
 */

// Profile names are UNIQUE in the daemon schema, so a re-run against a
// persisted state volume would collide on the UI-create step. Clear any
// leftover test profile first so creation is deterministic.
test.beforeAll(async () => {
  await deleteTestProfiles();
});

test("ad blocking: create a profile, add a blocklist + force update, allow a domain, add a custom rule", async ({
  page,
}) => {
  // ── Create the profile via the UI ──────────────────────────────────
  await page.goto("./dns/filter");
  await page.getByTestId("filter-add-profile").click();
  await expect(page).toHaveURL(/\/admin\/dns\/filter\/profiles\/new$/);

  await page.getByTestId("profile-name").fill(TEST_PROFILE_NAME);
  await page.getByTestId("profile-create-submit").click();

  // Lands on the new profile's detail page, titled with its name.
  await expect(page).toHaveURL(/\/admin\/dns\/filter\/profiles\/[0-9a-f-]+$/);
  await expect(page.getByText(TEST_PROFILE_NAME).first()).toBeVisible();

  // ── Blocklist: add, then force an update the daemon really fetches ──
  const blocklistName = "e2e-blocklist";

  await page.getByTestId("blocklist-add").click();
  await page.getByTestId("blocklist-name").fill(blocklistName);
  await page.getByTestId("blocklist-url").fill(BLOCKLIST_FIXTURE_URL);
  await page.getByTestId("blocklist-submit").click();

  const blocklistRow = page.getByRole("row").filter({ hasText: blocklistName });
  await expect(blocklistRow).toBeVisible();

  // Force an update via the row's overflow menu. The daemon downloads the
  // fixture, parses it, and on success sets `last_updated` + a non-zero
  // entry count — so the row's "Updated at" stops reading "Never". That
  // durable row change is the success signal (the toast is transient).
  //
  // We deliberately do NOT assert "Never" *before* refreshing: the daemon's
  // blocklist cron ticks every 60s and treats a never-fetched blocklist as
  // always-due (`last_updated == None`), so it can refresh a brand-new
  // blocklist on its own before this assertion would run — which would make
  // a pre-refresh "Never" check flaky.
  await blocklistRow.getByTestId("blocklist-row-menu").click();
  await page.getByTestId("blocklist-refresh").click();
  await expect(blocklistRow).not.toContainText("Never");

  // ── Allowlist: add an entry ────────────────────────────────────────
  const allowedDomain = "allowed.e2e-fixture.test";

  await page.getByTestId("allowlist-add").click();
  await page.getByTestId("allowlist-domain").fill(allowedDomain);
  await page.getByTestId("allowlist-submit").click();

  await expect(
    page.getByRole("row").filter({ hasText: allowedDomain }),
  ).toBeVisible();

  // ── Custom rule: add an AdGuard-syntax rule ────────────────────────
  const ruleText = "||example.com^";

  await page.getByTestId("rule-add").click();
  await page.getByTestId("rule-text").fill(ruleText);
  await page.getByTestId("rule-submit").click();

  await expect(
    page.getByRole("row").filter({ hasText: ruleText }),
  ).toBeVisible();
});
