import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import type { RoutingTarget, UpdateDeviceRequest } from "@wardnet/js";
import { deviceService } from "@/lib/sdk";

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
  });
}

export function useSetMyRule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (target: RoutingTarget) => deviceService.setMyRule(target),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["devices", "me"] }),
  });
}

export function useUpdateDevice() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, body }: { id: string; body: UpdateDeviceRequest }) =>
      deviceService.update(id, body),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["devices"] });
    },
  });
}
