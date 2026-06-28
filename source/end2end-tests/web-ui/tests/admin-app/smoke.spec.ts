import { expect, test } from "@playwright/test";

/**
 * Smoke canary: the admin-app PWA shell loads in a mobile viewport without
 * throwing. Full PWA install / offline / nav coverage lives in
 * `pwa-shell.spec.ts`.
 */
test("admin-app shell renders", async ({ page }) => {
  const pageErrors: string[] = [];
  page.on("pageerror", (err) => pageErrors.push(String(err)));

  // baseURL ends in `/admin-app/`; `./` resolves to it.
  await page.goto("./");

  const root = page.locator("#root");
  await expect(root).toBeAttached();
  await expect(root).not.toBeEmpty();
  expect(pageErrors, `uncaught page errors:\n${pageErrors.join("\n")}`).toEqual(
    [],
  );
});
