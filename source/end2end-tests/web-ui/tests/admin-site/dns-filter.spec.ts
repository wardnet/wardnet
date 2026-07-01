import { expect, test } from "@playwright/test";

import { api, ensureAdminSetup } from "../../fixtures/seed.js";
import {
  PROFILE_MGMT_NAME,
  PROFILE_MGMT_RENAMED,
  deleteTestProfiles,
} from "../../fixtures/dns-filter.js";

/**
 * Admin-site DNS filter-profile lifecycle coverage (A5, #620).
 *
 * A4's `adblocking.spec.ts` covers a profile's *content* (blocklists,
 * allowlists, custom rules); this spec owns the profile *lifecycle* it
 * deferred: create via the `/new` form, see it in the list, edit its identity,
 * mark it as a default profile, and delete it.
 *
 * Runs in the `admin-site` project (seeded daemon + admin storageState).
 * Selectors follow the suite's `data-testid` convention (README.md).
 */

// Profile names are UNIQUE in the daemon schema, so a re-run against a
// persisted state volume would collide on create. Clear both names this spec
// may leave behind (a mid-test failure could leave the profile renamed), and
// enable global filtering so the list's default-profile toggle is interactive.
test.beforeAll(async () => {
  await deleteTestProfiles(PROFILE_MGMT_NAME);
  await deleteTestProfiles(PROFILE_MGMT_RENAMED);
  const token = await ensureAdminSetup();
  await api("/dns/filter/config", {
    method: "PUT",
    token,
    body: JSON.stringify({ enabled: true }),
  });
});

test("filter profile: create, list, edit, set default, delete", async ({
  page,
}) => {
  // ── Create via the /new form ───────────────────────────────────────
  await page.goto("./dns/filter");
  await page.getByTestId("filter-add-profile").click();
  await expect(page).toHaveURL(/\/admin\/dns\/filter\/profiles\/new$/);

  // Submit is gated on a non-empty name.
  await expect(page.getByTestId("profile-create-submit")).toBeDisabled();
  await page.getByTestId("profile-name").fill(PROFILE_MGMT_NAME);
  await page.getByTestId("profile-create-submit").click();
  await expect(page).toHaveURL(/\/admin\/dns\/filter\/profiles\/[0-9a-f-]+$/);

  // ── It appears in the list ─────────────────────────────────────────
  await page.goto("./dns/filter");
  const listRow = page.getByRole("row").filter({ hasText: PROFILE_MGMT_NAME });
  await expect(listRow).toBeVisible();

  // ── Edit its identity (rename + description) ───────────────────────
  await listRow.click();
  await expect(page).toHaveURL(/\/admin\/dns\/filter\/profiles\/[0-9a-f-]+$/);
  await page.getByTestId("profile-edit").click();
  await page.getByTestId("profile-edit-name").fill(PROFILE_MGMT_RENAMED);
  await page.getByTestId("profile-edit-desc").fill("Renamed by A5 e2e");
  await page.getByTestId("profile-save").click();
  await expect(page.getByText(PROFILE_MGMT_RENAMED).first()).toBeVisible();

  // ── Mark it as a default profile from the list ─────────────────────
  await page.goto("./dns/filter");
  const renamedRow = page
    .getByRole("row")
    .filter({ hasText: PROFILE_MGMT_RENAMED });
  const defaultToggle = renamedRow.getByRole("switch");
  await expect(defaultToggle).toHaveAttribute("aria-checked", "false");
  await defaultToggle.click();
  await expect(defaultToggle).toHaveAttribute("aria-checked", "true");

  // ── Delete it ──────────────────────────────────────────────────────
  await renamedRow.click();
  await page.getByTestId("profile-delete").click();
  await page.getByTestId("confirm-dialog-confirm").click();
  await expect(page).toHaveURL(/\/admin\/dns\/filter$/);
  await expect(
    page.getByRole("row").filter({ hasText: PROFILE_MGMT_RENAMED }),
  ).toHaveCount(0);
});
