import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import type {
  ValidateCredentialsRequest,
  ListServersRequest,
  SetupProviderRequest,
} from "@wardnet/js";
import { providerService } from "@/lib/sdk";

export function useProviders() {
  return useQuery({
    queryKey: ["providers"],
    queryFn: () => providerService.list(),
  });
}

export function useProviderCountries(providerId: string) {
  return useQuery({
    queryKey: ["providers", providerId, "countries"],
    queryFn: () => providerService.listCountries(providerId),
    enabled: !!providerId,
  });
}

export function useValidateCredentials() {
  return useMutation({
    mutationFn: ({ providerId, body }: { providerId: string; body: ValidateCredentialsRequest }) =>
      providerService.validateCredentials(providerId, body),
  });
}

export function useProviderServers() {
  return useMutation({
    mutationFn: ({ providerId, body }: { providerId: string; body: ListServersRequest }) =>
      providerService.listServers(providerId, body),
  });
}

export function useProviderSetup() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ providerId, body }: { providerId: string; body: SetupProviderRequest }) =>
      providerService.setupTunnel(providerId, body),
    onSuccess: (data) => {
      toast.success(data.message || "Tunnel created via provider");
      qc.invalidateQueries({ queryKey: ["tunnels"] });
    },
    onError: () => toast.error("Failed to set up tunnel"),
  });
}
