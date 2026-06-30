import { expect, test } from "@playwright/test";

import {
  SEED_DEVICE_IP,
  SEED_DEVICE_MAC,
  seedDiscoveredDevice,
  seedTunnel,
} from "../../fixtures/devices.js";

/**
 * admin-app PWA page coverage (epic #614 → B2, #625): each of the five
 * sections — dashboard, devices, tunnels, DNS, system — renders its content
 * and supports its primary actions in the mobile viewport, and an unknown
 * route redirects to "/".
 *
 * Runs in the `admin-app` project — Pixel 7 viewport, seeded daemon, admin
 * `storageState` (see playwright.config.ts). The shell (manifest, service
 * worker, tab-bar navigation, h1 headings, login) is covered by
 * pwa-shell.spec.ts; this spec deliberately exercises page *content* and
 * *actions* instead. Selectors follow the suite's `data-testid` convention
 * (README.md → "Selector convention"): testids locate, human-facing
 * label/role/text is additionally asserted where meaningful.
 *
 * State discipline (single-worker shared daemon): destructive primary
 * actions are fired and then restored so behaviour doesn't depend on spec
 * order —
 *   - DNS server / filtering toggles: flipped and flipped back, leaving the
 *     server enabled and filtering at its original state.
 *   - Device routing: set to a tunnel, then restored to Default on the
 *     seeded device.
 * The daemon-restart flow really restarts the daemon, so per the suite README
 * ("Spec ordering (stateful/)") it lives in its own stateful/restart.spec.ts —
 * which sorts last — rather than here, keeping its blast radius off these specs.
 * Two actions are NOT fired, for hard infra reasons documented at their tests:
 *   - Device *reboot* can't recover the compose container (mirrors the
 *     admin-site power.spec, which also avoids reboot/shutdown) — open + cancel.
 *   - Tunnel *rebuild* / *speed test* would run against the only seedable
 *     tunnel, a non-functional dummy (status down, throwaway WireGuard keys),
 *     so firing them is meaningless and flaky — presence + enabled is the
 *     contract.
 */

const TUNNEL_LABEL = "E2E Test Tunnel";

// Both fixtures are idempotent and shared with the admin-site Devices spec;
// by the time the admin-app project runs they are typically already present,
// so this resolves quickly. Hard preconditions — a missing device means
// packet capture isn't reaching wardnet_lan, which should fail the run.
test.beforeAll(async () => {
  await seedDiscoveredDevice();
  await seedTunnel(TUNNEL_LABEL);
});

test.describe("dashboard", () => {
  test("renders the status and summary cards and links into a section", async ({
    page,
  }) => {
    await page.goto("./");

    // The dashboard has no <h1> (pwa-shell asserts that contract); its
    // content is the status card plus the four summary cards.
    await expect(page.getByTestId("dashboard-status-card")).toBeVisible();
    await expect(page.getByTestId("dashboard-devices-card")).toBeVisible();
    await expect(page.getByTestId("dashboard-tunnels-card")).toBeVisible();
    await expect(page.getByTestId("dashboard-dns-queries-card")).toBeVisible();
    await expect(page.getByTestId("dashboard-blocked-card")).toBeVisible();

    // Each summary card is a link; the primary action is drilling into its
    // section. Tapping Devices navigates there.
    await page.getByTestId("dashboard-devices-card").click();
    await expect(page).toHaveURL(/\/admin-app\/devices$/);
  });
});

test.describe("devices", () => {
  test("lists the discovered LAN client", async ({ page }) => {
    await page.goto("./devices");

    // Unmanaged, hostname-less device: the row falls back to the MAC and
    // shows last_ip alongside its route label. Scope by IP, assert the MAC.
    const row = page.getByTestId("device-row").filter({ hasText: SEED_DEVICE_IP });
    await expect(row).toBeVisible();
    await expect(row).toContainText(SEED_DEVICE_MAC);
  });

  test("the filter pills narrow the visible set", async ({ page }) => {
    await page.goto("./devices");

    // The seeded device is listed under "All".
    await expect(page.getByTestId("device-filter-all")).toContainText("All");
    await expect(
      page.getByTestId("device-row").filter({ hasText: SEED_DEVICE_IP }),
    ).toBeVisible();

    // Each pill carries a count ("Label (N)"); selecting it must show exactly
    // that many rows. Asserting against the pill's own count keeps this
    // independent of whatever route an earlier spec left the device in (the
    // admin-site Devices spec leaves it on a tunnel, i.e. "On VPN").
    for (const f of ["all", "online", "vpn"] as const) {
      const pill = page.getByTestId(`device-filter-${f}`);
      await pill.click();
      // Re-read the pill's count and the row count together on each poll: the
      // "online" count is time-derived and a background refetch can move both,
      // so a one-shot snapshot could leave `toHaveCount` chasing a value that
      // no longer occurs. Comparing fresh reads converges once the page settles.
      await expect
        .poll(async () => {
          const label = (await pill.textContent()) ?? "";
          const expected = Number(label.match(/\((\d+)\)/)?.[1] ?? "-1");
          return (await page.getByTestId("device-row").count()) === expected;
        })
        .toBe(true);
    }
  });

  test("the device routing rule can be changed to a tunnel and restored", async ({
    page,
  }) => {
    await page.goto("./devices");

    const row = page.getByTestId("device-row").filter({ hasText: SEED_DEVICE_IP });

    // Tap the device → the routing sheet opens with the routing options.
    await row.click();
    const sheet = page.getByTestId("device-routing-sheet");
    await expect(sheet).toBeVisible();
    await expect(page.getByTestId("device-routing-default")).toBeVisible();
    await expect(page.getByTestId("device-routing-direct")).toBeVisible();

    // Pick the seeded tunnel (the option's accessible name carries the flag
    // glyph + label; match the label substring). The sheet closes on success
    // and the row's route label reflects the new target.
    await sheet.getByRole("button", { name: new RegExp(TUNNEL_LABEL) }).click();
    await expect(sheet).toBeHidden();
    await expect(row).toContainText(TUNNEL_LABEL);

    // Restore to Default so later specs see the device unrouted.
    await row.click();
    await expect(page.getByTestId("device-routing-sheet")).toBeVisible();
    await page.getByTestId("device-routing-default").click();
    await expect(page.getByTestId("device-routing-sheet")).toBeHidden();
    await expect(row).toContainText("Default");
  });
});

test.describe("tunnels", () => {
  test("renders the summary and the seeded tunnel card with its controls", async ({
    page,
  }) => {
    await page.goto("./tunnels");

    // Summary header with the up/total count.
    await expect(page.getByTestId("tunnels-summary")).toContainText("Tunnels Up");

    // The seeded tunnel's card surfaces its endpoint (from the imported
    // WireGuard config).
    const card = page.getByTestId("tunnel-card").filter({ hasText: TUNNEL_LABEL });
    await expect(card).toBeVisible();
    await expect(card).toContainText("198.51.100.1");

    // Primary actions are present and actionable. They are NOT fired: the
    // only seedable tunnel is a non-functional dummy (status down, throwaway
    // keys), so a real rebuild / speed test against it is meaningless and
    // flaky — the contract here is that the controls are wired and enabled.
    const speedTest = card.getByTestId("tunnel-speed-test-button");
    await expect(speedTest).toBeVisible();
    await expect(speedTest).toBeEnabled();
    const rebuild = card.getByTestId("tunnel-rebuild");
    await expect(rebuild).toBeVisible();
    await expect(rebuild).toBeEnabled();
  });
});

test.describe("dns", () => {
  test("renders status and supports toggles, filtering and cache flush", async ({
    page,
  }) => {
    await page.goto("./dns");

    // Status pill reflects a real running state (Running|Stopped).
    const statusPill = page.getByTestId("dns-status-pill");
    await expect(statusPill).toBeVisible();
    await expect(statusPill).toHaveText(/Running|Stopped/);

    // DNS server toggle — each flip goes through a confirm dialog. Normalise
    // to enabled, disable, then re-enable, leaving the server enabled.
    const dnsToggle = page.getByTestId("dns-toggle");
    if ((await dnsToggle.getAttribute("aria-checked")) !== "true") {
      await dnsToggle.click();
      await page.getByTestId("confirm-dialog-confirm").click();
      await expect(dnsToggle).toHaveAttribute("aria-checked", "true");
    }
    await dnsToggle.click();
    await expect(page.getByText("Disable DNS server?", { exact: true })).toBeVisible();
    await page.getByTestId("confirm-dialog-confirm").click();
    await expect(dnsToggle).toHaveAttribute("aria-checked", "false");
    await dnsToggle.click();
    await page.getByTestId("confirm-dialog-confirm").click();
    await expect(dnsToggle).toHaveAttribute("aria-checked", "true");

    // Filtering toggle — flip and flip back to the original state.
    const filterToggle = page.getByTestId("dns-filter-toggle");
    const initial = await filterToggle.getAttribute("aria-checked");
    const flipped = initial === "true" ? "false" : "true";
    await filterToggle.click();
    await page.getByTestId("confirm-dialog-confirm").click();
    await expect(filterToggle).toHaveAttribute("aria-checked", flipped);
    await filterToggle.click();
    await page.getByTestId("confirm-dialog-confirm").click();
    await expect(filterToggle).toHaveAttribute("aria-checked", initial ?? "false");

    // Top-blocked section renders (list or empty state).
    await expect(page.getByTestId("dns-top-blocked")).toBeVisible();

    // Cache flush surfaces a success toast.
    await page.getByTestId("dns-flush-cache").click();
    await expect(page.getByText(/Cache flushed/i)).toBeVisible();
  });
});

test.describe("system", () => {
  test("renders daemon health and account actions", async ({ page }) => {
    await page.goto("./system");

    const statusPill = page.getByTestId("system-status-pill");
    await expect(statusPill).toBeVisible();
    await expect(statusPill).toHaveText(/Running|Unreachable/);

    // Daemon health tiles.
    await expect(page.getByText("Version", { exact: true })).toBeVisible();
    await expect(page.getByText("Uptime", { exact: true })).toBeVisible();
    await expect(page.getByText("CPU", { exact: true })).toBeVisible();
    await expect(page.getByText("Memory", { exact: true })).toBeVisible();

    // Account section: the desktop-admin link points at /admin/, and log-out
    // is present (not fired — it would end the shared session).
    await expect(page.getByTestId("system-open-desktop")).toHaveAttribute(
      "href",
      "/admin/",
    );
    await expect(page.getByTestId("system-logout")).toBeVisible();
  });

  test("reboot asks for confirmation and can be cancelled", async ({ page }) => {
    await page.goto("./system");

    // Reboot is intentionally NOT fired: a real reboot wouldn't recover the
    // compose container cleanly (the admin-site power.spec avoids it for the
    // same reason). Verify the confirm dialog is wired, then cancel.
    await page.getByTestId("system-reboot-device").click();
    const dialog = page.getByTestId("confirm-dialog");
    await expect(dialog).toBeVisible();
    await expect(page.getByText("Reboot device?", { exact: true })).toBeVisible();
    await dialog.getByRole("button", { name: "Cancel" }).click();
    await expect(dialog).toBeHidden();
  });
});

test("an unknown route redirects to the dashboard", async ({ page }) => {
  // App.tsx routes `*` to <Navigate to="/" replace />, so any unknown path
  // under /admin-app/ lands back on the dashboard (the authed tab bar mounts
  // only inside AppLayout, so its presence confirms the redirect resolved).
  await page.goto("./this-route-does-not-exist");
  await expect(page).toHaveURL(/\/admin-app\/?$/);
  await expect(page.getByTestId("tab-home")).toBeVisible();
});
