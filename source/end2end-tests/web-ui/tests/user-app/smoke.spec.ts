import { expect, test } from "@playwright/test";

/**
 * Smoke canary: the user-app PWA shell loads without throwing. Driven from
 * the LAN-side runner over the `tls_proxy_lan` origin (C1, #626), so the
 * daemon resolves a device — but this spec only asserts the shell mounts.
 * Manifest/offline and device-identity coverage live in `pwa-shell.spec.ts`
 * and `identity.spec.ts`.
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
