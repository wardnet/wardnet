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
    // The verify card's height differs a lot across its loading (180px) /
    // error (~150-170px) / loaded-map (~290px+) states, and a mask only
    // paints over whatever bounding box is current at capture time — it
    // doesn't reserve consistent space. Which state it lands in depends on
    // whether the BROWSER can reach the real ipapi.co over the CI runner's
    // actual internet egress (status.spec.ts documents this as
    // environment-dependent and deliberately doesn't assert on it either) —
    // so unlike every other masked element here, this one can't be waited
    // into a known state, it has to be made deterministic. Force the
    // failure branch by intercepting the request instead of hitting the
    // real network at all.
    await page.route("https://ipapi.co/**", (route) =>
      route.fulfill({ status: 500, body: "" }),
    );

    await page.goto("./");
    await expect(page.getByTestId("device-identity")).toBeVisible();
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
