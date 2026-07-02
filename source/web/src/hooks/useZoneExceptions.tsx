import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "@wardnet/ui";
import { zoneExceptionsService } from "../lib/sdk";
import type {
  ListZoneExceptionsResponse,
  CreateZoneExceptionRequest,
  ExceptionEndpoint,
} from "@wardnet/js";

// ---------------------------------------------------------------------------
// Cross-zone exceptions (issue #737)
// ---------------------------------------------------------------------------

export function useZoneExceptions() {
  return useQuery<ListZoneExceptionsResponse>({
    queryKey: ["zone-exceptions"],
    queryFn: () => zoneExceptionsService.list(),
  });
}

export function useCreateZoneException() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateZoneExceptionRequest) =>
      zoneExceptionsService.create(body),
    onSuccess: () => {
      toast.success("Exception created");
      qc.invalidateQueries({ queryKey: ["zone-exceptions"] });
    },
    onError: () => toast.error("Failed to create exception"),
  });
}

export function useDeleteZoneException() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => zoneExceptionsService.delete(id),
    onSuccess: () => {
      toast.success("Exception removed");
      qc.invalidateQueries({ queryKey: ["zone-exceptions"] });
    },
    onError: () => toast.error("Failed to remove exception"),
  });
}

/**
 * One-click casting preset: open the built-in casting service set (mDNS,
 * SSDP/DLNA, Chromecast, AirPlay) bidirectionally between two endpoints
 * (each a device or a whole zone). Thin wrapper over
 * {@link useCreateZoneException} that fills in `service` + `bidirectional`.
 */
export function useCreateCastingException() {
  const create = useCreateZoneException();
  return {
    ...create,
    castBetween: (from: ExceptionEndpoint, to: ExceptionEndpoint) =>
      create.mutate({
        from,
        to,
        service: { type: "preset", set: "casting" },
        bidirectional: true,
      }),
  };
}
