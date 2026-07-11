import { expect, test, type Locator } from "@playwright/test";

import { seedReservation } from "../../fixtures/dhcp.js";

/**
 * Admin-site DHCP page coverage: the enable/disable toggle, the
 * pool-config editor (client-side range validation), and reservation
 * create/delete. Runs in the `admin-site` project (seeded daemon + admin
 * storageState). Selectors follow the suite's `data-testid` convention
 * (README.md → "Selector convention"): testids locate, and human-facing
 * label/role/text is additionally asserted where meaningful.
 *
 * Self-contained: each test re-establishes the DHCP state it needs and
 * leaves the server enabled, so behaviour doesn't depend on spec order
 * on the shared single-worker daemon.
 */

/**
 * Fill a segmented Ipv4Input/MacInput by setting each octet/segment
 * input directly. Filling per-segment (rather than typing the dotted
 * string) avoids the component's auto-tab: an octet > 25 auto-advances
 * focus, which would otherwise let a following "." skip the next
 * segment and produce a malformed value.
 */
async function fillSegments(field: Locator, parts: string[]): Promise<void> {
  const inputs = field.locator("input");
  for (let i = 0; i < parts.length; i++) {
    // eslint-disable-next-line security/detect-object-injection -- numeric loop index into the caller-supplied local string[] of IP/MAC segments
    await inputs.nth(i).fill(parts[i]);
  }
}

test("the status toggle enables and disables the DHCP server", async ({
  page,
}) => {
  await page.goto("./dhcp");
  await expect(page).toHaveURL(/\/admin\/dhcp$/);
  await expect(page.getByTestId("page-title")).toHaveText("DHCP");

  const toggle = page.getByTestId("dhcp-toggle");
  await expect(toggle).toBeVisible();

  // Normalise to enabled first, so both transitions below are
  // deterministic regardless of what an earlier spec left behind. The
  // `running` status pill can lag a poll interval behind `enabled`
  // (the daemon spawns the DHCP server asynchronously), so we assert on
  // the switch's own `aria-checked` rather than the pill text.
  if ((await toggle.getAttribute("aria-checked")) !== "true") {
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-checked", "true");
  }

  // Disable, then re-enable — the control drives both directions and we
  // leave the server enabled.
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-checked", "false");
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-checked", "true");
});

test("the pool editor validates the range client-side and persists a valid edit", async ({
  page,
}) => {
  await page.goto("./dhcp");

  await page.getByTestId("dhcp-config-edit").click();

  const poolStart = page.getByTestId("dhcp-pool-start");
  const poolEnd = page.getByTestId("dhcp-pool-end");
  const save = page.getByTestId("dhcp-config-save");

  // Inverted range — pool end below pool start.
  await fillSegments(poolStart, ["10", "91", "0", "200"]);
  await fillSegments(poolEnd, ["10", "91", "0", "100"]);

  // Client-side guard blocks the save with an inline message.
  const validation = page.getByTestId("dhcp-config-validation");
  await expect(validation).toBeVisible();
  await expect(validation).toContainText(
    /pool end must be at or after pool start/i,
  );
  await expect(save).toBeDisabled();

  // Fix the range to a valid one that still contains any seeded lease
  // (.100–.150): start .100, end .180. The message clears, save unlocks.
  await fillSegments(poolStart, ["10", "91", "0", "100"]);
  await fillSegments(poolEnd, ["10", "91", "0", "180"]);

  await expect(validation).toBeHidden();
  await expect(save).toBeEnabled();

  await save.click();

  // Back in read mode the new range is shown.
  const range = page.getByTestId("dhcp-config-pool-range");
  await expect(range).toBeVisible();
  await expect(range).toContainText("10.91.0.100");
  await expect(range).toContainText("10.91.0.180");
});

// Guarantee the entry table is non-empty so its toolbar (and the "Add
// reservation" button) renders — the table shows a placeholder with no
// toolbar when it has zero leases and zero reservations. Seeded via the
// API so the test doesn't depend on the leases spec running first.
test.beforeAll(async () => {
  await seedReservation("AA:BB:CC:DD:EE:50", "10.91.0.50");
});

test("a reservation can be created and then deleted", async ({ page }) => {
  await page.goto("./dhcp");

  // Below the dynamic pool (.100+) and unique, so create can't collide
  // with a lease or an existing reservation.
  const reservationIp = "10.91.0.60";
  const hostname = "e2e-printer";

  // Open the inline create form from the table toolbar.
  await page.getByTestId("dhcp-add-reservation").click();

  await fillSegments(page.getByTestId("dhcp-reservation-mac"), [
    "AA",
    "BB",
    "CC",
    "DD",
    "EE",
    "60",
  ]);
  await fillSegments(page.getByTestId("dhcp-reservation-ip"), [
    "10",
    "91",
    "0",
    "60",
  ]);
  await page.getByTestId("dhcp-reservation-hostname").fill(hostname);
  await page.getByTestId("dhcp-reservation-submit").click();

  // On success the form closes, the table switches to the reservations
  // group, and the new row appears.
  const row = page.getByRole("row").filter({ hasText: reservationIp });
  await expect(row).toBeVisible();
  await expect(row).toContainText(hostname);

  // Delete via the row's overflow menu → confirm dialog.
  await row.getByTestId("dhcp-entry-menu").click();
  await page.getByTestId("dhcp-entry-delete").click();
  await page.getByTestId("confirm-dialog-confirm").click();

  await expect(
    page.getByRole("row").filter({ hasText: reservationIp }),
  ).toHaveCount(0);
});
