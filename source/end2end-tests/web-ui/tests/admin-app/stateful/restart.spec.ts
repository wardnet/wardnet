import { expect, test } from "@playwright/test";

import { ADMIN_PASSWORD, ADMIN_USERNAME } from "../../../fixtures/seed.js";
import { loginViaUi } from "../../../fixtures/ui.js";

/**
 * admin-app PWA daemon-restart coverage (epic #614 → B2, #625): the System
 * page's restart flow really restarts the daemon and shows the busy overlay
 * until the app reconnects.
 *
 * Stateful: this really restarts the daemon, so — per the suite README,
 * "Spec ordering (stateful/)" — it lives under `tests/admin-app/stateful/`,
 * whose path sorts after every read-only admin-app spec (pages / pwa-shell /
 * smoke). That keeps a slow or flaky restart from cascading into them, the
 * same isolation the admin-site power.spec gets under `tests/admin-site/
 * stateful/`. Runs in the `admin-app` project — Pixel 7 viewport, seeded
 * daemon, admin storageState (see playwright.config.ts).
 *
 * The daemon runs as systemd PID 1 in the compose container with a
 * persistent state volume, so it comes back; DB-backed sessions stay valid
 * across the restart (the cookie survives), so the expected outcome is an
 * in-place recovery. The signed-out branch is handled defensively.
 */
test("the restart flow shows the busy overlay and recovers", async ({
  page,
}) => {
  test.setTimeout(120_000);

  await page.goto("./system");

  // Confirm restart.
  await page.getByTestId("system-restart-daemon").click();
  await expect(page.getByText("Restart daemon?", { exact: true })).toBeVisible();
  await page.getByTestId("confirm-dialog-confirm").click();

  // The full-screen busy overlay appears while the daemon is down, then
  // tears itself down once the daemon is back and the app reconnects.
  await expect(page.getByTestId("system-busy-overlay")).toBeVisible();
  await expect(page.getByTestId("system-busy-overlay")).toBeHidden({
    timeout: 90_000,
  });

  // Recover: normally the session survives and we're still in the shell; if
  // the daemon came back signed-out, re-authenticate so later specs keep an
  // authenticated context.
  if (/\/admin-app\/login/.test(page.url())) {
    await loginViaUi(page, ADMIN_USERNAME, ADMIN_PASSWORD);
    const declineBiometric = page.getByTestId("biometric-setup-decline");
    if (await declineBiometric.isVisible().catch(() => false)) {
      await declineBiometric.click();
    }
    await expect(page).toHaveURL(/\/admin-app\/?$/);
  } else {
    await page.goto("./system");
    await expect(page.getByTestId("system-status-pill")).toBeVisible();
  }
});
