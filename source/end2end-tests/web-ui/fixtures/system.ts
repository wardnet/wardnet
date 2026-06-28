import { expect, type Page } from "@playwright/test";
import { ADMIN_PASSWORD, ADMIN_USERNAME } from "./seed";
import { loginViaUi } from "./ui";

/**
 * Shared helpers for the admin-site System-area specs (settings / power /
 * backups, #623). Pure UI-action helpers in the spirit of `ui.ts` —
 * label/role assertions belong in the calling spec.
 */

/**
 * Open the release-channel select on the Settings page and pick the
 * option by its label ("Stable channel" / "Beta channel"). The select is
 * controlled by the update-status query, which `useUpdateConfig` refreshes
 * via `setQueryData` on success, so the trigger reflects the new value
 * once the mutation settles — the caller asserts that.
 */
export async function setChannel(page: Page, label: string): Promise<void> {
  await page.getByTestId("update-channel-trigger").click();
  await page.getByRole("option", { name: label }).click();
}

/**
 * Settle the RestartProgressDialog after a daemon restart: dismiss the
 * success state, or — if the session was invalidated — re-login via the
 * UI so the current spec (and any later spec) keeps an authenticated
 * context. Sessions are DB-backed (validated by token-hash lookup), so on
 * a restart that preserves/restores the database the cookie stays valid
 * and the signed-out branch shouldn't fire; we handle it defensively.
 */
export async function recoverSession(page: Page): Promise<void> {
  const signIn = page.getByTestId("restart-signin");
  if (await signIn.isVisible().catch(() => false)) {
    await signIn.click();
    await loginViaUi(page, ADMIN_USERNAME, ADMIN_PASSWORD);
    await expect(page).toHaveURL(/\/admin(\/|$)/);
  } else {
    await page.getByTestId("progress-dismiss").click();
    await expect(page.getByTestId("progress-dialog")).toBeHidden();
  }
}
