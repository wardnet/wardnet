import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "@wardnet/ui";
import type {
  ApprovalParams,
  CreateAccessRequestRequest,
  AccessRequestStatus,
} from "@wardnet/js";

import { accessRequestService } from "../lib/sdk";

/**
 * The calling device's own access requests (device, by IP).
 *
 * `refetchOnWindowFocus` deliberately overrides the user PWA's global `false`,
 * for the same reason `usePrivateDnsMe` does: the decision is made in the
 * admin's own browser, so this device's cache cannot learn about it itself.
 * An approval arrives with a push the member taps, but a **decline** sends
 * nothing — without this, "Requested — waiting for your administrator" would
 * sit there indefinitely on an already-answered request. Foregrounding the app
 * is exactly when the answer can have changed, and a decline is a once-per-ask
 * event, so polling would be waste.
 */
export function useMyAccessRequests() {
  return useQuery({
    queryKey: ["access-requests", "me"],
    queryFn: () => accessRequestService.listMine(),
    refetchOnWindowFocus: true,
  });
}

/** Submit a request to the admin (device, by IP). */
export function useCreateAccessRequest() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateAccessRequestRequest) =>
      accessRequestService.createMine(body),
    onSuccess: () => {
      toast.success("Request sent to your administrator");
      qc.invalidateQueries({ queryKey: ["access-requests", "me"] });
    },
    onError: () => toast.error("Failed to send request"),
  });
}

/** Admin: all access requests, optionally filtered by status. */
export function useAccessRequests(status?: AccessRequestStatus) {
  return useQuery({
    queryKey: ["access-requests", "all", status ?? "any"],
    queryFn: () => accessRequestService.list(status),
  });
}

/**
 * Admin: approve or reject an access request.
 *
 * Approving a `private_dns` request mints the device's grant, so the Private
 * DNS status query is invalidated too — otherwise the Remote Access card would
 * keep showing the device as ungranted until something else refetched it.
 */
export function useDecideAccessRequest() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      id,
      status,
      approval,
    }: {
      id: string;
      status: AccessRequestStatus;
      approval?: ApprovalParams;
    }) => accessRequestService.decide(id, status, approval),
    // Keyed on the row the server returned, not the status we asked for. When
    // `AccessRequestListener` resolves the same row to `approved` from an
    // out-of-band grant, a concurrent Decline updates nothing and the service
    // hands back the decision that actually landed — telling the admin
    // "declined" there would be a straight lie about the state of the box.
    onSuccess: (data) => {
      toast.success(
        data.status === "approved" ? "Request approved" : "Request declined",
      );
      qc.invalidateQueries({ queryKey: ["access-requests"] });
      qc.invalidateQueries({ queryKey: ["private-dns", "status"] });
    },
    onError: () => toast.error("Failed to update request"),
  });
}
