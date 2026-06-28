import { expect, test } from "@playwright/test";

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

  // Inverted range — pool end below pool start. The segmented Ipv4Input
  // auto-tabs between octets on each "." so typing the dotted-quad fills
  // the whole field and overwrites the pre-filled value.
  await poolStart.click();
  await poolStart.pressSequentially("10.91.0.200");
  await poolEnd.click();
  await poolEnd.pressSequentially("10.91.0.100");

  // Client-side guard blocks the save with an inline message.
  const validation = page.getByTestId("dhcp-config-validation");
  await expect(validation).toBeVisible();
  await expect(validation).toContainText(
    /pool end must be at or after pool start/i,
  );
  await expect(save).toBeDisabled();

  // Fix the range to a valid one that still contains any seeded lease
  // (.100–.150): start .100, end .180. The message clears, save unlocks.
  await poolStart.click();
  await poolStart.pressSequentially("10.91.0.100");
  await poolEnd.click();
  await poolEnd.pressSequentially("10.91.0.180");

  await expect(validation).toBeHidden();
  await expect(save).toBeEnabled();

  await save.click();

  // Back in read mode the new range is shown.
  const range = page.getByTestId("dhcp-config-pool-range");
  await expect(range).toBeVisible();
  await expect(range).toContainText("10.91.0.100");
  await expect(range).toContainText("10.91.0.180");
});

test("a reservation can be created and then deleted", async ({ page }) => {
  await page.goto("./dhcp");

  // Below the dynamic pool (.100+) and unique, so create can't collide
  // with a lease or an existing reservation.
  const reservationIp = "10.91.0.60";
  const hostname = "e2e-printer";

  // Open the inline create form from the table toolbar.
  await page.getByTestId("dhcp-add-reservation").click();

  await page.getByTestId("dhcp-reservation-mac").click();
  await page
    .getByTestId("dhcp-reservation-mac")
    .pressSequentially("AABBCCDDEE60");
  await page.getByTestId("dhcp-reservation-ip").click();
  await page
    .getByTestId("dhcp-reservation-ip")
    .pressSequentially(reservationIp);
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
