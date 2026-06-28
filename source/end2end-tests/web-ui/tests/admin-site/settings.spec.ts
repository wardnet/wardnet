import { expect, test } from "@playwright/test";
import { setChannel } from "../../fixtures/system.js";

/**
 * Admin-site Settings page coverage (A8, #623): the read-only System
 * Information card and the Updates card (release channel, auto-update
 * toggle, "check for updates"). Runs in the `admin-site` project
 * (seeded daemon + admin storageState).
 *
 * Note on scope: the issue's original wording mentioned password change
 * and theme/language persistence, but the shipped Settings page has none
 * of that (the Account card is a "future release" placeholder, theme is
 * OS-driven, there is no language switcher). This spec covers what the
 * page actually exposes.
 *
 * Selectors follow the suite's `data-testid` convention (README.md →
 * "Selector convention"). Each test navigates fresh and normalises the
 * state it touches (channel left on stable, auto-update restored), so
 * behaviour doesn't depend on spec order on the shared single-worker
 * daemon.
 */

test("the system information card shows live daemon details", async ({
  page,
}) => {
  await page.goto("./settings");
  await expect(page).toHaveURL(/\/admin\/settings$/);
  await expect(page.getByTestId("page-title")).toHaveText("Settings");

  const info = page.getByTestId("system-info");
  await expect(info).toBeVisible();
  // Version is sourced from the daemon and must render a real value.
  await expect(page.getByTestId("system-version")).not.toHaveText("");
  // The read-only stat grid renders its labels.
  await expect(info).toContainText("Uptime");
  await expect(info).toContainText("Database size");
});

test("the release channel can be changed and persists across reload", async ({
  page,
}) => {
  await page.goto("./settings");

  const trigger = page.getByTestId("update-channel-trigger");
  await expect(trigger).toBeVisible();

  // Normalise to stable first so the transition below is deterministic.
  await setChannel(page, "Stable channel");
  await expect(trigger).toContainText("Stable channel");

  // Switch to beta and confirm it sticks across a reload — this exercises
  // the PUT /api/config/updates round-trip, not just local UI state.
  await setChannel(page, "Beta channel");
  await expect(trigger).toContainText("Beta channel");
  await page.reload();
  await expect(page.getByTestId("update-channel-trigger")).toContainText(
    "Beta channel",
  );

  // Leave the channel on stable for the next spec.
  await setChannel(page, "Stable channel");
  await expect(page.getByTestId("update-channel-trigger")).toContainText(
    "Stable channel",
  );
});

test("the auto-update toggle flips and persists across reload", async ({
  page,
}) => {
  await page.goto("./settings");

  const toggle = page.getByTestId("update-auto-toggle");
  await expect(toggle).toBeVisible();

  const initial = await toggle.getAttribute("aria-checked");
  const flipped = initial === "true" ? "false" : "true";

  // Flip to the opposite state and confirm it survives a reload (backed by
  // PUT /api/config/updates).
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-checked", flipped);
  await page.reload();
  await expect(page.getByTestId("update-auto-toggle")).toHaveAttribute(
    "aria-checked",
    flipped,
  );

  // Restore the original state.
  await page.getByTestId("update-auto-toggle").click();
  await expect(page.getByTestId("update-auto-toggle")).toHaveAttribute(
    "aria-checked",
    initial ?? "false",
  );
});

test("checking for updates settles to a terminal state", async ({ page }) => {
  await page.goto("./settings");

  const check = page.getByTestId("update-check");
  await expect(check).toBeEnabled();
  await check.click();

  // The offline test environment may not reach a release server, so we
  // don't assert a specific result — only that the action settles: the
  // button re-enables once the check resolves (success or failure) and
  // the page stays intact (no crash / error boundary).
  await expect(check).toBeEnabled({ timeout: 30_000 });
  await expect(page.getByTestId("page-title")).toHaveText("Settings");
});
