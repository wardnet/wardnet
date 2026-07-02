import { expect, test } from "@playwright/test";

/**
 * Visual-regression baseline for the user-app Home page (V1, #628).
 *
 * Driven by the LAN runner over `tls_proxy_lan`, so `devices/me` resolves to
 * the seeded LAN device (C1, #626) and Home renders the device view. Runs
 * first (`_` prefix) on pristine seeded state, before `routing.spec.ts`
 * mutates the device's rule. Pixel 7 viewport. Determinism knobs come from
 * `playwright.config.ts` (see the admin-site `_visual.spec.ts` header).
 */
test.describe("user-app visual", { tag: "@visual" }, () => {
  test("home", async ({ page }) => {
    await page.goto("./");
    await expect(page.getByTestId("device-identity")).toBeVisible();

    await expect(page).toHaveScreenshot("home.png", {
      // The Verify card is a live Leaflet map + IP-geolocation panel whose
      // state (loading / error / loaded) depends on network reachability;
      // route-status is a live tunnel pill (present only when a tunnel is
      // active). Mask both. `.leaflet-container` is masked too as a backstop
      // for the map tiles in case the Card testid isn't forwarded.
      mask: [
        page.getByTestId("verify-card"),
        page.getByTestId("route-status"),
        page.locator(".leaflet-container"),
      ],
    });
  });
});
