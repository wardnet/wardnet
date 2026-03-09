import { useQuery } from "@tanstack/react-query";
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
