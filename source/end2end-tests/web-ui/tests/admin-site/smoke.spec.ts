import { expect, test } from "@playwright/test";

/**
 * Scaffold smoke: the admin-site shell loads and mounts without throwing.
 * Asserts on uncaught page errors (not console output) — service-worker
 * and secure-context warnings are expected over the HTTP+flag origin and
 * are not failures. Deep page behaviour lands in the A-stage issues.
 */
test("admin-site shell renders", async ({ page }) => {
  const pageErrors: string[] = [];
  page.on("pageerror", (err) => pageErrors.push(String(err)));

  // baseURL ends in `/admin/`; `./` resolves to it (not the origin root).
  await page.goto("./");

  const root = page.locator("#root");
  await expect(root).toBeAttached();
  await expect(root).not.toBeEmpty();
  expect(pageErrors, `uncaught page errors:\n${pageErrors.join("\n")}`).toEqual(
    [],
  );
});
