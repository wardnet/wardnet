import { expect, test } from "@playwright/test";

/**
 * user-app PWA shell coverage (epic #614 → C1, #626): the web-app manifest
 * + installability criteria and the service-worker offline shell, for the
 * device-keyed user PWA served at `/app/` (a sibling scope to the
 * admin-app's `/admin-app/`, so the two PWAs install side by side; the
 * bare origin root permanently redirects into `/app/`).
 *
 * Runs in the `user-app` project — Pixel 7 viewport, driven by the LAN-side
 * runner over the `tls_proxy_lan` HTTPS origin (a secure context is required
 * for service workers to register; see playwright.config.ts / Caddyfile.lan).
 * Mirrors `admin-app/pwa-shell.spec.ts`, minus the tab-bar/login coverage
 * (the user-app has no login and its nav is out of scope for #626).
 */

test.describe("manifest + installability", () => {
  test("serves a manifest and meets the installable criteria", async ({
    page,
  }) => {
    await page.goto("./");

    // The document advertises a manifest; resolve its href against the page
    // URL and fetch it through the browser context so the self-signed TLS
    // proxy is honoured.
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

    // Core installability fields: a name, the /app/-scoped start URL + scope
    // (a SIBLING of /admin-app/ — Chrome refuses to install an app whose page
    // sits inside an already-installed app's scope, so the user-app must not
    // own the origin root), standalone display, and the 192/512 icons Chrome
    // requires to offer installation.
    expect(manifest.name).toBeTruthy();
    expect(manifest.id).toBe("/app/");
    expect(manifest.start_url).toBe("/app/");
    expect(manifest.scope).toBe("/app/");
    expect(manifest.display).toBe("standalone");
    const iconSizes: string[] = (manifest.icons ?? []).map(
      (icon: { sizes: string }) => icon.sizes,
    );
    expect(iconSizes).toContain("192x192");
    expect(iconSizes).toContain("512x512");

    // The remaining install-gate components: a registered service worker on
    // a secure origin. (headless Chromium does not reliably fire
    // `beforeinstallprompt`, so we verify the criteria that produce it.)
    await page.evaluate(() => navigator.serviceWorker.ready.then(() => {}));
    expect(new URL(page.url()).protocol).toBe("https:");
  });
});

test("service worker serves the cached app shell offline", async ({
  page,
  context,
}) => {
  await page.goto("./");

  // Wait for the SW to install/activate, then reload so it controls the
  // page (sw.ts does not call clients.claim(), so the first load is
  // uncontrolled).
  await page.evaluate(() => navigator.serviceWorker.ready.then(() => {}));
  await page.reload();
  await page.waitForFunction(() => Boolean(navigator.serviceWorker.controller));

  // Cut the network and reload. The SW's NavigationRoute must answer the
  // navigation from the precache; otherwise the reload would surface a
  // browser network-error page.
  await context.setOffline(true);
  try {
    await page.reload();

    // The title and #root both live in the cached static index.html, so
    // their presence proves the shell was served from the SW cache offline.
    await expect(page).toHaveTitle(/Wardnet/i);
    await expect(page.locator("#root")).toBeAttached();
  } finally {
    await context.setOffline(false);
  }
});
