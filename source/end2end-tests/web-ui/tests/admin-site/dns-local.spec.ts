import { expect, test } from "@playwright/test";

import {
  TEST_FORWARD_DOMAIN,
  TEST_RECORD_DOMAIN,
  TEST_ZONE_NAME,
  cleanupLocalDns,
} from "../../fixtures/dns-local.js";

/**
 * Admin-site local-DNS coverage (A5, #620) for `/admin/dns/local`.
 *
 * Each of the page's three interactive cards — custom Records, authoritative
 * Zones, and Conditional forwarding — is exercised through its real UI
 * create/edit/delete flow. State is created and removed within each test so the
 * shared seeded daemon is left as found.
 *
 * Runs in the `admin-site` project (seeded daemon + admin storageState).
 * Selectors follow the suite's `data-testid` convention (README.md); table rows
 * are located by their unique text and deletes go through the shared
 * `confirm-dialog-confirm` control.
 */

// Record/zone/forwarding names are effectively unique per fixture, so a re-run
// against a persisted state volume would accumulate leftovers. Clear them up
// front so every create step starts from a clean slate.
test.beforeAll(async () => {
  await cleanupLocalDns();
});

test("local DNS: create, edit, toggle, and delete a custom record", async ({
  page,
}) => {
  await page.goto("./dns/local");
  await expect(page.getByTestId("page-title")).toHaveText("Local DNS");

  // ── Create an A record ─────────────────────────────────────────────
  await page.getByTestId("local-record-add").click();
  await page.getByTestId("rec-domain").fill(TEST_RECORD_DOMAIN);
  await page.getByTestId("rec-value").fill("192.168.1.50");
  await page.getByTestId("local-record-submit").click();

  const row = page.getByRole("row").filter({ hasText: TEST_RECORD_DOMAIN });
  await expect(row).toBeVisible();
  await expect(row).toContainText("192.168.1.50");

  // ── Edit its value ─────────────────────────────────────────────────
  await row.getByTestId("local-record-row-menu").click();
  await page.getByTestId("local-record-edit").click();
  await page.getByTestId("rec-value").fill("192.168.1.51");
  await page.getByTestId("local-record-submit").click();
  await expect(row).toContainText("192.168.1.51");

  // ── Toggle it off (starts enabled) ─────────────────────────────────
  const enabledToggle = row.getByRole("switch");
  await expect(enabledToggle).toHaveAttribute("aria-checked", "true");
  await enabledToggle.click();
  await expect(enabledToggle).toHaveAttribute("aria-checked", "false");

  // ── Delete it ──────────────────────────────────────────────────────
  await row.getByTestId("local-record-row-menu").click();
  await page.getByTestId("local-record-delete").click();
  await page.getByTestId("confirm-dialog-confirm").click();
  await expect(row).toHaveCount(0);
});

test("local DNS: create and delete an authoritative zone", async ({ page }) => {
  await page.goto("./dns/local");

  await page.getByTestId("zone-add").click();
  await page.getByTestId("zone-name").fill(TEST_ZONE_NAME);
  await page.getByTestId("zone-submit").click();

  const row = page.getByRole("row").filter({ hasText: TEST_ZONE_NAME });
  await expect(row).toBeVisible();

  await row.getByTestId("zone-row-menu").click();
  await page.getByTestId("zone-delete").click();
  await page.getByTestId("confirm-dialog-confirm").click();
  await expect(row).toHaveCount(0);
});

test("local DNS: create and delete a conditional-forwarding rule", async ({
  page,
}) => {
  await page.goto("./dns/local");

  await page.getByTestId("fwd-add").click();
  await page.getByTestId("fwd-domain").fill(TEST_FORWARD_DOMAIN);
  await page.getByTestId("fwd-upstream").fill("10.0.0.1");
  await page.getByTestId("fwd-submit").click();

  const row = page.getByRole("row").filter({ hasText: TEST_FORWARD_DOMAIN });
  await expect(row).toBeVisible();
  await expect(row).toContainText("10.0.0.1");

  await row.getByTestId("fwd-row-menu").click();
  await page.getByTestId("fwd-delete").click();
  await page.getByTestId("confirm-dialog-confirm").click();
  await expect(row).toHaveCount(0);
});
