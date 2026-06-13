import { expect, test } from "@playwright/test";

/**
 * Scaffold smoke: the admin-app PWA shell loads in a mobile viewport
 * without throwing. PWA install / offline / nav coverage lands in B1.
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
