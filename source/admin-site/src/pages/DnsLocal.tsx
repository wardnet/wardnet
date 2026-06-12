import { PageHeader } from "@/components/compound/PageHeader";
import { LocalRecordsCard } from "@/components/features/LocalRecordsCard";
import { DnsZonesCard } from "@/components/features/DnsZonesCard";
import { ConditionalForwardingCard } from "@/components/features/ConditionalForwardingCard";
import { DhcpLanInfoCard } from "@/components/features/DhcpLanInfoCard";

/** Local DNS page (admin only) — authoritative zones, custom records, and
 *  conditional forwarding. Distinct from the resolver page (`/dns`), which
 *  owns cache, upstreams, and the query log. */
export default function DnsLocal() {
  return (
    <div className="col gap-20">
      <PageHeader
        title="Local DNS"
        description="Resolve your own names on the LAN: authoritative zones, custom records, and per-domain forwarding."
      />

      <div className="col gap-20">
        <LocalRecordsCard />
        <DnsZonesCard />
        <ConditionalForwardingCard />
        <DhcpLanInfoCard />
      </div>
    </div>
  );
}
