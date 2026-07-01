import { expect, test } from "@playwright/test";

import { seedTunnel } from "../../fixtures/devices.js";
import {
  getLanDeviceId,
  setAdminLocked,
  setMyRule,
} from "../../fixtures/user-app.js";

/**
 * user-app self-service routing (epic #614 → C2, #627).
 *
 * The Home tab lets a device pick its own internet route unless an admin has
 * locked the rule. Both halves of that contract are covered from the LAN
 * runner (baseURL = the `tls_proxy_lan` origin, whose IP is the seeded
 * device):
 *
 *  - Unlocked: open the routing selector, pick the seeded tunnel, save, and
 *    assert the card reflects the new target.
 *  - Locked (admin sets `admin_locked` via `PUT /api/devices/{id}`): the
 *    selector/save controls are replaced by a read-only locked notice.
 *
 * A tunnel is imported so the selector offers a non-Direct option (it is
 * disabled with zero tunnels). The spec sets its own lock/rule preconditions
 * and restores them after, so it never depends on file order. Selectors
 * follow the suite's `data-testid` convention (README → "Selector
 * convention").
 */

let deviceId: string;
const TUNNEL_LABEL = "E2E Test Tunnel";

test.beforeAll(async () => {
  await seedTunnel(TUNNEL_LABEL);
  deviceId = await getLanDeviceId();
  // Known starting point: unlocked, routed Direct.
  await setAdminLocked(deviceId, false);
  await setMyRule({ type: "direct" });
});

test.afterAll(async () => {
  // Leave the device as we found it for the other user-app specs.
  await setAdminLocked(deviceId, false);
  await setMyRule({ type: "direct" });
});

test("the device can change its own route to a tunnel", async ({ page }) => {
  await setAdminLocked(deviceId, false);
  await setMyRule({ type: "direct" });

  await page.goto("./");

  // The unlocked card shows the editable selector, not the locked notice.
  await expect(page.getByTestId("routing-locked")).toHaveCount(0);
  const selector = page.getByTestId("routing-select");
  await expect(selector).toBeVisible();

  // Open the dropdown and pick the seeded tunnel. Radix renders options in
  // a portal with role="option"; the flag glyph is aria-hidden, so the
  // accessible name is the bare label.
  await selector.click();
  await page.getByRole("option", { name: TUNNEL_LABEL }).click();

  // The trigger now reflects the chosen tunnel; save it.
  await expect(selector).toContainText(TUNNEL_LABEL);
  await page.getByTestId("routing-save").click();

  // After the rule is persisted and `devices/me` refetches, the active
  // tunnel surfaces its status pill and the selector keeps the tunnel
  // selected.
  await expect(page.getByTestId("route-status")).toBeVisible();
  await expect(page.getByTestId("routing-select")).toContainText(TUNNEL_LABEL);
});

test("a route locked by the admin is shown read-only", async ({ page }) => {
  await setAdminLocked(deviceId, true);

  await page.goto("./");

  // The locked notice replaces the editable controls.
  const locked = page.getByTestId("routing-locked");
  await expect(locked).toBeVisible();
  await expect(locked).toContainText(/locked this setting/i);

  // No way to change the route while locked.
  await expect(page.getByTestId("routing-select")).toHaveCount(0);
  await expect(page.getByTestId("routing-save")).toHaveCount(0);

  await setAdminLocked(deviceId, false);
});
