import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type {
  ConfigureCloudflareRequest,
  DdnsRegisterRequest,
  TlsStatusResponse,
} from "@wardnet/js";
import { secureAccessService } from "../lib/sdk";

/** Check whether a bridge short name is available (manual, on demand). */
export function useCheckDdnsName() {
  return useMutation({
    mutationFn: (name: string) => secureAccessService.checkName(name),
  });
}

/** Register on the bridge; issuance begins in the background. */
export function useRegisterDdns() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (body: DdnsRegisterRequest) => secureAccessService.register(body),
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
      secureAccessService.configureCloudflare(body),
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
    queryFn: () => secureAccessService.ddnsStatus(),
    enabled,
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
    queryFn: () => secureAccessService.tlsStatus(),
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
