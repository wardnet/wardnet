import { Info } from "lucide-react";
import { Banner } from "@wardnet/web";

/**
 * The honest isolation disclaimer, shown wherever zone isolation is implied
 * (issue #739 acceptance criterion). Zones are isolated from each other by the
 * daemon's egress + admin-UI gates, but same-subnet peer↔peer traffic on a flat
 * L2 segment is the AP's job (or the isolate-members rung) — see the #736 ADR.
 */
export const ISOLATION_DISCLAIMER =
  "Zones are isolated from each other. Devices within the same zone are " +
  "isolated from one another only if your access point provides client " +
  "isolation, or member isolation is turned on for the zone.";

export function IsolationDisclaimer() {
  return (
    <Banner tone="info" icon={<Info />}>
      {ISOLATION_DISCLAIMER}
    </Banner>
  );
}
