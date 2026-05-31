import { useQuery } from "@tanstack/react-query";
import { dnsService } from "../lib/sdk";
import type { ListQueryLogParams, ListQueryLogResponse } from "@wardnet/js";

/** Paginated DNS query log. */
export function useDnsQueryLog(filter: ListQueryLogParams) {
  return useQuery<ListQueryLogResponse>({
    queryKey: ["dns", "query-log", filter],
    queryFn: () => dnsService.listQueryLog(filter),
  });
}
