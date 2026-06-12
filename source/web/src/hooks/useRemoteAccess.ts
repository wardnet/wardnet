import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type {
  ConfigureCloudflareRequest,
  DdnsRegisterRequest,
  DdnsResolutionCheckResponse,
  TlsStatusResponse,
} from "@wardnet/js";
import { remoteAccessService } from "../lib/sdk";

/** Check whether a bridge short name is available (manual, on demand). */
export function useCheckDdnsName() {
  return useMutation({
    mutationFn: (name: string) => remoteAccessService.checkName(name),
  });
}

/** Register on the bridge; issuance begins in the background. */
export function useRegisterDdns() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (body: DdnsRegisterRequest) => remoteAccessService.register(body),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["ddns", "status"] });
      queryClient.invalidateQueries({ queryKey: ["tls", "status"] });
    },
  });
}

/** Configure BYOD-Cloudflare; issuance begins in the background. */
export function useConfigureCloudflare() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (body: ConfigureCloudflareRequest) =>
      remoteAccessService.configureCloudflare(body),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["ddns", "status"] });
      queryClient.invalidateQueries({ queryKey: ["tls", "status"] });
    },
  });
}

/** Current DDNS configuration. */
export function useDdnsStatus(enabled = true) {
  return useQuery({
    queryKey: ["ddns", "status"],
    queryFn: () => remoteAccessService.ddnsStatus(),
    enabled,
  });
}

/**
 * External resolution check: does public DNS resolve the active FQDN to the
 * published IP? Runs on mount and can be re-run on demand (`refetch`). Cached
 * briefly so navigating back doesn't re-hit the external resolvers every time.
 */
export function useResolutionCheck(enabled = true) {
  return useQuery<DdnsResolutionCheckResponse>({
    queryKey: ["ddns", "resolution-check"],
    queryFn: () => remoteAccessService.resolutionCheck(),
    enabled,
    staleTime: 30_000,
  });
}

/**
 * Disable remote access (teardown). Invalidates the DDNS + TLS status and the
 * resolution check so every surface reverts to the unconfigured state.
 */
export function useDeleteDdns() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => remoteAccessService.teardown(),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["ddns", "status"] });
      queryClient.invalidateQueries({ queryKey: ["tls", "status"] });
      queryClient.invalidateQueries({ queryKey: ["ddns", "resolution-check"] });
    },
  });
}

/**
 * TLS provisioning status. While issuance is in flight (`issuing`), poll every
 * few seconds so the wizard and dashboard reflect progress; stop polling once a
 * terminal phase (`issued`/`failed`/`idle`) is reached.
 */
export function useTlsStatus(options?: { enabled?: boolean; poll?: boolean }) {
  const enabled = options?.enabled ?? true;
  const poll = options?.poll ?? true;
  return useQuery<TlsStatusResponse>({
    queryKey: ["tls", "status"],
    queryFn: () => remoteAccessService.tlsStatus(),
    enabled,
    // While issuing, poll every 3s; stop at any terminal phase. `refetchInterval`
    // overrides `staleTime` for the polling case, so this stays live during
    // issuance while letting the dashboard banner reuse the cache across mounts
    // in the steady state instead of refetching on every visit.
    staleTime: 30_000,
    refetchInterval: (query) =>
      poll && query.state.data?.phase === "issuing" ? 3000 : false,
  });
}
