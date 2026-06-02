import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import type { CreateTunnelRequest } from "@wardnet/js";
import { tunnelService } from "../lib/sdk";

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

export function useTestTunnel() {
  return useMutation({
    mutationFn: (id: string) => tunnelService.test(id),
    onError: (error) => {
      const message =
        error instanceof Error ? error.message : "Tunnel test failed";
      // 409 surfaces as "conflict" / "test already in progress" — show a
      // dedicated toast so a rapid double-click is obviously rate-limited
      // rather than a generic failure.
      if (/already|conflict/i.test(message)) {
        toast.error("Test already in progress");
      } else {
        toast.error(message);
      }
    },
  });
}

export function useRebuildTunnel() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => tunnelService.rebuild(id),
    onSuccess: () => {
      toast.success("Tunnel rebuild initiated");
      qc.invalidateQueries({ queryKey: ["tunnels"] });
    },
    onError: () => toast.error("Failed to rebuild tunnel"),
  });
}

export function useSetTunnelDnsOverride() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, value }: { id: string; value: boolean }) =>
      tunnelService.setDnsOverride(id, { override_default_dns: value }),
    onSuccess: (_data, vars) => {
      toast.success(
        vars.value
          ? "Tunneled-device DNS will route through wardnet (filtered)"
          : "Tunneled-device DNS now uses the system upstream pool",
      );
      qc.invalidateQueries({ queryKey: ["tunnels"] });
      qc.invalidateQueries({ queryKey: ["tunnels", vars.id] });
    },
    onError: () => toast.error("Failed to update DNS override"),
  });
}
