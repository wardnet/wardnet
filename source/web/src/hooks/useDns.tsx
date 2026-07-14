import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "@wardnet/ui";
import { dnsService } from "../lib/sdk";
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
      // Toggling this changes which DNS server DHCP advertises, but DHCP is
      // pull-based: a device keeps the resolver it was handed until its lease
      // renews (up to half the lease time — hours). Nothing can push the new
      // value to it, so say so plainly rather than let the admin believe the
      // switch took effect network-wide the moment they flipped it.
      toast.success(
        data.config.enabled ? "DNS server enabled" : "DNS server disabled",
        {
          description: LEASE_RENEWAL_NOTE,
          duration: 10_000,
        },
      );
      qc.invalidateQueries({ queryKey: ["dns"] });
      // The advertised DHCP option changed with it. Only the config view
      // derives from it — leases/reservations/status are unaffected, so
      // don't refetch the whole ["dhcp"] prefix.
      qc.invalidateQueries({ queryKey: ["dhcp", "config"] });
    },
    onError: () => toast.error("Failed to toggle DNS server"),
  });
}

/** Shared copy: DHCP cannot push a new DNS server to an already-leased device.
 * Worded for both directions — after *disabling*, the server from a device's
 * current lease is the now-dead Pi, so reconnecting is how an admin restores
 * name resolution, not just how they speed up rollout. */
export const LEASE_RENEWAL_NOTE =
  "Devices keep using the DNS server from their current lease until it renews — until then a change may leave them resolving via the old server, or not at all. Reconnect a device (toggle its Wi-Fi) to apply the change immediately.";

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
