import { expect, test } from "@playwright/test";

import { setMyCapture } from "../../fixtures/user-app.js";

/**
 * user-app settings (epic #614 → C2, #627).
 *
 * The issue lists "theme/language; push-subscription UX"; the Settings tab
 * as built is the device's self-service controls: a DNS-capture toggle, the
 * admin-owned retention caps shown read-only, the device's own rule-change
 * requests, and a notifications placeholder (push is "coming soon"). This
 * spec covers what exists.
 *
 * Driven from the LAN runner. The capture toggle is the one stateful control
 * here — it flips the device's own `dns_capture_enabled` (device-keyed, no
 * login). The test normalises the starting state via the fixture and leaves
 * capture off. Selectors follow the suite's `data-testid` convention.
 */

test.afterAll(async () => {
  await setMyCapture(false);
});

test("the DNS-capture toggle flips the device's own capture flag", async ({
  page,
}) => {
  await setMyCapture(false);

  await page.goto("./settings");

  const toggle = page.getByTestId("capture-toggle");
  await expect(toggle).toBeVisible();
  await expect(toggle).toHaveAttribute("aria-checked", "false");

  // Enable — the mutation refetches `devices/me`, so the control reflects
  // the persisted flag.
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-checked", "true");

  // Disable again, leaving capture off for the other specs.
  await toggle.click();
  await expect(toggle).toHaveAttribute("aria-checked", "false");
});

test("the admin-owned retention caps are shown read-only", async ({ page }) => {
  await page.goto("./settings");

  // The caps come from the device record and are presented as an
  // explanatory note, with no control to change them.
  await expect(
    page.getByText(/This retention limit is set by your administrator/),
  ).toBeVisible();
});

test("the notifications placeholder advertises push as coming soon", async ({
  page,
}) => {
  await page.goto("./settings");

  await expect(page.getByText("Notifications", { exact: true })).toBeVisible();
  await expect(
    page.getByText("Push notifications about your device are coming soon."),
  ).toBeVisible();
});

test("the my-requests card is absent when the device has no requests", async ({
  page,
}) => {
  await page.goto("./settings");

  // `MyRequests` renders nothing when the device has filed no rule-change
  // requests (none are seeded here).
  await expect(page.getByTestId("my-requests")).toHaveCount(0);
});
