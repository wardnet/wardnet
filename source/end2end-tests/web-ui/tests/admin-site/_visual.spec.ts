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
