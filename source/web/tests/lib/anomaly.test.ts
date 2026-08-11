import { describe, expect, it } from "vitest";
import type { Anomaly } from "@wardnet/js";
import {
  anomalySubjectLink,
  type AnomalyRouteMap,
} from "../../src/lib/anomaly";

/** Stands in for the desktop admin site, which has detail routes. */
const RICH: AnomalyRouteMap = {
  tunnel: (id) => `/tunnels/${id}`,
  device: (id) => `/devices/${id}`,
  blocklist: (profileId, blocklistId) =>
    `/dns/filter/profiles/${profileId}#blocklist-${blocklistId}`,
  fallback: (a) => (a.component === "dns" ? "/dns/filter" : "/tunnels"),
};

/** Stands in for the mobile PWA, which has only section routes. */
const COARSE: AnomalyRouteMap = {
  fallback: (a) => (a.component === "dns" ? "/dns" : "/tunnels"),
};

function anomaly(overrides: Partial<Anomaly> = {}): Anomaly {
  return {
    id: "a-1",
    type: "blocklist_refresh_failing",
    severity: "error",
    component: "dns",
    subject_id: "bl-1",
    message: "failed 7 times",
    hint: "check the URL",
    details: { profile_id: "p-1" },
    opened_at: "2026-08-01T00:00:00Z",
    last_seen_at: "2026-08-01T06:00:00Z",
    occurrences: 7,
    resolved_at: null,
    ...overrides,
  };
}

describe("anomalySubjectLink", () => {
  it("links a blocklist anomaly to its profile page, anchored on the row", () => {
    const link = anomalySubjectLink(anomaly(), RICH);
    expect(link?.href).toBe("/dns/filter/profiles/p-1#blocklist-bl-1");
  });

  it("links a tunnel anomaly to the tunnel detail page", () => {
    for (const type of ["tunnel_start_failed", "tunnel_unhealthy"] as const) {
      const link = anomalySubjectLink(
        anomaly({ type, component: "tunnel", subject_id: "tun-9" }),
        RICH,
      );
      expect(link?.href).toBe("/tunnels/tun-9");
    }
  });

  // A blocklist has no page of its own, so without the owning profile there
  // is nowhere precise to go — fall back rather than guess a URL.
  it("falls back when a blocklist anomaly has no profile_id", () => {
    const link = anomalySubjectLink(anomaly({ details: null }), RICH);
    expect(link?.href).toBe("/dns/filter");
  });

  it("falls back on a surface with no detail routes", () => {
    const link = anomalySubjectLink(anomaly(), COARSE);
    expect(link?.href).toBe("/dns");
  });

  // Not an oversight: route_table_lost has no subject to link to.
  it("returns null when nothing can resolve", () => {
    const link = anomalySubjectLink(
      anomaly({
        type: "route_table_lost",
        component: "routing",
        subject_id: "100",
        details: null,
      }),
      { tunnel: (id) => `/tunnels/${id}` },
    );
    expect(link).toBeNull();
  });

  it("falls back for an anomaly type it has no precise route for", () => {
    const link = anomalySubjectLink(
      anomaly({
        type: "dhcp_conflict",
        component: "dhcp",
        subject_id: "1.2.3.4",
      }),
      RICH,
    );
    expect(link?.href).toBe("/tunnels");
  });

  // A newer daemon can add a type this client has never heard of.
  it("falls back for an unknown type rather than throwing", () => {
    const link = anomalySubjectLink(anomaly({ type: "solar_flare" }), RICH);
    expect(link?.href).toBe("/dns/filter");
  });

  it("names the target so list rows do not all share one accessible name", () => {
    expect(anomalySubjectLink(anomaly(), RICH)?.label).toBe(
      "Open the blocklist",
    );
    expect(
      anomalySubjectLink(
        anomaly({
          type: "tunnel_unhealthy",
          component: "tunnel",
          subject_id: "t",
        }),
        RICH,
      )?.label,
    ).toBe("Open the tunnel");
  });
});
