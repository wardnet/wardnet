import { expect, test } from "@playwright/test";

import { ADMIN_PASSWORD, ADMIN_USERNAME } from "../../fixtures/seed.js";
import { loginViaUi } from "../../fixtures/ui.js";

/**
 * admin-app PWA shell coverage (epic #614 → B1): web-app manifest +
 * installability criteria, the service-worker offline shell, the mobile
 * tab-bar navigation, and `/admin-app/login` + admin-session reuse via the
 * project's shared `storageState`.
 *
 * Runs in the `admin-app` project — Pixel 7 viewport, seeded daemon, admin
 * `storageState` (see playwright.config.ts). Selectors follow the suite's
 * `data-testid` convention (README.md → "Selector convention"): testids
 * locate, human-facing label/role/text is additionally asserted where
 * meaningful.
 */

// Tab-bar entries that should navigate, keyed by testid → [URL, h1 | null].
// The dashboard (`tab-home`) renders status cards with no <h1>, so it asserts
// URL + active state only; every other tab carries a level-1 page heading.
const TABS: Array<[string, RegExp, string | null]> = [
  ["tab-devices", /\/admin-app\/devices$/, "Devices"],
  ["tab-tunnels", /\/admin-app\/tunnels$/, "Tunnels"],
  ["tab-dns", /\/admin-app\/dns$/, "DNS"],
  ["tab-system", /\/admin-app\/system$/, "System"],
  ["tab-home", /\/admin-app\/?$/, null],
];

test.describe("manifest + installability", () => {
  test("serves a manifest and meets the installable criteria", async ({
    page,
  }) => {
    await page.goto("./");

    // The document advertises a manifest; resolve its href against the page
    // URL (Vite rewrites the root-absolute href to the `/admin-app/` base in
    // the built output) and fetch it through the browser context so the
    // self-signed TLS proxy and any session cookie are honoured.
    const href = await page
      .locator('link[rel="manifest"]')
      .getAttribute("href");
    expect(href, "a <link rel=manifest> is present").toBeTruthy();
    const manifestUrl = new URL(href!, page.url()).toString();

    const res = await page.request.get(manifestUrl);
    expect(res.ok(), `manifest fetch ${manifestUrl} → ${res.status()}`).toBe(
      true,
    );
    const manifest = await res.json();

    // Core installability fields: a name, an in-scope start URL, standalone
    // display, and the 192/512 icons Chrome requires to offer installation.
    expect(manifest.name).toBeTruthy();
    expect(manifest.start_url).toContain("/admin-app/");
    expect(manifest.scope).toContain("/admin-app/");
    expect(manifest.display).toBe("standalone");
    const iconSizes: string[] = (manifest.icons ?? []).map(
      (icon: { sizes: string }) => icon.sizes,
    );
    expect(iconSizes).toContain("192x192");
    expect(iconSizes).toContain("512x512");

    // The remaining install-gate components: a registered service worker on a
    // secure origin. (The actual `beforeinstallprompt` event is asserted
    // indirectly — headless Chromium does not reliably fire it, so we verify
    // the criteria that produce it instead.)
    await page.evaluate(() => navigator.serviceWorker.ready.then(() => {}));
    expect(new URL(page.url()).protocol).toBe("https:");
  });
});

test("service worker serves the cached app shell offline", async ({
  page,
  context,
}) => {
  await page.goto("./");

  // Wait for the SW to install/activate, then reload so it controls the page:
  // sw.ts does not call clients.claim(), so the first load is uncontrolled.
  await page.evaluate(() => navigator.serviceWorker.ready.then(() => {}));
  await page.reload();
  await page.waitForFunction(() => Boolean(navigator.serviceWorker.controller));

  // Cut the network and reload. The SW's NavigationRoute must answer the
  // navigation from the precache (denylist `^/api`); otherwise the reload
  // would surface a browser network-error page.
  await context.setOffline(true);
  try {
    await page.reload();

    // The title and #root both live in the cached static index.html, so their
    // presence proves the shell was served from the SW cache while offline.
    await expect(page).toHaveTitle(/Wardnet/i);
    await expect(page.locator("#root")).toBeAttached();
  } finally {
    await context.setOffline(false);
  }
});

test("tab bar navigates between sections and marks the active tab", async ({
  page,
}) => {
  // Pixel 7 viewport (project default) — the mobile tab bar is the primary nav.
  await page.goto("./");

  for (const [testId, url, heading] of TABS) {
    await page.getByTestId(testId).click();
    await expect(page).toHaveURL(url);
    // react-router's NavLink marks the active tab via aria-current.
    await expect(page.getByTestId(testId)).toHaveAttribute(
      "aria-current",
      "page",
    );
    // Non-dashboard tabs land on a page with a level-1 heading (human-facing
    // label); the dashboard has none, so URL + active state is the contract.
    if (heading) {
      await expect(
        page.getByRole("heading", { level: 1, name: heading }),
      ).toBeVisible();
    }
  }
});

test("reuses the shared admin session on load", async ({ page }) => {
  // The admin-app project injects the admin `storageState`; landing on the
  // dashboard (not bounced to /login) proves the session is reused. The tab
  // bar only mounts inside the authenticated AppLayout.
  await page.goto("./");
  await expect(page).toHaveURL(/\/admin-app\/?$/);
  await expect(page.getByTestId("tab-home")).toBeVisible();
});

test.describe("login", () => {
  // These need a logged-out context, so override the project's storageState.
  test.use({ storageState: { cookies: [], origins: [] } });

  test("renders the login form and authenticates into the shell", async ({
    page,
  }) => {
    await page.goto("./login");

    const submit = page.getByTestId("login-submit");
    await expect(page.getByTestId("login-username")).toBeVisible();
    await expect(submit).toBeVisible();
    await expect(submit).toContainText(/log in/i);

    await loginViaUi(page, ADMIN_USERNAME, ADMIN_PASSWORD);

    // Chromium exposes `window.PublicKeyCredential`, so the app treats
    // biometrics as available and shows the one-time setup prompt after the
    // first login (nothing registered yet). Decline it to reach the dashboard.
    const declineBiometric = page.getByTestId("biometric-setup-decline");
    await expect(declineBiometric).toContainText(/not now/i);
    await declineBiometric.click();

    // A successful login lands on the dashboard with the authed tab bar.
    await expect(page).toHaveURL(/\/admin-app\/?$/);
    await expect(page.getByTestId("tab-home")).toBeVisible();
  });
});
