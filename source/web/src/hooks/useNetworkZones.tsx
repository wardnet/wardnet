import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "@wardnet/ui";
import { networkZonesService } from "../lib/sdk";
import type {
  ListNetworkZonesResponse,
  CreateNetworkZoneRequest,
  UpdateNetworkZoneRequest,
  QuarantineNewDevicesResponse,
} from "@wardnet/js";

// ---------------------------------------------------------------------------
// Zones (issue #735 / epic #244)
// ---------------------------------------------------------------------------

export function useNetworkZones() {
  return useQuery<ListNetworkZonesResponse>({
    queryKey: ["network-zones"],
    queryFn: () => networkZonesService.list(),
  });
}

export function useCreateNetworkZone() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateNetworkZoneRequest) =>
      networkZonesService.create(body),
    onSuccess: () => {
      toast.success("Zone created");
      qc.invalidateQueries({ queryKey: ["network-zones"] });
    },
    onError: () => toast.error("Failed to create zone"),
  });
}

/**
 * Partially update a zone. Also the path for promoting a zone: pass
 * `{ is_default: true }` or `{ is_default_for_new: true }`.
 */
export function useUpdateNetworkZone(options?: { successMessage?: string }) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      id,
      body,
    }: {
      id: string;
      body: UpdateNetworkZoneRequest;
    }) => networkZonesService.update(id, body),
    onSuccess: () => {
      toast.success(options?.successMessage ?? "Zone updated");
      qc.invalidateQueries({ queryKey: ["network-zones"] });
    },
    onError: () => toast.error("Failed to update zone"),
  });
}

export function useDeleteNetworkZone() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => networkZonesService.delete(id),
    onSuccess: () => {
      toast.success("Zone deleted");
      qc.invalidateQueries({ queryKey: ["network-zones"] });
    },
    onError: () => toast.error("Failed to delete zone"),
  });
}

/**
 * Reassign a device to a different zone. Invalidates both the device caches
 * (the device's `zone_id` changes) and the zone list (member counts shift).
 */
export function useAssignDeviceZone(options?: { successMessage?: string }) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ deviceId, zoneId }: { deviceId: string; zoneId: string }) =>
      networkZonesService.assignDevice(deviceId, zoneId),
    onSuccess: () => {
      toast.success(options?.successMessage ?? "Zone updated");
      qc.invalidateQueries({ queryKey: ["devices"] });
      qc.invalidateQueries({ queryKey: ["network-zones"] });
    },
    onError: () => toast.error("Failed to reassign zone"),
  });
}

// ---------------------------------------------------------------------------
// New-device quarantine toggle (issue #738)
// ---------------------------------------------------------------------------

export function useQuarantineNewDevices() {
  return useQuery<QuarantineNewDevicesResponse>({
    queryKey: ["network-zones", "quarantine"],
    queryFn: () => networkZonesService.getQuarantineNewDevices(),
  });
}

export function useSetQuarantineNewDevices() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (enabled: boolean) =>
      networkZonesService.setQuarantineNewDevices({ enabled }),
    onSuccess: (_data, enabled) => {
      toast.success(
        enabled
          ? "New-device notifications enabled"
          : "New-device notifications disabled",
      );
      qc.invalidateQueries({ queryKey: ["network-zones", "quarantine"] });
    },
    onError: () => toast.error("Failed to update setting"),
  });
}
