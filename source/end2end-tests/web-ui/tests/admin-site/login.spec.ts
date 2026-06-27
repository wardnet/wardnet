import { expect, test } from "@playwright/test";

import { ADMIN_PASSWORD, ADMIN_USERNAME } from "../../fixtures/seed.js";
import { loginViaUi } from "../../fixtures/ui.js";

/**
 * Login form coverage on the shared (already set-up) daemon.
 *
 * Selectors follow the suite's `data-testid` convention (README.md →
 * "Selector convention"): testids locate, and human-facing label/role/text
 * is additionally asserted where meaningful.
 *
 * The `admin-site` project injects an authenticated `storageState`; these
 * tests need a logged-out context, so override it to empty. The admin
 * itself was created by the `setup` project, so the good-credentials
 * cases use the shared `seed.ts` credentials.
 */
test.use({ storageState: { cookies: [], origins: [] } });

test("rejects bad credentials and stays on the login page", async ({
  page,
}) => {
  await page.goto("./login");
  await page.getByTestId("login-username").fill(ADMIN_USERNAME);
  await page.getByTestId("login-password").fill("wrong-password");
  await page.getByTestId("login-submit").click();

  // Locate the error by testid; assert it carries the alert role and the
  // human-facing failure copy.
  const error = page.getByTestId("login-error");
  await expect(error).toBeVisible();
  await expect(error).toHaveRole("alert");
  await expect(error).toContainText(/invalid username or password/i);
  await expect(page).toHaveURL(/\/admin\/login$/);
});

test("good credentials land on the dashboard", async ({ page }) => {
  await page.goto("./login");
  await loginViaUi(page, ADMIN_USERNAME, ADMIN_PASSWORD);

  // Dashboard is the router basename root — `/admin`, no trailing slash.
  await expect(page).toHaveURL(/\/admin\/?$/);
  await expect(page.getByRole("main")).toBeVisible();
});

test("preserves the deep-link target across login", async ({ page }) => {
  // A protected route while unauthenticated bounces to /login…
  await page.goto("./dns");
  await expect(page).toHaveURL(/\/admin\/login$/);

  // …and after login we return to the originally requested route,
  // not the default dashboard.
  await loginViaUi(page, ADMIN_USERNAME, ADMIN_PASSWORD);
  await expect(page).toHaveURL(/\/admin\/dns$/);
});
