import { expect, test } from "@playwright/test";

import { UI_BASE_URL } from "../../fixtures/seed.js";

/**
 * user-app device identity (epic #614 → C1, #626).
 *
 * The user-app is device-keyed: the daemon resolves `GET /api/devices/me`
 * from the request's source IP (`middleware.rs::resolve_auth_context`).
 * This spec asserts both ends of that contract from one runner:
 *
 *  - From the LAN context (the project's baseURL = the `tls_proxy_lan`
 *    origin, whose IP `seed-lan-device` seeded as a discovered device), the
 *    app resolves a non-null device and renders its identity.
 *  - From a non-LAN context (the mgmt proxy origin, whose IP is not a
 *    device), the app shows the null-device fallback.
 *
 * Selectors follow the suite's `data-testid` convention (README →
 * "Selector convention"): testids locate, human-facing text is additionally
 * asserted where meaningful.
 */

test("renders the resolved device from the LAN context", async ({ page }) => {
  await page.goto("./");

  // Non-null device → the identity header renders with a level-1 heading
  // (the device's display name; its exact value depends on what discovery
  // recorded for the proxy, so we assert structure, not a literal name).
  const identity = page.getByTestId("device-identity");
  await expect(identity).toBeVisible();
  await expect(identity.getByRole("heading", { level: 1 })).toBeVisible();

  // The no-device fallback must NOT be shown.
  await expect(page.getByTestId("no-device")).toHaveCount(0);
});

test("shows the no-device fallback from a non-LAN context", async ({ page }) => {
  // Navigate to the mgmt proxy origin: the daemon sees that proxy's IP,
  // which is not a discovered device, so `devices/me` returns null.
  await page.goto(`${UI_BASE_URL}/app/`);

  const fallback = page.getByTestId("no-device");
  await expect(fallback).toBeVisible();
  await expect(
    page.getByRole("heading", { level: 1, name: "Device not detected" }),
  ).toBeVisible();

  // The resolved-device header must NOT be shown.
  await expect(page.getByTestId("device-identity")).toHaveCount(0);
});
