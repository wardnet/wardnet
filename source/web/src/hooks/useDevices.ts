import { useRef } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import type { RoutingTarget, UpdateDeviceRequest, DnsCaptureSettingsRequest } from "@wardnet/js";
import { deviceService } from "../lib/sdk";

export function useDevices() {
  return useQuery({
    queryKey: ["devices"],
    queryFn: () => deviceService.list(),
    refetchInterval: 10_000,
  });
}

export function useDevice(id: string) {
  return useQuery({
    queryKey: ["devices", id],
    queryFn: () => deviceService.getById(id),
    enabled: !!id,
  });
}

export function useMyDevice() {
  return useQuery({
    queryKey: ["devices", "me"],
    queryFn: () => deviceService.getMe(),
    refetchInterval: 10_000,
  });
}

export function useSetMyRule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (target: RoutingTarget) => deviceService.setMyRule(target),
    onSuccess: () => {
      toast.success("Routing updated");
      qc.invalidateQueries({ queryKey: ["devices", "me"] });
    },
    onError: () => toast.error("Failed to update routing"),
  });
}

export function useUpdateDevice(options?: { successMessage?: string }) {
  const qc = useQueryClient();
  const optionsRef = useRef(options);
  optionsRef.current = options;
  return useMutation({
    mutationFn: ({ id, body }: { id: string; body: UpdateDeviceRequest }) =>
      deviceService.update(id, body),
    onSuccess: (_, variables) => {
      toast.success(optionsRef.current?.successMessage ?? "Device updated");
      qc.invalidateQueries({ queryKey: ["devices"], exact: true });
      qc.invalidateQueries({
        queryKey: ["devices", variables.id],
        exact: true,
      });
    },
    onError: () => toast.error("Failed to update device"),
  });
}

export function useDnsCaptureSettings(id: string) {
  return useQuery({
    queryKey: ["devices", id, "dns-capture"],
    queryFn: () => deviceService.getDnsCaptureSettings(id),
  });
}

export function useUpdateDnsCaptureSettings(id: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: DnsCaptureSettingsRequest) =>
      deviceService.updateDnsCaptureSettings(id, body),
    onSuccess: () => {
      toast.success("DNS capture settings saved");
      qc.invalidateQueries({ queryKey: ["devices", id, "dns-capture"] });
    },
    onError: () => toast.error("Failed to save DNS capture settings"),
  });
}
