import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import type { CreateTunnelRequest } from "@wardnet/js";
import { tunnelService } from "@/lib/sdk";

export function useTunnels() {
  return useQuery({
    queryKey: ["tunnels"],
    queryFn: () => tunnelService.list(),
    refetchInterval: 15_000,
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
