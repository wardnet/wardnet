import { expect, test } from "@playwright/test";

import {
  getLanDeviceId,
  setAdminLocked,
  setMyRule,
} from "../../fixtures/user-app.js";

/**
 * user-app connection + route status (epic #614 → C2, #627).
 *
 * The issue calls this the "connection status card"; in the app the live
 * connection state lives in the shared `AppHeader` indicator, and the route
 * itself is shown on the Home tab's "Internet route" + "Verify your route"
 * cards. This spec asserts those, driven from the LAN runner.
 *
 * The verify card auto-fetches the device's public IP from an external
 * service (ipapi.co), which is not reachable in the compose stack — so we
 * assert only that the card mounts (title + refresh control), not any
 * geolocation result. Selectors follow the suite's `data-testid` convention.
 */

test.beforeAll(async () => {
  // The route card renders its editable form only when unlocked + a known
  // target; normalise so the assertions below don't depend on prior specs.
  const deviceId = await getLanDeviceId();
  await setAdminLocked(deviceId, false);
  await setMyRule({ type: "direct" });
});

test("the header shows the daemon connection as connected", async ({
  page,
}) => {
  await page.goto("./");

  // With the daemon reachable, the shared AppHeader indicator resolves to
  // the online state.
  const status = page.getByTestId("conn-status");
  await expect(status).toBeVisible();
  await expect(status).toHaveText(/Connected/);
});

test("the internet route card renders with the routing control", async ({
  page,
}) => {
  await page.goto("./");

  await expect(page.getByText("Internet route")).toBeVisible();
  // Unlocked → the device can pick its route, so the selector is present.
  await expect(page.getByTestId("routing-select")).toBeVisible();
});

test("the verify-route card mounts", async ({ page }) => {
  await page.goto("./");

  await expect(page.getByText("Verify your route")).toBeVisible();
  // The card always offers a manual re-check control, regardless of whether
  // the external geolocation lookup succeeds in this environment.
  await expect(
    page.getByRole("button", { name: "Refresh route check" }),
  ).toBeVisible();
});
