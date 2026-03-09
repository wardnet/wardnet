import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
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
    onSuccess: () => qc.invalidateQueries({ queryKey: ["tunnels"] }),
  });
}

export function useDeleteTunnel() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => tunnelService.delete(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["tunnels"] }),
  });
}
