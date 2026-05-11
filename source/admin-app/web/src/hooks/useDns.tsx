import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { dnsService } from "@/lib/sdk";
import type {
  DnsConfigResponse,
  DnsStatusResponse,
  UpdateDnsConfigRequest,
  DnsCacheFlushResponse,
} from "@wardnet/js";

export function useDnsStatus() {
  return useQuery<DnsStatusResponse>({
    queryKey: ["dns", "status"],
    queryFn: () => dnsService.status(),
    refetchInterval: 15_000,
  });
}

export function useDnsConfig() {
  return useQuery<DnsConfigResponse>({
    queryKey: ["dns", "config"],
    queryFn: () => dnsService.getConfig(),
  });
}

export function useToggleDns() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (enabled: boolean) => dnsService.toggle({ enabled }),
    onSuccess: (data) => {
      toast.success(data.config.enabled ? "DNS server enabled" : "DNS server disabled");
      qc.invalidateQueries({ queryKey: ["dns"] });
    },
    onError: () => toast.error("Failed to toggle DNS server"),
  });
}

export function useUpdateDnsConfig() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: UpdateDnsConfigRequest) => dnsService.updateConfig(body),
    onSuccess: () => {
      toast.success("DNS configuration updated");
      qc.invalidateQueries({ queryKey: ["dns"] });
    },
    onError: () => toast.error("Failed to update DNS configuration"),
  });
}

export function useFlushDnsCache() {
  const qc = useQueryClient();
  return useMutation<DnsCacheFlushResponse>({
    mutationFn: () => dnsService.flushCache(),
    onSuccess: (data) => {
      toast.success(`Cache flushed (${data.entries_cleared} entries cleared)`);
      qc.invalidateQueries({ queryKey: ["dns", "status"] });
    },
    onError: () => toast.error("Failed to flush DNS cache"),
  });
}
