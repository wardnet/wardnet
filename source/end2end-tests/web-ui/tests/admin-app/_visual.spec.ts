import { expect, test } from "@playwright/test";

/**
 * Visual-regression baseline for the admin-app dashboard (V1, #628).
 *
 * Runs first (`_` prefix) on pristine seeded state, before the
 * `stateful/restart.spec.ts` daemon restart. Pixel 7 viewport (project
 * device). The dashboard is almost entirely live cards (status/uptime,
 * device online counts, tunnel byte counters, DNS query/blocked totals with
 * sparklines), each carrying only a card-root testid — so all five are
 * masked and the snapshot guards the mobile shell + card-grid layout.
 * Determinism knobs come from `playwright.config.ts` (see the admin-site
 * `_visual.spec.ts` header).
 */
test.describe("admin-app visual", { tag: "@visual" }, () => {
  test("dashboard", async ({ page }) => {
    await page.goto("./");
    await expect(page.getByTestId("dashboard-status-card")).toBeVisible();
    await expect(page.getByTestId("dashboard-blocked-card")).toBeVisible();

    await expect(page).toHaveScreenshot("dashboard.png", {
      mask: [
        page.getByTestId("dashboard-status-card"),
        page.getByTestId("dashboard-devices-card"),
        page.getByTestId("dashboard-tunnels-card"),
        page.getByTestId("dashboard-dns-queries-card"),
        page.getByTestId("dashboard-blocked-card"),
      ],
    });
  });
});
