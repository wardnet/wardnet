import { type Page } from "@playwright/test";

/**
 * Fill and submit the shared admin login form (`@wardnet/web` LoginForm,
 * used by admin-site and admin-app). Assumes the login page is already
 * loaded; navigation after success is the page's responsibility.
 *
 * Locators are `data-testid`-based per the suite's selector convention
 * (see README.md → "Selector convention"): testids give locator stability
 * across the pending branding re-skin / copy changes. This is a pure action
 * helper — label/role assertions belong in the calling spec. Reused by the
 * A2–A8 and B-stage specs that need an authenticated session via the real UI.
 */
export async function loginViaUi(
  page: Page,
  username: string,
  password: string,
): Promise<void> {
  await page.getByTestId("login-username").fill(username);
  await page.getByTestId("login-password").fill(password);
  await page.getByTestId("login-submit").click();
}
