import { test as setup } from "@playwright/test";

import { ensureAdminSetup, ensurePremiumEnrollment } from "./seed.js";

/**
 * Premium-entitlement bootstrap for the two premium-gated surfaces
 * (admin-app, user-app — see `Entitlement::is_entitled()` gate from
 * 366f942). Split out from the generic `setup`/`seed-lan-device` projects
 * so `admin-site`/`admin-site-http`, which never hit that gate, don't
 * depend on `wardnet_cloud_mock` or pay for the enroll/register walk.
 *
 * Runs once per runner container (`ui_runner` for `admin-app`,
 * `ui_runner_lan` for `user-app`) — both against the one shared daemon.
 * `ensurePremiumEnrollment` serializes the two containers' enroll/register
 * calls under a cross-container lock; see its doc comment in `seed.ts`.
 */
setup("enroll into the (mocked) wardnet DDNS provider", async () => {
  const token = await ensureAdminSetup();
  await ensurePremiumEnrollment(token);
});
