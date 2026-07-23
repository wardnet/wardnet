import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "@wardnet/ui";
import { privateDnsService } from "../lib/sdk";

/** HTTP status carried by a `WardnetApiError`, if the rejection has one. */
function httpStatus(err: unknown): number | undefined {
  return (err as { status?: number } | null)?.status;
}

const STATUS_KEY = ["private-dns", "status"] as const;
const ME_KEY = ["private-dns", "me"] as const;

/**
 * Admin status: enabled state, enable prerequisites, and every grant. The card
 * on Remote Access is always mounted, so this drives the prerequisite checklist
 * and the grant table.
 */
export function usePrivateDnsStatus() {
  return useQuery({
    queryKey: STATUS_KEY,
    queryFn: () => privateDnsService.getStatus(),
  });
}

/**
 * Enable / disable Private DNS. Enabling is gated on the hard prerequisites and
 * the daemon rejects with 409 when one is missing — the toggle is already
 * disabled in that state, so a 409 is a safety net rather than the norm.
 */
export function useSetPrivateDnsEnabled() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (enabled: boolean) => privateDnsService.setEnabled(enabled),
    onSuccess: (data, enabled) => {
      // The mutation returns the fresh full status — seed the read cache so the
      // checklist and grant table update without a round-trip.
      qc.setQueryData(STATUS_KEY, data);
      toast.success(enabled ? "Private DNS enabled" : "Private DNS disabled");
    },
    onError: (error) => {
      if (httpStatus(error) === 409) {
        toast.error(
          "Private DNS can't be enabled yet — a prerequisite isn't met.",
        );
        return;
      }
      toast.error("Failed to update Private DNS");
    },
  });
}

/** Grant a managed device encrypted-DNS access; returns the minted grant. */
export function useGrantPrivateDnsDevice() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (deviceId: string) => privateDnsService.grantDevice(deviceId),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: STATUS_KEY });
    },
    onError: (error) => {
      const message =
        error instanceof Error ? error.message : "Failed to grant access";
      toast.error(message);
    },
  });
}

/** Revoke a device's grant. Keyed by device id, not grant id. */
export function useRevokePrivateDnsDevice() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (deviceId: string) => privateDnsService.revokeDevice(deviceId),
    onSuccess: () => {
      toast.success("Private DNS access revoked");
      qc.invalidateQueries({ queryKey: STATUS_KEY });
    },
    onError: () => toast.error("Failed to revoke access"),
  });
}

/**
 * Send a device-keyed push nudging the household member to set Private DNS up
 * on their phone. Resend-safe. A `delivered: false` result is still a success —
 * the device just hasn't enabled notifications yet, so we tell the admin to
 * relay the hostname directly.
 */
export function useSendPrivateDnsToDevice() {
  return useMutation({
    mutationFn: (deviceId: string) => privateDnsService.notifyDevice(deviceId),
    onSuccess: (result) => {
      if (result.delivered) {
        toast.success("Sent to device");
      } else {
        toast.warning(
          "This device hasn't enabled notifications yet — show them the hostname directly.",
        );
      }
    },
    onError: () => toast.error("Failed to send to device"),
  });
}

/**
 * This device's own Private DNS state, identified by source IP. Backs the user
 * PWA setup page.
 */
export function usePrivateDnsMe() {
  return useQuery({
    queryKey: ME_KEY,
    queryFn: () => privateDnsService.me(),
  });
}
