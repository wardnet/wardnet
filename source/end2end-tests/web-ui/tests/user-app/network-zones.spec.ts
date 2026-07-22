import { expect, test } from "@playwright/test";

import { getLanDeviceId } from "../../fixtures/user-app.js";
import {
  GUEST_ZONE,
  assignDeviceToZone,
} from "../../fixtures/network-zones.js";

/**
 * user-app device-keyed Network Zones coverage (epic #244 → #740).
 *
 * The user-app is read-only about zones: the caller's own device shows which
 * zone it's on, but the owner can't change it — only an admin can. This spec
 * pins the LAN-proxy device (the one `/api/devices/me` resolves to on this
 * runner — see user-app.ts / lan-device.setup.ts) to a known zone and asserts
 * the Home tab's zone card renders that zone and offers no control to change it.
 *
 * Runs in the `user-app` project (Pixel 7, LAN-side runner, no storageState —
 * device-keyed). Selectors follow the suite's `data-testid` convention.
 */

let deviceId: string;

test.beforeAll(async () => {
  // Pin the device to Guest (a non-home zone → the "isolated" copy) so the
  // assertions don't depend on prior specs. Guest is also its natural
  // default-for-new zone, so this is a no-op restore for later specs.
  deviceId = await getLanDeviceId();
  await assignDeviceToZone(deviceId, GUEST_ZONE);
});

test("the home tab shows the caller's zone read-only", async ({ page }) => {
  await page.goto("./");

  await expect(page.getByText("Network zone")).toBeVisible();

  const card = page.getByTestId("zone-readonly");
  await expect(card).toBeVisible();
  // Non-home zone → the "isolated from the home network" copy naming the zone.
  await expect(card).toContainText(`You're on: ${GUEST_ZONE}`);
  await expect(card).toContainText("isolated from the home network");
  // The lock note stating only an admin can move the device between zones.
  await expect(card).toContainText(
    "Only your network administrator can change which zone your device is on.",
  );
});

test("the owner cannot change the zone", async ({ page }) => {
  await page.goto("./");

  // The zone card is display-only: no button, select, combobox, or toggle the
  // owner could use to reassign the zone (that endpoint is admin-gated).
  const card = page.getByTestId("zone-readonly");
  await expect(card).toBeVisible();
  await expect(card.getByRole("button")).toHaveCount(0);
  await expect(card.getByRole("combobox")).toHaveCount(0);
  await expect(card.locator("select, input, [role='switch']")).toHaveCount(0);
});
