import { useQuery } from "@tanstack/react-query";
import { networkService } from "@/lib/sdk";

/** Read the LAN interface's current state for the wizard + Settings. */
export function useNetworkStatus() {
  return useQuery({
    queryKey: ["network", "status"],
    queryFn: () => networkService.getStatus(),
  });
}
