import { expect, test } from "@playwright/test";
import { recoverSession } from "../../../fixtures/system.js";

/**
 * Admin-site Power page coverage (A8, #623): the "Restart daemon" flow —
 * confirm dialog → progress dialog → recovery. Runs in the `admin-site`
 * project (seeded daemon + admin storageState).
 *
 * Stateful: this really restarts the daemon, so it lives under
 * `tests/admin-site/stateful/`, which sorts after every read-only spec —
 * the suite runs single-worker in lexicographic path order, so the
 * destructive restart specs run last (honouring the issue's "order this
 * last" intent and keeping the blast radius off the read-only specs).
 *
 * The daemon runs as systemd PID 1 in the compose container and the
 * state volume persists, so it comes back and the progress dialog reaches
 * its terminal success state. Sessions are DB-backed (validated by
 * token-hash lookup, no in-memory signing key), so the cookie stays valid
 * across the restart and the expected outcome is "Daemon is back online";
 * `recoverSession` still handles the signed-out branch defensively.
 *
 * Only the restart-daemon control is exercised — NOT reboot/shutdown,
 * which wouldn't recover the container cleanly inside the suite.
 */

test("restarting the daemon shows progress and recovers", async ({ page }) => {
  // Restart + come-back can take a while; give the whole flow room.
  test.setTimeout(120_000);

  await page.goto("./power");
  await expect(page).toHaveURL(/\/admin\/power$/);
  await expect(page.getByTestId("page-title")).toHaveText("Power");

  // Trigger restart-daemon and confirm in the AlertModal.
  await page.getByTestId("power-restart-daemon").click();
  await page.getByTestId("power-restart-confirm").click();

  // The progress dialog opens (inescapable while in-progress).
  await expect(page.getByTestId("progress-dialog")).toBeVisible();

  // It walks to a terminal success state once the daemon is back.
  await expect(page.getByTestId("progress-title")).toHaveText(
    /Daemon is back online|Daemon restarted, session expired/,
    { timeout: 90_000 },
  );

  await recoverSession(page);
});
