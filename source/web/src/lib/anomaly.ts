import type { Anomaly } from "@wardnet/js";

/**
 * Where each app sends a viewer who clicks through from an anomaly.
 *
 * The routes differ per surface — the desktop admin site has detail pages the
 * mobile PWA does not — so each app supplies its own map rather than the
 * daemon guessing. A builder returning `null` means "this surface cannot go
 * anywhere more specific", and resolution falls back to the anomaly type's
 * coarse landing page.
 */
export interface AnomalyRouteMap {
  /** `/tunnels/:id` on surfaces that have a tunnel detail page. */
  tunnel?: (tunnelId: string) => string | null;
  /** `/devices/:id` on surfaces that have a device detail page. */
  device?: (deviceId: string) => string | null;
  /**
   * The page listing a profile's blocklists. Blocklists have no page of their
   * own, so the profile page is the finest-grained destination that exists —
   * hence both ids.
   */
  blocklist?: (profileId: string, blocklistId: string) => string | null;
  /**
   * The type's coarse landing page (`/tunnels`, `/dns/filter`, …), as carried
   * on the anomaly itself. Supply this to opt into the fallback.
   */
  fallback?: (anomaly: Anomaly) => string | null;
}

/** A resolved destination for an anomaly's subject. */
export interface AnomalyLink {
  href: string;
  /**
   * Accessible name for the control. Names the target rather than saying
   * "open", because every row in a list would otherwise share one name.
   */
  label: string;
}

/** The `details` key a blocklist anomaly carries its owning profile under. */
function profileIdOf(anomaly: Anomaly): string | null {
  const value = anomaly.details?.["profile_id"];
  return typeof value === "string" && value.length > 0 ? value : null;
}

/**
 * Resolve where an anomaly's subject lives, or `null` when there is nowhere
 * to go.
 *
 * `null` is a real answer, not an oversight: `route_table_lost` has no
 * subject at all, and `dhcp_conflict` identifies an address rather than a
 * device the UI can link to.
 */
export function anomalySubjectLink(
  anomaly: Anomaly,
  routes: AnomalyRouteMap,
): AnomalyLink | null {
  const subject = anomaly.subject_id;

  if (subject) {
    switch (anomaly.type) {
      case "tunnel_start_failed":
      case "tunnel_unhealthy": {
        const href = routes.tunnel?.(subject);
        if (href) return { href, label: "Open the tunnel" };
        break;
      }
      case "blocklist_refresh_failing": {
        const profileId = profileIdOf(anomaly);
        const href = profileId
          ? routes.blocklist?.(profileId, subject)
          : undefined;
        if (href) return { href, label: "Open the blocklist" };
        break;
      }
      default:
        break;
    }
  }

  const fallback = routes.fallback?.(anomaly);
  return fallback ? { href: fallback, label: areaLabel(anomaly) } : null;
}

/** Human name for the area an anomaly's component maps to. */
function areaLabel(anomaly: Anomaly): string {
  // Upstream health is configured on the DNS page, not the ad-blocking one
  // the rest of the `dns` component points at.
  if (anomaly.type === "dns_upstream_unreachable") return "Open DNS";

  switch (anomaly.component) {
    case "tunnel":
      return "Open Tunnels";
    case "dns":
      return "Open Ad Blocking";
    case "dhcp":
      return "Open DHCP";
    case "routing":
      return "Open Routing";
    default:
      return "Open Settings";
  }
}
