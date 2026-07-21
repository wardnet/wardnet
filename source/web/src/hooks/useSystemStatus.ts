import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { RecentErrorsResponse } from "@wardnet/js";
import { systemService } from "../lib/sdk";

export function useSystemStatus() {
  return useQuery({
    queryKey: ["system", "status"],
    queryFn: () => systemService.getStatus(),
    refetchInterval: 30_000,
  });
}

/**
 * Mutation that dismisses the unclean-shutdown banner.
 *
 * On success, invalidates the `["system","status"]` query so the
 * banner predicate (`acknowledged_at >= last_shutdown.at`) re-evaluates
 * with the freshly persisted timestamp and the banner disappears.
 */
export function useAcknowledgeShutdown() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => systemService.acknowledgeShutdown(),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["system", "status"] });
    },
  });
}

export function useRecentErrors() {
  return useQuery<RecentErrorsResponse>({
    queryKey: ["system", "errors"],
    queryFn: () => systemService.getRecentErrors(),
    refetchInterval: 15_000,
  });
}

export type { SystemDiagnostic } from "@wardnet/js";
