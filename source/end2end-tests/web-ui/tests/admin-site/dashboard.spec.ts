import { expect, test } from "@playwright/test";

/**
 * Admin-site dashboard (Home) shell coverage.
 *
 * Runs in the `admin-site` project (seeded daemon + admin storageState),
 * so `./` lands on the authenticated dashboard. Selectors follow the
 * suite's `data-testid` convention (README.md → "Selector convention"):
 * tiles are located by testid, with their human-facing label asserted too.
 *
 * Only the always-present tiles are asserted. The system tiles
 * (uptime/CPU/memory/disk/DHCP) render conditionally off live daemon
 * status and aren't deterministic enough to gate the shell test on.
 */
test("dashboard renders its always-present stat tiles", async ({ page }) => {
  await page.goto("./");

  // We're on the dashboard, not bounced to login or setup.
  await expect(page).toHaveURL(/\/admin\/?$/);
  await expect(page.getByTestId("page-title")).toHaveText("Dashboard");

  // Always-present tiles: locate by testid, assert the visible label.
  const tiles: Array<[string, string | RegExp]> = [
    ["stat-devices", "Devices"],
    ["stat-tunnels", "Tunnels"],
    ["stat-dns-queries", /DNS queries/i],
    ["stat-blocked", /Blocked traffic/i],
  ];
  for (const [testId, label] of tiles) {
    const tile = page.getByTestId(testId);
    await expect(tile).toBeVisible();
    await expect(tile).toContainText(label);
  }
});
