import { describe, expect, it } from "vitest";
import type { Anomaly } from "@wardnet/js";

import { ADMIN_APP_ANOMALY_ROUTES } from "@/lib/anomalyRoutes";

function anomaly(overrides: Partial<Anomaly> = {}): Anomaly {
  return {
    id: "a-1",
    type: "blocklist_refresh_failing",
    severity: "error",
    component: "dns",
    subject_id: "bl-1",
    message: "failed 7 times",
    hint: "check the URL",
    details: null,
    opened_at: "2026-08-01T00:00:00Z",
    last_seen_at: "2026-08-01T06:00:00Z",
    occurrences: 7,
    resolved_at: null,
    ...overrides,
  };
}

describe("ADMIN_APP_ANOMALY_ROUTES", () => {
  // This surface has section routes but no detail pages, so a per-entity
  // builder would only ever produce a URL that 404s.
  it("offers no per-entity routes", () => {
    expect(ADMIN_APP_ANOMALY_ROUTES.tunnel).toBeUndefined();
    expect(ADMIN_APP_ANOMALY_ROUTES.device).toBeUndefined();
    expect(ADMIN_APP_ANOMALY_ROUTES.blocklist).toBeUndefined();
  });

  it.each([
    ["tunnel", "/tunnels"],
    ["dns", "/dns"],
    ["dhcp", "/devices"],
    ["routing", "/devices"],
  ])("sends a %s anomaly to %s", (component, expected) => {
    expect(ADMIN_APP_ANOMALY_ROUTES.fallback?.(anomaly({ component }))).toBe(
      expected,
    );
  });

  // A newer daemon can raise an anomaly from a component this build predates.
  it("sends an unrecognised component to the system page", () => {
    expect(
      ADMIN_APP_ANOMALY_ROUTES.fallback?.(anomaly({ component: "solar" })),
    ).toBe("/system");
  });
});
