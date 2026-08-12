import { describe, expect, it } from "vitest";
import type { Anomaly } from "@wardnet/js";

import { ADMIN_SITE_ANOMALY_ROUTES } from "@/lib/anomalyRoutes";

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

describe("ADMIN_SITE_ANOMALY_ROUTES", () => {
  it("routes tunnels and devices to their detail pages", () => {
    expect(ADMIN_SITE_ANOMALY_ROUTES.tunnel?.("tun-1")).toBe("/tunnels/tun-1");
    expect(ADMIN_SITE_ANOMALY_ROUTES.device?.("dev-1")).toBe("/devices/dev-1");
  });

  // A blocklist has no page of its own, so the link lands on the profile that
  // lists it and the hash picks out the row.
  it("anchors a blocklist on its owning profile's page", () => {
    expect(ADMIN_SITE_ANOMALY_ROUTES.blocklist?.("p-1", "bl-1")).toBe(
      "/dns/filter/profiles/p-1#blocklist-bl-1",
    );
  });

  it.each([
    ["tunnel", "/tunnels"],
    ["dns", "/dns/filter"],
    ["dhcp", "/dhcp"],
    ["routing", "/routing"],
  ])("falls back for a %s anomaly to %s", (component, expected) => {
    expect(ADMIN_SITE_ANOMALY_ROUTES.fallback?.(anomaly({ component }))).toBe(
      expected,
    );
  });

  it("sends an unrecognised component to settings", () => {
    expect(
      ADMIN_SITE_ANOMALY_ROUTES.fallback?.(anomaly({ component: "solar" })),
    ).toBe("/settings");
  });
});
