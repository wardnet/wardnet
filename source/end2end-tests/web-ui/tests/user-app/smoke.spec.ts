import { expect, test } from "@playwright/test";

/**
 * Scaffold smoke: the user-app PWA shell loads without throwing. Driven
 * from the mgmt-side runner, whose source IP is not a discovered LAN
 * device, so the daemon's `/api/devices/me` returns `{device:null}` and
 * the app shows its no-device state — we only assert the shell mounts.
 * Real device-keyed flows (and the LAN-side runner) arrive in C1/C2
 * (#626/#627).
 */
test("user-app shell renders", async ({ page }) => {
  const pageErrors: string[] = [];
  page.on("pageerror", (err) => pageErrors.push(String(err)));

  await page.goto("./");

  const root = page.locator("#root");
  await expect(root).toBeAttached();
  await expect(root).not.toBeEmpty();
  expect(pageErrors, `uncaught page errors:\n${pageErrors.join("\n")}`).toEqual(
    [],
  );
});
