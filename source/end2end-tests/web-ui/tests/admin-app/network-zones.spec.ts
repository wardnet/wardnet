import { expect, test } from "@playwright/test";

import {
  GUEST_DEVICE_IP,
  GUEST_DEVICE_MAC,
  GUEST_ZONE,
  TRUSTED_ZONE,
  assignDeviceToZone,
  seedZoneDevices,
  type SeededZoneDevice,
} from "../../fixtures/network-zones.js";

/**
 * admin-app PWA Network Zones coverage (epic #244 → #740): the two mobile zone
 * flows on `/admin-app/devices` — the quick zone-reassignment sheet and the
 * "new devices awaiting review" approval inbox.
 *
 * Runs in the `admin-app` project — Pixel 7 viewport, seeded daemon, admin
 * `storageState` (see playwright.config.ts). The shared seeded Guest member (in
 * the default-for-new zone) drives both flows; each test restores it so the
 * layout is stable across spec order. Selectors follow the suite's
 * `data-testid` convention; the per-zone / per-device rows carry a shared
 * testid, so they're scoped by the device's IP/MAC text.
 */

let guestDevice: SeededZoneDevice;

test.beforeAll(async () => {
  ({ guest: guestDevice } = await seedZoneDevices());
});

test("the routing sheet reassigns a device's zone", async ({ page }) => {
  await assignDeviceToZone(guestDevice.id, GUEST_ZONE);
  await page.goto("./devices");

  const row = page
    .getByTestId("device-row")
    .filter({ hasText: GUEST_DEVICE_IP });
  await expect(row).toBeVisible();

  // Tap the device → the sheet opens with the Zone section.
  await row.click();
  const sheet = page.getByTestId("device-routing-sheet");
  await expect(sheet).toBeVisible();

  // Pick the Trusted zone (each zone option shares one testid; scope by name).
  await sheet
    .getByTestId("device-zone-option")
    .filter({ hasText: TRUSTED_ZONE })
    .click();

  // Success reassigns the zone, toasts, and closes the sheet.
  await expect(page.getByText("Zone updated")).toBeVisible();
  await expect(sheet).toBeHidden();

  // Restore for the approval flow below.
  await assignDeviceToZone(guestDevice.id, GUEST_ZONE);
});

test("the new-device inbox approves a quarantined device", async ({ page }) => {
  await assignDeviceToZone(guestDevice.id, GUEST_ZONE);
  await page.goto("./devices");

  // The seeded Guest member sits in the default-for-new zone, so it surfaces in
  // the "awaiting review" inbox.
  await expect(page.getByTestId("new-devices-section")).toBeVisible();
  const entry = page
    .getByTestId("new-device")
    .filter({ hasText: GUEST_DEVICE_MAC });
  await expect(entry).toBeVisible();

  // Approve → reassigns to the home zone, so the entry leaves the inbox.
  await entry.getByTestId("new-device-approve").click();
  await expect(page.getByText("Device approved")).toBeVisible();
  await expect(entry).toHaveCount(0);

  // Restore the canonical Guest membership.
  await assignDeviceToZone(guestDevice.id, GUEST_ZONE);
});
