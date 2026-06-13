import { mkdirSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";

import { test as setup } from "@playwright/test";

import {
  STORAGE_STATE,
  UI_HOST,
  ensureAdminSetup,
  waitForReady,
} from "./seed.js";

/**
 * One-shot harness bootstrap (Playwright `setup` project — runs before
 * every surface project via `dependencies`).
 *
 * Seeds the admin and completes the setup wizard over the SDK, then
 * writes a browser storageState carrying the `wardnet_session` cookie so
 * the admin-site and admin-app projects start authenticated. We build
 * the cookie from the login token (the daemon sets
 * `wardnet_session=<token>; Secure; …`) rather than scripting the login
 * form — deterministic, and independent of A1's auth-UI work.
 */
setup("seed admin and capture session", async () => {
  await waitForReady();
  const token = await ensureAdminSetup();

  const storageState = {
    cookies: [
      {
        name: "wardnet_session",
        value: token,
        domain: UI_HOST,
        path: "/",
        // Session cookie (no persisted expiry); -1 = expires at session end.
        expires: -1,
        httpOnly: true,
        // The daemon marks the cookie Secure; the surface projects run
        // Chromium with --unsafely-treat-insecure-origin-as-secure so it
        // is still sent over the plain-HTTP origin.
        secure: true,
        sameSite: "Strict" as const,
      },
    ],
    origins: [],
  };

  mkdirSync(dirname(STORAGE_STATE), { recursive: true });
  writeFileSync(STORAGE_STATE, JSON.stringify(storageState, null, 2));
});
