import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { dhcpService } from "@/lib/sdk";
import type {
  DhcpConfigResponse,
  DhcpStatusResponse,
  ListDhcpLeasesResponse,
  ListDhcpReservationsResponse,
  UpdateDhcpConfigRequest,
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
      toast.success(data.config.enabled ? "DHCP server enabled" : "DHCP server disabled");
      qc.invalidateQueries({ queryKey: ["dhcp"] });
    },
    onError: () => toast.error("Failed to toggle DHCP server"),
  });
}

export function useUpdateDhcpConfig() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: UpdateDhcpConfigRequest) => dhcpService.updateConfig(body),
    onSuccess: () => {
      toast.success("DHCP configuration updated");
      qc.invalidateQueries({ queryKey: ["dhcp"] });
    },
    onError: () => toast.error("Failed to update configuration"),
  });
}

export function useCreateReservation() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateDhcpReservationRequest) => dhcpService.createReservation(body),
    onSuccess: (data) => {
      toast.success(data.message || "Reservation created");
      qc.invalidateQueries({ queryKey: ["dhcp", "reservations"] });
    },
    onError: () => toast.error("Failed to create reservation"),
  });
}

export function useDeleteReservation() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => dhcpService.deleteReservation(id),
    onSuccess: (data) => {
      toast.success(data.message || "Reservation deleted");
      qc.invalidateQueries({ queryKey: ["dhcp", "reservations"] });
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
    },
    onError: () => toast.error("Failed to revoke lease"),
  });
}
