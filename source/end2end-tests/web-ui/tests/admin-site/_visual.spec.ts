import { expect, test } from "@playwright/test";

/**
 * Visual-regression baselines for admin-site (V1, #628).
 *
 * Ordering: the `_` filename prefix sorts before every feature spec (and
 * before `stateful/`) in Playwright's lexicographic file order, so these
 * snapshots observe the *pristine seeded* state — never another spec's
 * mutations or a post-restart daemon (see README → "Spec ordering").
 *
 * Framing: screenshots are viewport-only (Playwright's default), so
 * below-the-fold live content — the RecentErrorsCard and the live log
 * stream on the dashboard — never enters frame. The dynamic tiles that ARE
 * in view are masked; the surrounding layout is the actual contract.
 *
 * Determinism knobs (animations off, caret hidden, pixel tolerance) are set
 * once in `playwright.config.ts` → `expect.toHaveScreenshot`. Baselines live
 * under `snapshots/` and are regenerated with `make e2e-ui-update-snapshots`
 * (README → "Visual regression snapshots").
 */
test.describe("admin-site visual", { tag: "@visual" }, () => {
  test("dashboard", async ({ page }) => {
    await page.goto("./");
    await expect(page.getByTestId("page-title")).toHaveText("Dashboard");
    // Gate on the last-loading tiles so the grid is fully populated before
    // the shot: the system tiles render off live daemon status and the DHCP
    // tile off dhcp status, both behind `{status && …}`. Waiting for them
    // guarantees a stable layout to mask against (a half-rendered grid would
    // shift). stat-devices / stat-tunnels are always present.
    await expect(page.getByTestId("stat-uptime")).toBeVisible();
    await expect(page.getByTestId("stat-dhcp")).toBeVisible();
    // The premium-setup project's register_network call kicks off a
    // BACKGROUND cert-issuance attempt against wardnet_cloud_mock, which
    // always ends up "failed" (the mock never actually terminates an ACME
    // order) — but not instantly. Screenshotting mid-transition would catch
    // a *different* remote-access-banner state each run — absent (idle),
    // "Issuing certificate…", or "Certificate issuance failed" — each a
    // different height/absence, none of which the mask (sized to whatever's
    // actually rendered) can paper over. Wait for the one state this always
    // settles to before shooting, so every run masks the same layout.
    // Scoped to the banner itself — the same "certificate issuance failed"
    // text also streams into the recent-errors card and the live log widget
    // below, which a page-wide getByText would also match.
    await expect(
      page
        .getByTestId("dashboard-remote-access-banner")
        .getByText("Certificate issuance failed"),
    ).toBeVisible();

    await expect(page).toHaveScreenshot("dashboard.png", {
      // Live / time-varying values. stat-devices and stat-tunnels are
      // seed-deterministic, so they stay visible and actually get compared.
      mask: [
        page.getByTestId("stat-dns-queries"),
        page.getByTestId("stat-blocked"),
        page.getByTestId("stat-uptime"),
        page.getByTestId("stat-cpu"),
        page.getByTestId("stat-memory"),
        page.getByTestId("stat-disk"),
        page.getByTestId("stat-dhcp"),
        // The remote-access provisioning banner surfaces the daemon's real
        // (mocked-cloud) ACME attempt, which fails in the e2e harness with an
        // upstream error string that isn't byte-for-byte stable across runs.
        // Its height IS stable (RemoteAccessProgress clamps the error text to
        // 2 lines), so masking it here doesn't shift anything below.
        page.getByTestId("dashboard-remote-access-banner"),
        // Below the stat grid but leaking into the viewport bottom: both
        // stream live, per-run content (error rows with timestamps; the live
        // log tail).
        page.getByTestId("dashboard-recent-errors"),
        page.getByTestId("dashboard-log-widget"),
      ],
    });
  });

  test("ad-blocking", async ({ page }) => {
    // The "Ad Blocking" surface is the DNS Filtering page (/dns/filter).
    await page.goto("./dns/filter");
    await expect(page.getByTestId("page-title")).toHaveText("DNS Filtering");
    await expect(page.getByTestId("filter-add-profile")).toBeVisible();

    await expect(page).toHaveScreenshot("ad-blocking.png", {
      // Per-profile rule-count pills aggregate live blocklist entry counts.
      // One badge group renders per profile row, so this locator matches
      // several elements — Playwright masks them all.
      mask: [page.getByTestId("profile-count-badges")],
    });
  });

  test.describe(() => {
    // The login page needs a logged-out context; the admin-site project
    // injects an authenticated storageState (mirrors login.spec.ts).
    test.use({ storageState: { cookies: [], origins: [] } });

    test("login", async ({ page }) => {
      await page.goto("./login");
      await expect(page.getByTestId("login-submit")).toBeVisible();
      // Fully static form on first paint — the only conditional bits
      // (login-error, the "Signing in…" label) appear only after a submit.
      await expect(page).toHaveScreenshot("login.png");
    });
  });
});
