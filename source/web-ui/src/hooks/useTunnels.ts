import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import type { CreateTunnelRequest, TunnelMetricsRange } from "@wardnet/js";
import { tunnelService } from "@/lib/sdk";

export function useTunnels() {
  return useQuery({
    queryKey: ["tunnels"],
    queryFn: () => tunnelService.list(),
    refetchInterval: 15_000,
  });
}

export function useTunnel(id: string) {
  return useQuery({
    queryKey: ["tunnels", id],
    queryFn: () => tunnelService.getById(id),
    enabled: !!id,
    refetchInterval: 15_000,
  });
}

export function useTunnelMetrics(id: string, range: TunnelMetricsRange) {
  return useQuery({
    queryKey: ["tunnels", id, "metrics", range],
    queryFn: () => tunnelService.getMetrics(id, range),
    enabled: !!id,
    // Refresh on the same cadence the daemon writes intraday rows.
    refetchInterval: range === "12mo" ? 5 * 60_000 : 60_000,
  });
}

export function useTunnelDevices(id: string) {
  return useQuery({
    queryKey: ["tunnels", id, "devices"],
    queryFn: () => tunnelService.listDevices(id),
    enabled: !!id,
    refetchInterval: 30_000,
  });
}

export function useCreateTunnel() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateTunnelRequest) => tunnelService.create(body),
    onSuccess: (data) => {
      toast.success(data.message || "Tunnel created");
      qc.invalidateQueries({ queryKey: ["tunnels"] });
    },
    onError: () => toast.error("Failed to create tunnel"),
  });
}

export function useDeleteTunnel() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => tunnelService.delete(id),
    onSuccess: (data) => {
      toast.success(data.message || "Tunnel deleted");
      qc.invalidateQueries({ queryKey: ["tunnels"] });
    },
    onError: () => toast.error("Failed to delete tunnel"),
  });
}
