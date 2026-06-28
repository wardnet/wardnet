import { expect, test } from "@playwright/test";

/**
 * Admin-site DNS page coverage (A4, #619): the enable/disable toggle and
 * status indicator, the upstream-servers editor (add + remove), and the
 * cache flush. Runs in the `admin-site` project (seeded daemon + admin
 * storageState). Selectors follow the suite's `data-testid` convention
 * (README.md → "Selector convention"): testids locate, and human-facing
 * label/text is additionally asserted where meaningful.
 *
 * Self-contained: each test re-establishes the state it needs and leaves
 * the DNS server enabled, so behaviour doesn't depend on spec order on
 * the shared single-worker daemon. Ad-blocking (blocklists / allowlist /
 * custom rules) lives on the filter-profile detail page and is covered
 * by adblocking.spec.ts.
 */

test("the status toggle enables and disables the DNS server", async ({
  page,
}) => {
  await page.goto("./dns");
  await expect(page).toHaveURL(/\/admin\/dns$/);
  await expect(page.getByTestId("page-title")).toHaveText("DNS");

  // The status banner renders and reflects a real running state. We don't
  // assert it equals "Running" right after enabling: the `running` pill
  // can lag a poll behind `enabled` (the daemon spawns the resolver
  // asynchronously), so the toggle's own `aria-checked` is the
  // deterministic signal — matching the DHCP spec's approach.
  const statusPill = page.getByTestId("dns-status-pill");
  await expect(statusPill).toBeVisible();
  await expect(statusPill).toHaveText(/Running|Stopped/);

  const toggle = page.getByTestId("dns-toggle");
  await expect(toggle).toBeVisible();

  // Normalise to enabled first, so both transitions below are
  // deterministic regardless of what an earlier spec left behind.
  if ((await toggle.getAttribute("aria-checked")) !== "true") {
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-checked", "true");
  }

  // Disable, then re-enable — the control drives both directions and we
  // leave the server enabled.
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-checked", "false");
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-checked", "true");
});

test("an upstream server can be added and then removed", async ({ page }) => {
  await page.goto("./dns");

  // Distinctive name/address so the row can be located unambiguously and
  // create can't collide with a seeded default upstream.
  const name = "e2e-upstream";
  const address = "9.9.9.9";

  // Open the inline add form (UDP is the default protocol).
  await page.getByTestId("upstream-add").click();
  await page.getByTestId("upstream-name").fill(name);
  await page.getByTestId("upstream-address").fill(address);
  await page.getByTestId("upstream-submit").click();

  // The new row appears in the table.
  const row = page.getByRole("row").filter({ hasText: name });
  await expect(row).toBeVisible();
  await expect(row).toContainText(address);

  // Remove it via the row's overflow menu, leaving the list as we found it.
  await row.getByTestId("upstream-row-menu").click();
  await page.getByTestId("upstream-remove").click();

  await expect(page.getByRole("row").filter({ hasText: name })).toHaveCount(0);
});

test("the cache can be flushed", async ({ page }) => {
  await page.goto("./dns");

  await page.getByTestId("dns-flush-cache").click();

  // Success surfaces as a toast reporting how many entries were cleared.
  await expect(page.getByText(/Cache flushed/i)).toBeVisible();
});
