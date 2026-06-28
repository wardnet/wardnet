import { expect, test, type Page } from "@playwright/test";
import { recoverSession, setChannel } from "../../../fixtures/system.js";

/**
 * Admin-site Backups page coverage (A8, #623): a real export → restore
 * round-trip. Runs in the `admin-site` project (seeded daemon + admin
 * storageState).
 *
 * Stateful: the restore restarts the daemon, so this lives under
 * `tests/admin-site/stateful/` (sorts after every read-only spec; the
 * single-worker suite runs it among the destructive specs at the end).
 *
 * Round-trip strategy (baseline-anchored so it's safe on the shared
 * single-worker daemon):
 *   1. Normalise a backed-up, UI-visible marker — the update channel — to
 *      `stable`, then export the current state to a bundle.
 *   2. Flip the marker to `beta` (a real change to system_config).
 *   3. Restore the exported bundle (preview → apply → daemon restart).
 *   4. Assert the marker reverted to `stable` — proving the restore
 *      genuinely replaced the database, not a no-op.
 * The net effect converges back to the exported baseline, and the bundle
 * captured the live (DB-backed) session, so the cookie stays valid after
 * the post-restore restart and other specs are unaffected.
 */

/** ≥12 chars to satisfy MIN_PASSPHRASE_LEN on both export and restore. */
const PASSPHRASE = "e2e-backup-passphrase";

/** Set the release channel and confirm it persisted across a reload — a
 *  fresh GET returning the new value proves the DB write committed. */
async function setChannelPersisted(page: Page, label: string): Promise<void> {
  await page.goto("./settings");
  await setChannel(page, label);
  await expect(page.getByTestId("update-channel-trigger")).toContainText(label);
  await page.reload();
  await expect(page.getByTestId("update-channel-trigger")).toContainText(label);
}

test("export and restore round-trip reverts a configuration change", async ({
  page,
}, testInfo) => {
  // Export, mutate, restore, restart, verify — give the flow room.
  test.setTimeout(180_000);

  // 1. Baseline: channel = stable, then export the current state.
  await setChannelPersisted(page, "Stable channel");

  await page.goto("./backups");
  await expect(page).toHaveURL(/\/admin\/backups$/);
  await expect(page.getByTestId("page-title")).toHaveText("Backups");

  await page.getByTestId("backup-export-open").click();
  await page.getByTestId("backup-passphrase").fill(PASSPHRASE);
  await page.getByTestId("backup-passphrase-confirm").fill(PASSPHRASE);
  const downloadPromise = page.waitForEvent("download");
  await page.getByTestId("backup-export-submit").click();
  const download = await downloadPromise;
  const bundlePath = testInfo.outputPath("backup.wardnet.age");
  await download.saveAs(bundlePath);

  // 2. Mutate the marker: flip channel to beta (persisted to the live DB).
  await setChannelPersisted(page, "Beta channel");

  // 3. Restore the baseline bundle.
  await page.goto("./backups");
  await page.getByTestId("backup-restore-open").click();
  await page.getByTestId("backup-bundle").setInputFiles(bundlePath);
  await page.getByTestId("restore-passphrase").fill(PASSPHRASE);
  await page.getByTestId("backup-preview-submit").click();

  // The preview manifest renders; the bundle is from this same daemon so
  // it must be compatible.
  const preview = page.getByTestId("restore-preview");
  await expect(preview).toBeVisible();
  await expect(preview).toContainText("Schema version");
  await expect(preview).not.toContainText("Bundle incompatible");

  // Apply → the daemon restarts to pick up the restored files.
  await page.getByTestId("backup-apply-submit").click();

  await expect(page.getByTestId("progress-dialog")).toBeVisible();
  await expect(page.getByTestId("progress-title")).toHaveText(
    /Daemon is back online|Daemon restarted, session expired/,
    { timeout: 90_000 },
  );
  await recoverSession(page);

  // 4. The restore reverted the marker: channel is back to stable.
  await page.goto("./settings");
  await expect(page.getByTestId("update-channel-trigger")).toContainText(
    "Stable channel",
  );
});
