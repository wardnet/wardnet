import { useMemo } from "react";
import { PageHeader } from "@/components/compound/PageHeader";
import { LocalRecordsCard } from "@/components/features/LocalRecordsCard";
import { DnsZonesCard } from "@/components/features/DnsZonesCard";
import { ConditionalForwardingCard } from "@/components/features/ConditionalForwardingCard";
import { DhcpLanInfoCard } from "@/components/features/DhcpLanInfoCard";
import {
  useDnsRecords,
  useDnsZones,
  useForwardingRules,
  useCreateDnsRecord,
  useUpdateDnsRecord,
  useDeleteDnsRecord,
  useCreateDnsZone,
  useUpdateDnsZone,
  useDeleteDnsZone,
  useCreateForwardingRule,
  useUpdateForwardingRule,
  useDeleteForwardingRule,
} from "@wardnet/web";

/** Local DNS page (admin only) — authoritative zones, custom records, and
 *  conditional forwarding. Distinct from the resolver page (`/dns`), which
 *  owns cache, upstreams, and the query log. Owns every query and mutation
 *  for the cards below, fetching the shared records/zones queries once and
 *  passing data + callbacks down so the cards stay pure presentation. */
export default function DnsLocal() {
  const { data: recordData } = useDnsRecords();
  const { data: zoneData } = useDnsZones();
  const { data: ruleData } = useForwardingRules();

  const createRecord = useCreateDnsRecord();
  const updateRecord = useUpdateDnsRecord();
  const deleteRecord = useDeleteDnsRecord();

  const createZone = useCreateDnsZone();
  const updateZone = useUpdateDnsZone();
  const deleteZone = useDeleteDnsZone();

  const createRule = useCreateForwardingRule();
  const updateRule = useUpdateForwardingRule();
  const deleteRule = useDeleteForwardingRule();

  const records = useMemo(() => recordData?.records ?? [], [recordData]);
  const zones = useMemo(() => zoneData?.zones ?? [], [zoneData]);
  const rules = useMemo(() => ruleData?.rules ?? [], [ruleData]);
  const dhcpRecordCount = useMemo(
    () => records.filter((r) => r.source === "dhcp").length,
    [records],
  );

  return (
    <div className="col gap-20">
      <PageHeader
        title="Local DNS"
        description="Resolve your own names on the LAN: authoritative zones, custom records, and per-domain forwarding."
      />

      <div className="col gap-20">
        <LocalRecordsCard
          records={records}
          zones={zones}
          isSaving={createRecord.isPending || updateRecord.isPending}
          updatePending={updateRecord.isPending}
          onCreateRecord={createRecord.mutate}
          onUpdateRecord={updateRecord.mutate}
          onDeleteRecord={deleteRecord.mutate}
        />
        <DnsZonesCard
          zones={zones}
          records={records}
          isSaving={createZone.isPending || updateZone.isPending}
          updatePending={updateZone.isPending}
          onCreateZone={createZone.mutate}
          onUpdateZone={updateZone.mutate}
          onDeleteZone={deleteZone.mutate}
        />
        <ConditionalForwardingCard
          rules={rules}
          isSaving={createRule.isPending || updateRule.isPending}
          updatePending={updateRule.isPending}
          onCreateRule={createRule.mutate}
          onUpdateRule={updateRule.mutate}
          onDeleteRule={deleteRule.mutate}
        />
        <DhcpLanInfoCard dhcpRecordCount={dhcpRecordCount} />
      </div>
    </div>
  );
}
