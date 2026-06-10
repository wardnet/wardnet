import { useMutation, useQuery } from "@tanstack/react-query";
import type { DiscoverGatewayMacRequest } from "@wardnet/js";
import { networkService } from "../lib/sdk";

/** Read the LAN interface's current state for the wizard + Settings. */
export function useNetworkStatus() {
  return useQuery({
    queryKey: ["network", "status"],
    queryFn: () => networkService.getStatus(),
  });
}

/**
 * Discover (or accept) the upstream router MAC.
 *
 * With an empty body the daemon probes the LAN gateway via ARP.
 * Pass `{mac: "..."}` to skip the probe and record an operator-typed
 * value instead.
 */
export function useDiscoverGatewayMac() {
  return useMutation({
    mutationFn: (body: DiscoverGatewayMacRequest = {}) =>
      networkService.discoverGatewayMac(body),
  });
}

/**
 * Send a DHCPDISCOVER and find out who responded.
 *
 * Used by the wizard's primary-mode step 3 after the operator says
 * they've disabled DHCP on their upstream router.
 */
export function useDhcpSelfProbe() {
  return useMutation({
    mutationFn: () => networkService.dhcpSelfProbe(),
  });
}
