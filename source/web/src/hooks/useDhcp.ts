import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "@wardnet/ui";
import { dhcpService } from "../lib/sdk";
import type {
  DhcpConfigResponse,
  DhcpStatusResponse,
  ListDhcpLeasesResponse,
  ListDhcpReservationsResponse,
  UpdateDhcpConfigRequest,
  PreviewDhcpConfigRequest,
  PreviewDhcpConfigResponse,
  CreateDhcpReservationRequest,
} from "@wardnet/js";

export function useDhcpStatus() {
  return useQuery<DhcpStatusResponse>({
    queryKey: ["dhcp", "status"],
    queryFn: () => dhcpService.status(),
    refetchInterval: 15_000,
  });
}

export function useDhcpConfig() {
  return useQuery<DhcpConfigResponse>({
    queryKey: ["dhcp", "config"],
    queryFn: () => dhcpService.getConfig(),
  });
}

export function useDhcpLeases() {
  return useQuery<ListDhcpLeasesResponse>({
    queryKey: ["dhcp", "leases"],
    queryFn: () => dhcpService.listLeases(),
    refetchInterval: 15_000,
  });
}

export function useDhcpReservations() {
  return useQuery<ListDhcpReservationsResponse>({
    queryKey: ["dhcp", "reservations"],
    queryFn: () => dhcpService.listReservations(),
  });
}

export function useToggleDhcp() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (enabled: boolean) => dhcpService.toggle({ enabled }),
    onSuccess: (data) => {
      toast.success(
        data.config.enabled ? "DHCP server enabled" : "DHCP server disabled",
      );
      qc.invalidateQueries({ queryKey: ["dhcp"] });
    },
    onError: () => toast.error("Failed to toggle DHCP server"),
  });
}

export function usePreviewDhcpConfig() {
  return useMutation<
    PreviewDhcpConfigResponse,
    Error,
    PreviewDhcpConfigRequest
  >({
    mutationFn: (body: PreviewDhcpConfigRequest) =>
      dhcpService.previewConfig(body),
  });
}

export function useUpdateDhcpConfig() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: UpdateDhcpConfigRequest) =>
      dhcpService.updateConfig(body),
    onSuccess: () => {
      toast.success("DHCP configuration updated");
      qc.invalidateQueries({ queryKey: ["dhcp"] });
    },
    onError: () => toast.error("Failed to update configuration"),
  });
}

export function useCreateReservation(options?: { silent?: boolean }) {
  const qc = useQueryClient();
  const silent = options?.silent ?? false;
  return useMutation({
    mutationFn: (body: CreateDhcpReservationRequest) =>
      dhcpService.createReservation(body),
    onSuccess: (data) => {
      // `silent` suppresses the toasts for flows that recreate a reservation
      // as a rollback step (see DeviceNetworkCard), where a "created" toast
      // would contradict the failure the user is being shown.
      if (!silent) toast.success(data.message || "Reservation created");
      qc.invalidateQueries({ queryKey: ["dhcp", "reservations"] });
      // Device DHCP chip is derived from the device payload, not the
      // reservation list — refresh it so the UI reflects the change without
      // needing a manual reload.
      qc.invalidateQueries({ queryKey: ["devices"] });
    },
    onError: () => {
      if (!silent) toast.error("Failed to create reservation");
    },
  });
}

export function useDeleteReservation() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => dhcpService.deleteReservation(id),
    onSuccess: (data) => {
      toast.success(data.message || "Reservation deleted");
      qc.invalidateQueries({ queryKey: ["dhcp", "reservations"] });
      qc.invalidateQueries({ queryKey: ["devices"] });
    },
    onError: () => toast.error("Failed to delete reservation"),
  });
}

export function useRevokeLease() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => dhcpService.revokeLease(id),
    onSuccess: (data) => {
      toast.success(data.message || "Lease revoked");
      qc.invalidateQueries({ queryKey: ["dhcp", "leases"] });
      qc.invalidateQueries({ queryKey: ["devices"] });
    },
    onError: () => toast.error("Failed to revoke lease"),
  });
}
