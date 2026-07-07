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
    // The verify card's height differs a lot across its loading (180px)
    // /error (~150-170px) / loaded-map (~290px+) states, and a mask only
    // paints over whatever bounding box is current at capture time — it
    // doesn't reserve consistent space. ipapi.co is unreachable from the
    // compose network (see status.spec.ts), so the query always ends up in
    // its one deterministic terminal state: error, after react-query's
    // single retry. Wait for that before shooting so every run masks the
    // same layout instead of racing loading-vs-error.
    await expect(
      page
        .getByTestId("verify-card")
        .getByText("Could not reach the geolocation service"),
    ).toBeVisible();

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
