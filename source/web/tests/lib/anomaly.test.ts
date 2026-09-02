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

// biome-ignore lint/security/noSecrets: identifier-shaped string, not a credential — the entropy heuristic misfires on long CamelCase names
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

  // A surface can list tunnels without having a page for an individual one.
  // The builder returning null is how it says so, and the viewer should still
  // get as far as the section rather than nowhere at all.
  it("falls back when a surface declines a tunnel detail route", () => {
    const link = anomalySubjectLink(
      anomaly({
        type: "tunnel_unhealthy",
        component: "tunnel",
        subject_id: "tun-9",
      }),
      { tunnel: () => null, fallback: () => "/tunnels" },
    );
    expect(link).toEqual({ href: "/tunnels", label: "Open Tunnels" });
  });

  // The fallback label names the area, because that is the most specific
  // thing left to say once there is no detail page to point at.
  it("labels a fallback by the area its component maps to", () => {
    const cases = [
      ["tunnel", "Open Tunnels"],
      ["dns", "Open Ad Blocking"],
      ["dhcp", "Open DHCP"],
      ["routing", "Open Routing"],
      // A newer daemon can name a component this client has never heard of.
      ["storage", "Open Settings"],
    ] as const;

    for (const [component, label] of cases) {
      const link = anomalySubjectLink(
        anomaly({ type: "route_table_lost", component, subject_id: null }),
        { fallback: () => "/somewhere" },
      );
      expect(link?.label).toBe(label);
    }
  });

  // `dns` covers both ad blocking and the upstream servers, so the component
  // alone is not enough: an upstream anomaly labelled "Open Ad Blocking"
  // would send an admin to the wrong page to fix it.
  it("labels an unreachable upstream by DNS rather than ad blocking", () => {
    const link = anomalySubjectLink(
      anomaly({
        type: "dns_upstream_unreachable",
        component: "dns",
        subject_id: "8.8.8.8",
      }),
      { fallback: () => "/dns" },
    );
    expect(link).toEqual({ href: "/dns", label: "Open DNS" });
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
