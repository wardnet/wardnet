import { useQuery } from "@tanstack/react-query";
import { systemService, client } from "@/lib/sdk";

export function useSystemStatus() {
  return useQuery({
    queryKey: ["system", "status"],
    queryFn: () => systemService.getStatus(),
    refetchInterval: 30_000,
  });
}

interface RecentError {
  timestamp: string;
  level: string;
  target: string;
  message: string;
}

interface RecentErrorsResponse {
  errors: RecentError[];
}

export function useRecentErrors() {
  return useQuery<RecentErrorsResponse>({
    queryKey: ["system", "errors"],
    queryFn: () => client.request<RecentErrorsResponse>("/system/errors"),
    refetchInterval: 15_000,
  });
}

export type { RecentError };
