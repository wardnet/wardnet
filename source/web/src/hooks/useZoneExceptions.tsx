import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "@wardnet/ui";
import { zoneExceptionsService } from "../lib/sdk";
import type {
  ListZoneExceptionsResponse,
  CreateZoneExceptionRequest,
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
