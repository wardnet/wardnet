import type { AnomalyRouteMap } from "@wardnet/web";

/**
 * Where the desktop admin site sends a viewer clicking through from an
 * anomaly.
 *
 * This surface has detail pages for tunnels and devices, so most anomalies
 * resolve to the exact entity. A blocklist has no page of its own — the
 * profile page that lists it is the finest-grained destination that exists —
 * so the link carries a `#blocklist-<id>` hash the profile page uses to scroll
 * to and highlight the right row.
 */
export const ADMIN_SITE_ANOMALY_ROUTES: AnomalyRouteMap = {
  tunnel: (tunnelId) => `/tunnels/${tunnelId}`,
  device: (deviceId) => `/devices/${deviceId}`,
  blocklist: (profileId, blocklistId) =>
    `/dns/filter/profiles/${profileId}#blocklist-${blocklistId}`,
  fallback: (anomaly) => {
    switch (anomaly.component) {
      case "tunnel":
        return "/tunnels";
      case "dns":
        return "/dns/filter";
      case "dhcp":
        return "/dhcp";
      case "routing":
        return "/routing";
      default:
        return "/settings";
    }
  },
};
