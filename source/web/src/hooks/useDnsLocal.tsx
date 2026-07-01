import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "@wardnet/ui";
import { dnsLocalService } from "../lib/sdk";
import type {
  ListZonesResponse,
  GetZoneResponse,
  CreateZoneRequest,
  UpdateZoneRequest,
  ListRecordsResponse,
  GetRecordResponse,
  CreateRecordRequest,
  UpdateRecordRequest,
  ListForwardingRulesResponse,
  GetForwardingRuleResponse,
  CreateForwardingRuleRequest,
  UpdateForwardingRuleRequest,
} from "@wardnet/js";

// ---------------------------------------------------------------------------
// Zones
// ---------------------------------------------------------------------------

export function useDnsZones() {
  return useQuery<ListZonesResponse>({
    queryKey: ["dns-local", "zones"],
    queryFn: () => dnsLocalService.listZones(),
  });
}

export function useDnsZone(id: string | undefined) {
  return useQuery<GetZoneResponse>({
    queryKey: ["dns-local", "zone", id],
    queryFn: () => dnsLocalService.getZone(id!),
    enabled: !!id,
  });
}

/** Records assigned to a single zone. */
export function useDnsZoneRecords(zoneId: string | undefined) {
  return useQuery<ListRecordsResponse>({
    queryKey: ["dns-local", "zone", zoneId, "records"],
    queryFn: () => dnsLocalService.listZoneRecords(zoneId!),
    enabled: !!zoneId,
  });
}

export function useCreateDnsZone() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateZoneRequest) => dnsLocalService.createZone(body),
    onSuccess: (data) => {
      toast.success(data.message || "Zone created");
      qc.invalidateQueries({ queryKey: ["dns-local"] });
    },
    onError: () => toast.error("Failed to create zone"),
  });
}

export function useUpdateDnsZone() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, body }: { id: string; body: UpdateZoneRequest }) =>
      dnsLocalService.updateZone(id, body),
    onSuccess: (data) => {
      toast.success(data.message || "Zone updated");
      qc.invalidateQueries({ queryKey: ["dns-local"] });
    },
    onError: () => toast.error("Failed to update zone"),
  });
}

export function useDeleteDnsZone() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => dnsLocalService.deleteZone(id),
    onSuccess: (data) => {
      toast.success(data.message || "Zone deleted");
      qc.invalidateQueries({ queryKey: ["dns-local"] });
    },
    onError: () => toast.error("Failed to delete zone"),
  });
}

// ---------------------------------------------------------------------------
// Records
// ---------------------------------------------------------------------------

export function useDnsRecords() {
  return useQuery<ListRecordsResponse>({
    queryKey: ["dns-local", "records"],
    queryFn: () => dnsLocalService.listRecords(),
  });
}

export function useDnsRecord(id: string | undefined) {
  return useQuery<GetRecordResponse>({
    queryKey: ["dns-local", "record", id],
    queryFn: () => dnsLocalService.getRecord(id!),
    enabled: !!id,
  });
}

export function useCreateDnsRecord() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateRecordRequest) =>
      dnsLocalService.createRecord(body),
    onSuccess: (data) => {
      toast.success(data.message || "Record created");
      qc.invalidateQueries({ queryKey: ["dns-local"] });
    },
    onError: () => toast.error("Failed to create record"),
  });
}

export function useUpdateDnsRecord() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, body }: { id: string; body: UpdateRecordRequest }) =>
      dnsLocalService.updateRecord(id, body),
    onSuccess: (data) => {
      toast.success(data.message || "Record updated");
      qc.invalidateQueries({ queryKey: ["dns-local"] });
    },
    onError: () => toast.error("Failed to update record"),
  });
}

export function useDeleteDnsRecord() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => dnsLocalService.deleteRecord(id),
    onSuccess: (data) => {
      toast.success(data.message || "Record deleted");
      qc.invalidateQueries({ queryKey: ["dns-local"] });
    },
    onError: () => toast.error("Failed to delete record"),
  });
}

// ---------------------------------------------------------------------------
// Conditional forwarding (forwarding rules)
// ---------------------------------------------------------------------------

export function useForwardingRules() {
  return useQuery<ListForwardingRulesResponse>({
    queryKey: ["dns-local", "forwarding"],
    queryFn: () => dnsLocalService.listForwardingRules(),
  });
}

export function useForwardingRule(id: string | undefined) {
  return useQuery<GetForwardingRuleResponse>({
    queryKey: ["dns-local", "forwarding", id],
    queryFn: () => dnsLocalService.getForwardingRule(id!),
    enabled: !!id,
  });
}

export function useCreateForwardingRule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateForwardingRuleRequest) =>
      dnsLocalService.createForwardingRule(body),
    onSuccess: (data) => {
      toast.success(data.message || "Forwarding rule created");
      qc.invalidateQueries({ queryKey: ["dns-local"] });
    },
    onError: () => toast.error("Failed to create forwarding rule"),
  });
}

export function useUpdateForwardingRule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      id,
      body,
    }: {
      id: string;
      body: UpdateForwardingRuleRequest;
    }) => dnsLocalService.updateForwardingRule(id, body),
    onSuccess: (data) => {
      toast.success(data.message || "Forwarding rule updated");
      qc.invalidateQueries({ queryKey: ["dns-local"] });
    },
    onError: () => toast.error("Failed to update forwarding rule"),
  });
}

export function useDeleteForwardingRule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => dnsLocalService.deleteForwardingRule(id),
    onSuccess: (data) => {
      toast.success(data.message || "Forwarding rule deleted");
      qc.invalidateQueries({ queryKey: ["dns-local"] });
    },
    onError: () => toast.error("Failed to delete forwarding rule"),
  });
}
