import { AlertTriangle } from "lucide-react";
import { Banner } from "@wardnet/web";
import { useDaemonStatus } from "@wardnet/web";

/**
 * Full-width red banner shown at the top of all pages when the daemon is unreachable.
 * Uses the same unauthenticated /api/info endpoint as the sidebar connection indicator.
 *
 * Thin data-coupled wrapper over the Forge `<Banner>` primitive — visual
 * shape lives in the `.banner` recipe (styles.css §05); this component only
 * decides whether to show the strip and what to put inside it.
 */
export function ConnectionBanner() {
  const { data } = useDaemonStatus();

  if (!data || data.reachable) return null;

  return (
    <Banner tone="down" icon={<AlertTriangle />} role="alert">
      Unable to connect to the Wardnet daemon. Make sure it is running.
    </Banner>
  );
}
