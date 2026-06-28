import { expect, test } from "@playwright/test";

import { seedDhcpLease } from "../../fixtures/dhcp.js";

/**
 * Admin-site DHCP leases table. A real lease is seeded before the spec
 * runs: `seedDhcpLease` enables the daemon's DHCP server, narrows the
 * pool, and drives the `test_debian` LAN client through dhclient until
 * it holds an in-pool address (see fixtures/dhcp.ts). The table should
 * then surface that lease under the Leases group.
 *
 * Runs in the `admin-site` project (seeded daemon + admin storageState).
 * Selectors follow the suite's `data-testid` convention.
 */

let leasedIp: string;

test.beforeAll(async () => {
  leasedIp = await seedDhcpLease();
});

test("the leases table shows the seeded LAN-client lease", async ({ page }) => {
  await page.goto("./dhcp");
  await expect(page).toHaveURL(/\/admin\/dhcp$/);
  await expect(page.getByTestId("page-title")).toHaveText("DHCP");

  // Scope the table to dynamic leases.
  await page.getByTestId("dhcp-group-leases").click();

  const row = page.getByRole("row").filter({ hasText: leasedIp });
  await expect(row).toBeVisible();
  // A dynamic lease is tagged "Lease" and, while held, "Active".
  await expect(row).toContainText(/lease/i);
  await expect(row).toContainText(/active/i);
});
