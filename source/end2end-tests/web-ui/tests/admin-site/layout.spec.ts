import { expect, test } from "@playwright/test";

/**
 * Admin-site app shell (AppLayout) coverage: sidebar navigation and the
 * mobile drawer. Runs in the `admin-site` project (seeded daemon + admin
 * storageState). Selectors follow the suite's `data-testid` convention
 * (README.md → "Selector convention").
 *
 * NOTE: admin-site has no theme toggle — theme is OS-driven via
 * `prefers-color-scheme` → `data-theme` (see `src/hooks/useTheme.ts`),
 * with no UI control and no persistence. The epic's "theme toggle
 * persists across reload" assertion therefore has nothing to target here
 * and is intentionally omitted.
 */

// Desktop sidebar entries that should navigate, keyed by testid → [URL, title].
const NAV: Array<[string, RegExp, string]> = [
  ["nav-devices", /\/admin\/devices$/, "Devices"],
  ["nav-tunnels", /\/admin\/tunnels$/, "Tunnels"],
  ["nav-dns", /\/admin\/dns$/, "DNS"],
  ["nav-settings", /\/admin\/settings$/, "Settings"],
  ["nav-dashboard", /\/admin\/?$/, "Dashboard"],
];

test("sidebar links navigate and mark the active route", async ({ page }) => {
  await page.goto("./");

  for (const [testId, url, title] of NAV) {
    await page.getByTestId(testId).click();
    await expect(page).toHaveURL(url);
    // The page header reflects the destination (human-facing label)…
    await expect(page.getByTestId("page-title")).toHaveText(title);
    // …and react-router's NavLink marks the link active via aria-current.
    await expect(page.getByTestId(testId)).toHaveAttribute(
      "aria-current",
      "page",
    );
  }
});

test("mobile menu opens the navigation drawer and navigates", async ({
  page,
}) => {
  // Below the 768px breakpoint the desktop sidebar is hidden and the
  // hamburger trigger appears (see source/styles/styles.css).
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("./");

  // The drawer isn't mounted until the trigger is clicked.
  await expect(page.getByRole("dialog")).toBeHidden();
  await page.getByTestId("mobile-menu-trigger").click();

  const drawer = page.getByRole("dialog");
  await expect(drawer).toBeVisible();

  // The desktop sidebar also carries nav testids (hidden via CSS), so scope
  // to the drawer to keep the locator unambiguous.
  const devicesLink = drawer.getByTestId("nav-devices");
  await expect(devicesLink).toBeVisible();
  await expect(devicesLink).toContainText("Devices");

  await devicesLink.click();
  await expect(page).toHaveURL(/\/admin\/devices$/);
  // Selecting an item closes the drawer.
  await expect(page.getByRole("dialog")).toBeHidden();
});
