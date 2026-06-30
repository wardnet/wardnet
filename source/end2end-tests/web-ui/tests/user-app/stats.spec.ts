import { expect, test } from "@playwright/test";

import { setMyCapture } from "../../fixtures/user-app.js";

/**
 * user-app personal DNS stats (epic #614 → C2, #627).
 *
 * The Stats tab reads entirely from a device-local IndexedDB store fed by an
 * SSE stream of the daemon's captured DNS events. Real captured data can't
 * be deterministically produced for the LAN-proxy device in the compose
 * stack (it would require that device's DNS to transit the daemon's resolver
 * with capture on), so this spec covers the deterministic state machine
 * instead of populated charts:
 *
 *  - capture off (the default for a freshly discovered device) → the
 *    "DNS capture is off" empty state, whose CTA deep-links to Settings;
 *  - capture on, no events yet → the "Waiting for DNS activity" state.
 *
 * The capture flag is driven through the LAN proxy (device-keyed, by source
 * IP) and restored to off afterwards. Each test navigates fresh, so the
 * per-context IndexedDB starts empty.
 */

test.afterAll(async () => {
  await setMyCapture(false);
});

test("shows the capture-off empty state and links to Settings", async ({
  page,
}) => {
  await setMyCapture(false);

  await page.goto("./stats");

  const emptyState = page.getByTestId("stats-capture-off");
  await expect(emptyState).toBeVisible();
  await expect(
    page.getByRole("heading", { level: 1, name: "DNS capture is off" }),
  ).toBeVisible();

  // The CTA deep-links to the Settings tab, where the toggle lives.
  await page.getByTestId("stats-settings-link").click();
  await expect(page).toHaveURL(/\/settings$/);
  await expect(page.getByTestId("capture-toggle")).toBeVisible();
});

test("shows the waiting state once capture is enabled", async ({ page }) => {
  await setMyCapture(true);

  await page.goto("./stats");

  // Capture is on but no events have streamed in, so the page sits in its
  // waiting state rather than the off state or a populated dashboard.
  await expect(page.getByTestId("stats-waiting")).toBeVisible();
  await expect(
    page.getByRole("heading", { level: 1, name: "Waiting for DNS activity" }),
  ).toBeVisible();
  await expect(page.getByTestId("stats-capture-off")).toHaveCount(0);
});
