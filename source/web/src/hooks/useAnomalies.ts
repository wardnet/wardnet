import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { ListAnomaliesParams, ListAnomaliesResponse } from "@wardnet/js";
import { anomalyService } from "../lib/sdk";

/**
 * Open anomalies, polled for the dashboard.
 *
 * One query serves every anomaly surface on a page — the card, a tunnel's
 * error footer — so a page fetches the list once and filters it by
 * `subject_id` client-side rather than issuing a request per entity.
 */
export function useAnomalies(params: ListAnomaliesParams = {}) {
  return useQuery<ListAnomaliesResponse>({
    queryKey: ["anomalies", params],
    queryFn: () => anomalyService.list(params),
    refetchInterval: 15_000,
  });
}

/**
 * Re-check every open anomaly now rather than waiting for the daemon's next
 * scheduled pass. Invalidates the listing so anything that closed disappears.
 */
export function useReevaluateAnomalies() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => anomalyService.reevaluate(),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["anomalies"] });
    },
  });
}
