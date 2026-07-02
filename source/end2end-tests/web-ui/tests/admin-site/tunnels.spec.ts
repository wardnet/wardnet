import { expect, test } from "@playwright/test";

import {
  TUNNEL_CONFIG,
  TUNNEL_IMPORT_LABEL,
  TUNNEL_LIST_DELETE_LABEL,
  deleteTestTunnels,
  importTunnel,
} from "../../fixtures/tunnels.js";

/**
 * Admin-site Tunnels lifecycle coverage (A7, #622).
 *
 * Covers the WireGuard `.conf` import flow (the "Manual" tab is a textarea
 * paste, not a file upload), seeing the tunnel in the list, drilling into its
 * detail page, and both delete paths: the detail page deletes immediately and
 * redirects, while the list page routes through a ConfirmDialog.
 *
 * Runs in the `admin-site` project (seeded daemon + admin storageState).
 * Selectors follow the suite's `data-testid` convention (README.md); the
 * tunnel components carried almost none before this issue, so this PR adds
 * them alongside the specs.
 */

// Tunnel labels are not UNIQUE in the daemon, but leftover `e2e-tunnel-*` rows
// from a prior run (or a mid-test failure) would make the list assertions
// ambiguous. Clear both labels this spec owns up front.
test.beforeAll(async () => {
  await deleteTestTunnels(TUNNEL_IMPORT_LABEL, TUNNEL_LIST_DELETE_LABEL);
});

test("tunnel: import .conf, see in list, open detail, delete from detail", async ({
  page,
}) => {
  // ── Import via the Manual (.conf paste) tab ────────────────────────
  await page.goto("./tunnels");
  await page.getByTestId("tunnel-add").click();
  // Manual is the default tab; select it explicitly so the test is robust
  // to a future default change.
  await page.getByTestId("tunnel-tab-manual").click();
  await page.getByTestId("tunnel-label").fill(TUNNEL_IMPORT_LABEL);
  await page.getByTestId("tunnel-country").fill("de");
  await page.getByTestId("tunnel-config").fill(TUNNEL_CONFIG);
  await page.getByTestId("tunnel-create-submit").click();

  // ── It appears in the list ─────────────────────────────────────────
  const card = page
    .getByTestId("tunnel-card")
    .filter({ hasText: TUNNEL_IMPORT_LABEL });
  await expect(card).toBeVisible();

  // ── Open the detail page ───────────────────────────────────────────
  await card.getByRole("link").click();
  await expect(page).toHaveURL(/\/admin\/tunnels\/[0-9a-f-]+$/);
  await expect(page.getByText(TUNNEL_IMPORT_LABEL).first()).toBeVisible();

  // ── Delete from detail (no confirm dialog) → redirect to list ──────
  await page.getByTestId("tunnel-detail-delete").click();
  await expect(page).toHaveURL(/\/admin\/tunnels$/);
  await expect(
    page.getByTestId("tunnel-card").filter({ hasText: TUNNEL_IMPORT_LABEL }),
  ).toHaveCount(0);
});

test("tunnel: delete from list via confirm dialog (cancel then confirm)", async ({
  page,
}) => {
  // Seed via the API so this test owns a deterministic row to delete.
  await importTunnel(TUNNEL_LIST_DELETE_LABEL);

  await page.goto("./tunnels");
  const card = page
    .getByTestId("tunnel-card")
    .filter({ hasText: TUNNEL_LIST_DELETE_LABEL });
  await expect(card).toBeVisible();

  // ── Cancel keeps the tunnel ────────────────────────────────────────
  await card.getByTestId("tunnel-delete").click();
  await page.getByTestId("confirm-dialog-cancel").click();
  // Wait for the dialog to fully tear down before reopening it, so the
  // reopen click can't race the Radix overlay's close transition.
  await expect(page.getByTestId("confirm-dialog-confirm")).toHaveCount(0);
  await expect(card).toBeVisible();

  // ── Confirm removes it ─────────────────────────────────────────────
  await card.getByTestId("tunnel-delete").click();
  await page.getByTestId("confirm-dialog-confirm").click();
  await expect(
    page
      .getByTestId("tunnel-card")
      .filter({ hasText: TUNNEL_LIST_DELETE_LABEL }),
  ).toHaveCount(0);
});
