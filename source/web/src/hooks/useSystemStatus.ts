import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { systemService, client } from "../lib/sdk";

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

/**
 * A structured, admin-facing problem report. Raised by a daemon component and
 * paired with a plain-language message and a remediation hint — see the
 * daemon's `diagnostics` module. Replaces the old raw warning/error log line.
 */
interface Diagnostic {
  timestamp: string;
  /** Stable class identifier, e.g. `tunnel_start_failed`. */
  code: string;
  /** One of `error`, `warning`, `info`. */
  severity: string;
  /** The subsystem that raised it. */
  component: string;
  message: string;
  /** What the admin can do about it. */
  hint: string;
}

interface RecentErrorsResponse {
  errors: Diagnostic[];
}

export function useRecentErrors() {
  return useQuery<RecentErrorsResponse>({
    queryKey: ["system", "errors"],
    queryFn: () => client.request<RecentErrorsResponse>("/system/errors"),
    refetchInterval: 15_000,
  });
}

export type { Diagnostic };
