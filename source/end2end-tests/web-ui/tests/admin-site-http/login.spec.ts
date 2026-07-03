import { expect, test } from "@playwright/test";

import { ADMIN_PASSWORD, ADMIN_USERNAME } from "../../fixtures/seed.js";
import { loginViaUi } from "../../fixtures/ui.js";

/**
 * Login coverage on the daemon's plain-HTTP `:7411` admin surface, reached
 * DIRECTLY (no TLS proxy). This is the pre-provisioning admin endpoint most
 * instances use — HTTPS on :443 only comes up once a cert is issued via the
 * DDNS / BYO-domain flow, so many deployments only ever have plain HTTP.
 *
 * The rest of the admin-site suite runs over the HTTPS proxy, where the daemon's
 * session cookie is stored regardless of its `Secure` attribute — which is
 * exactly why a `Secure`-over-http regression (the cookie the browser silently
 * drops on an http:// origin) slips past every other spec. These tests fail if
 * the `wardnet_session` cookie is ever marked `Secure` unconditionally: the
 * browser drops it, the next admin request 401s, and the guard bounces to /login.
 *
 * The `admin-site-http` project injects no `storageState`; log in fresh with the
 * `setup`-project credentials.
 */
test.use({ storageState: { cookies: [], origins: [] } });

test("stores the session cookie over http and lands on the dashboard", async ({
  page,
  context,
}) => {
  await page.goto("./login");
  await loginViaUi(page, ADMIN_USERNAME, ADMIN_PASSWORD);

  // Authenticated landing — the dashboard, not bounced back to /login.
  await expect(page).toHaveURL(/\/admin\/?$/);
  await expect(page.getByRole("main")).toBeVisible();

  // The browser actually stored the session cookie over an http:// origin,
  // which only happens when it is NOT marked `Secure`.
  const session = (await context.cookies()).find(
    (c) => c.name === "wardnet_session",
  );
  expect(
    session,
    "wardnet_session cookie must be stored over plain HTTP",
  ).toBeTruthy();
  expect(
    session?.secure,
    "session cookie must not be Secure on the plain-HTTP surface",
  ).toBe(false);
});

test("replays the session cookie across a protected-route navigation", async ({
  page,
}) => {
  await page.goto("./login");
  await loginViaUi(page, ADMIN_USERNAME, ADMIN_PASSWORD);
  await expect(page).toHaveURL(/\/admin\/?$/);

  // A protected route must resolve authenticated (cookie replayed), not bounce
  // to /login — the end-to-end proof that the session survives over http://.
  await page.goto("./dns");
  await expect(page).toHaveURL(/\/admin\/dns$/);
});
