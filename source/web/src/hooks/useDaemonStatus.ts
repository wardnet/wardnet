import { useQuery } from "@tanstack/react-query";
import { infoService } from "../lib/sdk";

interface DaemonStatus {
  reachable: boolean;
  /** Public CalVer (`YYYY.MM.DD`) — this is what the UI shows users. */
  version: string | null;
  /**
   * Diagnostic git-derived version (`X.Y.Z[-dev.N+gHASH]`). Useful for
   * support flows / bug reports; not displayed by default.
   */
  buildVersion: string | null;
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
          version: info.release_version,
          buildVersion: info.version,
          uptimeSeconds: info.uptime_seconds,
        };
      } catch {
        return {
          reachable: false,
          version: null,
          buildVersion: null,
          uptimeSeconds: null,
        };
      }
    },
    // Poll fast while the daemon is unreachable so the connection-error gate
    // and offline banner clear within a few seconds of it coming back; relax
    // to 30 s once we have a healthy connection.
    refetchInterval: (query) =>
      query.state.data?.reachable === false ? 5_000 : 30_000,
  });
}
