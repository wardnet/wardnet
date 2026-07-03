import { PageHeader } from "@/components/compound/PageHeader";
import { ZonesCard } from "@/components/features/ZonesCard";
import { QuarantineSettingsCard } from "@/components/features/QuarantineSettingsCard";
import { ZoneExceptionsCard } from "@/components/features/ZoneExceptionsCard";

/** Network Zones page (admin only) — device policy buckets, cross-zone
 *  exceptions/casting, and new-device quarantine. Epic #244. */
export default function Zones() {
  return (
    <div className="col gap-20">
      <PageHeader
        title="Zones"
        description="Group devices into policy buckets that gate routing and isolate them from the rest of your network."
      />

      <div className="col gap-20">
        <ZonesCard />
        <ZoneExceptionsCard />
        <QuarantineSettingsCard />
      </div>
    </div>
  );
}
