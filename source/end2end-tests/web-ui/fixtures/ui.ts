import { type Page } from "@playwright/test";

/**
 * Fill and submit the shared admin login form (`@wardnet/web` LoginForm,
 * used by admin-site and admin-app). Assumes the login page is already
 * loaded; navigation after success is the page's responsibility.
 *
 * Selector-resilient (role/label, no copy/pixel assertions) so the
 * pending branding re-skin doesn't break it. Reused by the A2–A8 and
 * B-stage specs that need an authenticated session via the real UI.
 */
export async function loginViaUi(
  page: Page,
  username: string,
  password: string,
): Promise<void> {
  await page.getByLabel("Username").fill(username);
  await page.getByLabel("Password", { exact: true }).fill(password);
  await page.getByRole("button", { name: /log in/i }).click();
}
