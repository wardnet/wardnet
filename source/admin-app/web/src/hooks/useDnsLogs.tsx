import { useQuery } from "@tanstack/react-query";
import { dnsService } from "@/lib/sdk";
import type { ListQueryLogParams, ListQueryLogResponse, DnsStatsResponse } from "@wardnet/js";

/** Paginated DNS query log. */
export function useDnsQueryLog(filter: ListQueryLogParams) {
  return useQuery<ListQueryLogResponse>({
    queryKey: ["dns", "query-log", filter],
    queryFn: () => dnsService.listQueryLog(filter),
  });
}

/** Aggregated DNS stats over the last `hours` hours. Refetched every 30s. */
export function useDnsStats(hours: number) {
  return useQuery<DnsStatsResponse>({
    queryKey: ["dns", "stats", hours],
    queryFn: () => dnsService.getStats(hours),
    refetchInterval: 30_000,
  });
}
