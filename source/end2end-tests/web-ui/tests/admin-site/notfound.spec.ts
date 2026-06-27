import { expect, test } from "@playwright/test";

/**
 * Admin-site 404 coverage. The `*` route sits outside the auth/setup
 * guards in App.tsx, so an unknown path renders NotFound directly.
 * Selectors follow the suite's `data-testid` convention (README.md →
 * "Selector convention").
 */
test("unknown route shows the 404 page and returns home", async ({ page }) => {
  await page.goto("./this-route-does-not-exist");

  const notFound = page.getByTestId("notfound-page");
  await expect(notFound).toBeVisible();
  // Human-facing copy alongside the testid locator.
  await expect(notFound).toContainText("404");
  await expect(notFound).toContainText(/page not found/i);

  // The CTA returns to the dashboard.
  await page.getByTestId("notfound-home").click();
  await expect(page).toHaveURL(/\/admin\/?$/);
  await expect(page.getByTestId("page-title")).toHaveText("Dashboard");
});
