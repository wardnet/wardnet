import { useQuery } from "@tanstack/react-query";
import { systemService } from "@/lib/sdk";

export function useSystemStatus() {
  return useQuery({
    queryKey: ["system", "status"],
    queryFn: () => systemService.getStatus(),
    refetchInterval: 30_000,
  });
}
