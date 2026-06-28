import { expect, test } from "@playwright/test";

import {
  SEED_DEVICE_IP,
  SEED_DEVICE_MAC,
  seedDiscoveredDevice,
  seedTunnel,
} from "../../fixtures/devices.js";

/**
 * Admin-site Devices coverage (A6, #621): the device table lists a
 * discovered LAN client, the drill-in detail page surfaces its fields,
 * and the routing rule can be changed to a tunnel.
 *
 * A synthetic device is seeded before the spec runs: `seedDiscoveredDevice`
 * makes the `test_debian` test-agent emit an ARP frame for a controlled
 * in-subnet MAC/IP, then polls `/api/devices` until the daemon's
 * packet-capture discovery materializes the row (see fixtures/devices.ts).
 * A tunnel is imported so the routing selector offers a non-Direct option.
 * Both are hard preconditions — if the device never appears (packet
 * capture not reaching wardnet_lan), `beforeAll` throws and the spec fails
 * rather than silently skipping.
 *
 * Runs in the `admin-site` project (seeded daemon + admin storageState).
 * Selectors follow the suite's `data-testid` convention (README.md →
 * "Selector convention"): testids locate, human-facing label/text is
 * additionally asserted. Self-contained: the routing test sets the rule it
 * asserts, so behaviour doesn't depend on spec order.
 */

let deviceId: string;
const TUNNEL_LABEL = "E2E Test Tunnel";

test.beforeAll(async () => {
  const device = await seedDiscoveredDevice();
  deviceId = device.id;
  await seedTunnel(TUNNEL_LABEL);
});

test("the devices table lists the discovered LAN client", async ({ page }) => {
  await page.goto("./devices");
  await expect(page).toHaveURL(/\/admin\/devices$/);
  await expect(page.getByTestId("page-title")).toHaveText("Devices");

  // Unmanaged, hostname-less device: the Device column falls back to the
  // MAC (deviceDisplayName → name ?? hostname ?? mac) and the IP column
  // shows last_ip. Scope to the row by IP, then assert the MAC.
  const row = page.getByRole("row").filter({ hasText: SEED_DEVICE_IP });
  await expect(row).toBeVisible();
  await expect(row).toContainText(SEED_DEVICE_MAC);
});

test("the device detail page shows the device fields", async ({ page }) => {
  await page.goto("./devices");

  // Row-click navigates to the detail route.
  await page.getByRole("row").filter({ hasText: SEED_DEVICE_IP }).click();
  await expect(page).toHaveURL(new RegExp(`/admin/devices/${deviceId}$`));

  // Identity card surfaces the MAC; the network card / header surface the IP.
  // Both also appear in the detail header, so match the first occurrence.
  await expect(page.getByText(SEED_DEVICE_MAC).first()).toBeVisible();
  await expect(page.getByText(SEED_DEVICE_IP).first()).toBeVisible();
});

test("the device routing rule can be changed to a tunnel", async ({ page }) => {
  await page.goto(`./devices/${deviceId}`);
  await expect(page).toHaveURL(new RegExp(`/admin/devices/${deviceId}$`));

  // Enter edit mode on the Settings card.
  await page.getByTestId("device-settings-edit").click();

  // Open the routing dropdown and pick the seeded tunnel (Radix renders
  // options in a portal with role="option"; the flag glyph is aria-hidden
  // so the accessible name is the bare label).
  await page.getByTestId("device-settings-routing").click();
  await page.getByRole("option", { name: TUNNEL_LABEL }).click();

  // Save (label is "To Managed Device" while the device is unmanaged — we
  // locate by testid regardless).
  await page.getByTestId("device-settings-save").click();

  // Back in read mode the routing value reflects the new tunnel target.
  const routingValue = page.getByTestId("device-settings-routing-value");
  await expect(routingValue).toBeVisible();
  await expect(routingValue).toContainText(/via tunnel/i);
  await expect(routingValue).toContainText(TUNNEL_LABEL);
});
