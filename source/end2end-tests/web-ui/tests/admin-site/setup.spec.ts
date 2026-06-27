import { expect, test } from "@playwright/test";

import { ADMIN_PASSWORD, ADMIN_USERNAME } from "../../fixtures/seed.js";

/**
 * First-run setup wizard, happy path, driven through the real UI.
 *
 * Runs in the `admin-site-setup` project against the never-seeded
 * `wardnetd-ui-fresh` daemon (the shared `wardnetd-ui` is seeded to
 * `completed` by the `setup` project, so its wizard is gone). The walk
 * is also the harness's first real exercise of the secure-context path:
 * Step 1 auto-logs-in and step 2's advance is authenticated, so if the
 * `Secure` session cookie weren't stored this would fail at step 2.
 *
 * Selectors follow the suite's `data-testid` convention (README.md →
 * "Selector convention"): interactive controls are located by testid, while
 * each step transition is gated on the next step's heading — the heading
 * doubles as the human-facing label assertion. No pixel/colour assertions,
 * so the pending branding re-skin won't break it.
 */
test("first-run wizard creates the admin and reaches the dashboard", async ({
  page,
}) => {
  // baseURL ends in `/admin/`; SetupGuard redirects an unfinished wizard
  // to /admin/setup.
  await page.goto("./");
  await expect(page).toHaveURL(/\/admin\/setup$/);

  // Step 1 — create the first admin (unauthenticated).
  await expect(
    page.getByRole("heading", { name: /create admin account/i }),
  ).toBeVisible();
  await page.getByTestId("setup-username").fill(ADMIN_USERNAME);
  await page.getByTestId("setup-password").fill(ADMIN_PASSWORD);
  await page.getByTestId("setup-confirm-password").fill(ADMIN_PASSWORD);
  await page.getByTestId("setup-admin-submit").click();

  // Step 2 — confirm network (auto-logged-in from here on).
  await expect(
    page.getByRole("heading", { name: /confirm network/i }),
  ).toBeVisible();
  await page.getByTestId("setup-network-continue").click();

  // Step 3 — DHCP onboarding. Pick "locked router" to skip the
  // primary-mode LAN-probe gate (the probe isn't meaningful in the
  // isolated e2e LAN).
  await expect(
    page.getByRole("heading", { name: /dhcp onboarding/i }),
  ).toBeVisible();
  await page.getByTestId("setup-dhcp-mode-locked").check();
  await page.getByTestId("setup-dhcp-continue").click();

  // Step 4 — router MAC (skip; no probed MAC in the e2e env).
  await expect(
    page.getByRole("heading", { name: /router mac/i }),
  ).toBeVisible();
  await page.getByTestId("setup-router-mac-continue").click();

  // Step 5 — first VPN tunnel (skip).
  await expect(
    page.getByRole("heading", { name: /first vpn tunnel/i }),
  ).toBeVisible();
  await page.getByTestId("setup-tunnel-skip").click();

  // Step 6 — default routing policy (continue with the seeded default).
  await expect(
    page.getByRole("heading", { name: /default routing/i }),
  ).toBeVisible();
  await page.getByTestId("setup-policy-continue").click();

  // Step 7 — remote access (skip; DDNS/ACME isn't reachable in e2e).
  await expect(
    page.getByRole("heading", { name: /remote access/i }),
  ).toBeVisible();
  await page.getByTestId("setup-remote-access-skip").click();

  // Step 8 — done. Jump to the dashboard.
  await expect(page.getByRole("heading", { name: /all set/i })).toBeVisible();
  await page.getByTestId("setup-go-dashboard").click();

  // Lands on the authenticated app shell at the admin-site root
  // (`/admin`, the router basename — no trailing slash).
  await expect(page).toHaveURL(/\/admin\/?$/);
  await expect(page.getByRole("main")).toBeVisible();
});
