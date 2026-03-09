import { useQuery } from "@tanstack/react-query";
import { infoService } from "@/lib/sdk";

interface DaemonStatus {
  reachable: boolean;
  version: string | null;
  uptimeSeconds: number | null;
}

/**
 * Checks daemon reachability using the unauthenticated /api/info endpoint.
 * Always returns version and uptime when connected, regardless of auth state.
 */
export function useDaemonStatus() {
  return useQuery<DaemonStatus>({
    queryKey: ["daemon", "info"],
    queryFn: async () => {
      try {
        const info = await infoService.getInfo();
        return {
          reachable: true,
          version: info.version,
          uptimeSeconds: info.uptime_seconds,
        };
      } catch {
        return { reachable: false, version: null, uptimeSeconds: null };
      }
    },
    refetchInterval: 30_000,
  });
}
