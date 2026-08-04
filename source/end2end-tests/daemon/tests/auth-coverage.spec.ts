import { describe, it, expect, beforeAll } from "vitest";
import { WardnetClient } from "@wardnet/js";

import { API_BASE_URL, waitForReady } from "./helpers.js";

/**
 * Admin-coverage matrix: a single table asserting that a representative
 * slice of the admin API rejects unauthenticated reads with 401.
 *
 * The per-feature specs each authenticate first and then exercise their
 * endpoint, so none of them proves the guard is actually attached. A
 * dropped `AdminAuth` extractor on a new route — or a router refactor
 * that mounts a handler outside the guarded layer — would sail through
 * every one of those specs and only surface in production. This matrix
 * is the cheap tripwire: one entry per surface, each hit with no
 * credentials, each expected to be turned away before the handler runs.
 *
 * Kept deliberately small and representative (one endpoint per major
 * subsystem) rather than exhaustive — the goal is to catch a
 * whole-surface auth regression, not to enumerate every route. Add a row
 * when a new subsystem's admin surface lands.
 */
const ADMIN_ENDPOINTS: ReadonlyArray<{ name: string; path: string }> = [
  { name: "DHCP config", path: "/dhcp/config" },
  { name: "DNS config", path: "/dns/config" },
  { name: "devices list", path: "/devices" },
  { name: "tunnels list", path: "/tunnels" },
  { name: "backup status", path: "/backup/status" },
  { name: "system status", path: "/system/status" },
  { name: "update status", path: "/update/status" },
];

describe("auth — admin endpoints reject unauthenticated reads", () => {
  beforeAll(async () => {
    await waitForReady(new WardnetClient({ baseUrl: API_BASE_URL }));
  }, 120_000);

  it.each(ADMIN_ENDPOINTS)(
    "GET $path ($name) → 401 without credentials",
    async ({ path }) => {
      const res = await fetch(`${API_BASE_URL}${path}`);
      expect(res.status).toBe(401);
    },
  );
});
