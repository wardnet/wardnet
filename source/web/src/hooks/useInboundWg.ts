import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "@wardnet/ui";
import type {
  AddInboundWgPeerRequest,
  InboundWgConfigRequest,
} from "@wardnet/js";
import { inboundWgService } from "../lib/sdk";

/** Server config + public key, for the enable/settings card. */
export function useInboundWgConfig() {
  return useQuery({
    queryKey: ["inbound-wg", "config"],
    queryFn: () => inboundWgService.getConfig(),
  });
}

export function useSetInboundWgConfig() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: InboundWgConfigRequest) =>
      inboundWgService.setConfig(body),
    onSuccess: (data, vars) => {
      qc.setQueryData(["inbound-wg", "config"], data);
      toast.success(
        vars.enabled ? "Remote access enabled" : "Remote access disabled",
      );
      qc.invalidateQueries({ queryKey: ["inbound-wg", "peers"] });
    },
    onError: () => toast.error("Failed to update remote access settings"),
  });
}

export function useInboundWgPeers() {
  return useQuery({
    queryKey: ["inbound-wg", "peers"],
    queryFn: () => inboundWgService.listPeers(),
    refetchInterval: 15_000,
  });
}

export function useAddInboundWgPeer() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: AddInboundWgPeerRequest) =>
      inboundWgService.addPeer(body),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["inbound-wg", "peers"] });
    },
    onError: (error) => {
      const message =
        error instanceof Error
          ? error.message
          : "Failed to grant remote access";
      toast.error(message);
    },
  });
}

export function useRemoveInboundWgPeer() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => inboundWgService.removePeer(id),
    onSuccess: () => {
      toast.success("Remote access revoked");
      qc.invalidateQueries({ queryKey: ["inbound-wg", "peers"] });
    },
    onError: () => toast.error("Failed to revoke remote access"),
  });
}

export function useSetInboundWgPeerEnabled() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) =>
      inboundWgService.setPeerEnabled(id, { enabled }),
    onSuccess: (_data, vars) => {
      toast.success(
        vars.enabled ? "Remote access resumed" : "Remote access paused",
      );
      qc.invalidateQueries({ queryKey: ["inbound-wg", "peers"] });
    },
    onError: () => toast.error("Failed to update peer"),
  });
}
