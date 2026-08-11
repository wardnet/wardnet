import type { AnomalyRouteMap } from "@wardnet/web";

/**
 * Where the mobile admin app sends a viewer clicking through from an anomaly.
 *
 * This surface has section routes but no detail pages, so almost everything
 * resolves through the fallback. That is deliberate rather than a gap: a
 * per-entity route the app does not have would 404, and the resolver's
 * fallback lands the viewer on the right screen instead.
 */
export const ADMIN_APP_ANOMALY_ROUTES: AnomalyRouteMap = {
  fallback: (anomaly) => {
    switch (anomaly.component) {
      case "tunnel":
        return "/tunnels";
      case "dns":
        return "/dns";
      case "dhcp":
      case "routing":
        return "/devices";
      default:
        return "/system";
    }
  },
};
