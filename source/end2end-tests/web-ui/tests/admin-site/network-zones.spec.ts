import { expect, test } from "@playwright/test";

import {
  GUEST_DEVICE_MAC,
  GUEST_ZONE,
  IOT_ZONE,
  TEST_ZONE_NAME,
  TEST_ZONE_NAME_EDITED,
  TRUSTED_ZONE,
  assignDeviceToZone,
  cleanupZoneExceptions,
  cleanupZones,
  getQuarantine,
  seedZoneDevices,
  type SeededZoneDevice,
} from "../../fixtures/network-zones.js";

/**
 * Admin-site Network Zones coverage (epic #244 → #740) for `/admin/zones`.
 *
 * The page stacks three cards — the zone catalog (`ZonesCard`), the cross-zone
 * exceptions manager (`ZoneExceptionsCard`), and new-device quarantine
 * (`QuarantineSettingsCard`) — plus the per-device zone selector on the device
 * detail page (`DeviceZoneCard`). Each surface is driven through its real UI
 * flow. Two synthetic LAN members are seeded (one pinned to Trusted, one to
 * Guest) so the guard-rail, approval, and reassignment paths have real devices;
 * anything a test mutates it restores, so behaviour never depends on spec order.
 *
 * Runs in the `admin-site` project (seeded daemon + admin storageState).
 * Selectors follow the suite's `data-testid` convention (README.md): testids
 * locate, human-facing label/text is additionally asserted. Table rows are
 * located by their unique text and deletes go through the shared
 * `confirm-dialog-confirm` control.
 */

let guestDevice: SeededZoneDevice;

test.beforeAll(async () => {
  ({ guest: guestDevice } = await seedZoneDevices());
  // Clear any leftover manual zone / casting exception from a prior run so the
  // UI-create steps below start from a known-empty slate.
  await cleanupZones();
  await cleanupZoneExceptions(GUEST_ZONE, IOT_ZONE);
});

test("zone lifecycle: create, edit, and delete a manual zone", async ({
  page,
}) => {
  await page.goto("./zones");
  await expect(page.getByTestId("page-title")).toHaveText("Zones");

  // ── Create a manual zone (defaults: shared subnet, direct + tunnel) ──
  await page.getByTestId("zone-add").click();
  await page.getByTestId("zone-name").fill(TEST_ZONE_NAME);
  await page.getByTestId("zone-submit").click();

  const row = page.getByRole("row").filter({ hasText: TEST_ZONE_NAME });
  await expect(row).toBeVisible();
  await expect(row).toContainText("Shared subnet");

  // ── Edit: rename and switch the isolation stance ────────────────────
  await row.getByTestId("zone-row-menu").click();
  await page.getByTestId("zone-edit").click();
  await page.getByTestId("zone-name").fill(TEST_ZONE_NAME_EDITED);
  await page.getByTestId("zone-stance").click();
  await page.getByRole("option", { name: "Isolate members" }).click();
  await page.getByTestId("zone-submit").click();

  const editedRow = page
    .getByRole("row")
    .filter({ hasText: TEST_ZONE_NAME_EDITED });
  await expect(editedRow).toBeVisible();
  await expect(editedRow).toContainText("Isolate members");
  await expect(
    page.getByRole("row").filter({ hasText: TEST_ZONE_NAME }),
  ).toHaveCount(0);

  // ── Delete it ───────────────────────────────────────────────────────
  await editedRow.getByTestId("zone-row-menu").click();
  await page.getByTestId("zone-delete").click();
  await page.getByTestId("confirm-dialog-confirm").click();
  await expect(editedRow).toHaveCount(0);
});

test("seeded-zone guard rail: system and home zones offer no Delete", async ({
  page,
}) => {
  await page.goto("./zones");

  // Trusted is the system "home" (is_default) zone: its row menu offers Edit
  // but neither Delete (system + anchor) nor "Set as home" (already home).
  const trustedRow = page.getByRole("row").filter({ hasText: TRUSTED_ZONE });
  await trustedRow.getByTestId("zone-row-menu").click();
  await expect(page.getByTestId("zone-edit")).toBeVisible();
  await expect(page.getByTestId("zone-delete")).toHaveCount(0);
  await expect(page.getByTestId("zone-set-home")).toHaveCount(0);
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("zone-edit")).toHaveCount(0);

  // Guest is the system default-for-new zone and holds a seeded member — still
  // no Delete (system provenance alone forbids it).
  const guestRow = page.getByRole("row").filter({ hasText: GUEST_ZONE });
  await guestRow.getByTestId("zone-row-menu").click();
  await expect(page.getByTestId("zone-edit")).toBeVisible();
  await expect(page.getByTestId("zone-delete")).toHaveCount(0);
});

test("cross-zone exceptions: add and remove a casting allowance", async ({
  page,
}) => {
  await page.goto("./zones");

  await page.getByTestId("exception-add").click();

  // Guest → IoT, keeping the default one-click Casting preset.
  await page.getByTestId("exception-from").click();
  await page.getByRole("option", { name: `Zone: ${GUEST_ZONE}` }).click();
  await page.getByTestId("exception-to").click();
  await page.getByRole("option", { name: `Zone: ${IOT_ZONE}` }).click();
  await expect(page.getByTestId("exception-service")).toContainText("Casting");
  await page.getByTestId("exception-submit").click();

  // The allowance is a single row carrying both endpoints + the Casting pill.
  // A zone row only ever names one zone, so matching both names is unambiguous.
  const row = page
    .getByRole("row")
    .filter({ hasText: GUEST_ZONE })
    .filter({ hasText: IOT_ZONE });
  await expect(row).toBeVisible();
  await expect(row).toContainText("Casting");

  // ── Remove it ───────────────────────────────────────────────────────
  await row.getByTestId("exception-row-menu").click();
  await page.getByTestId("exception-delete").click();
  await page.getByTestId("confirm-dialog-confirm").click();
  await expect(row).toHaveCount(0);
});

test("new-device quarantine: toggle, default-for-new, and approve a device", async ({
  page,
}) => {
  // Put the Guest member back in the default-for-new zone so it appears pending,
  // and capture the current toggle so we can restore it.
  await assignDeviceToZone(guestDevice.id, GUEST_ZONE);
  const startEnabled = await getQuarantine();

  await page.goto("./zones");

  // The default-for-new zone is the seeded Guest zone.
  await expect(page.getByTestId("quarantine-default-for-new")).toContainText(
    GUEST_ZONE,
  );

  // Toggle notifications, then flip back to the original state.
  const toggle = page.getByTestId("quarantine-toggle");
  const startAttr = startEnabled ? "true" : "false";
  await expect(toggle).toHaveAttribute("aria-checked", startAttr);
  await toggle.click();
  await expect(toggle).toHaveAttribute(
    "aria-checked",
    startEnabled ? "false" : "true",
  );
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-checked", startAttr);

  // The seeded Guest member is awaiting review; approving reassigns it out of
  // the default-for-new zone (to the home zone), so its row disappears.
  const pendingRow = page
    .getByTestId("quarantine-pending-row")
    .filter({ hasText: GUEST_DEVICE_MAC });
  await expect(pendingRow).toBeVisible();
  await pendingRow.getByTestId("quarantine-approve").click();
  await expect(page.getByText("Device approved")).toBeVisible();
  await expect(pendingRow).toHaveCount(0);

  // Restore the canonical Trusted/Guest layout for later specs.
  await assignDeviceToZone(guestDevice.id, GUEST_ZONE);
});

test("device zone assignment: reassign from the device detail page", async ({
  page,
}) => {
  await assignDeviceToZone(guestDevice.id, GUEST_ZONE);

  await page.goto(`./devices/${guestDevice.id}`);

  // Read mode surfaces the current zone.
  await expect(page.getByTestId("device-zone-value")).toContainText(GUEST_ZONE);

  // Edit → pick a different zone → save.
  await page.getByTestId("device-zone-edit").click();
  await page.getByTestId("device-zone-select").click();
  await page.getByRole("option", { name: IOT_ZONE, exact: true }).click();
  await page.getByTestId("device-zone-save").click();

  await expect(page.getByTestId("device-zone-value")).toContainText(IOT_ZONE);

  // Restore.
  await assignDeviceToZone(guestDevice.id, GUEST_ZONE);
});
