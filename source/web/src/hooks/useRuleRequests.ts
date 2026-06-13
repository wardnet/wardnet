import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import type { CreateRuleRequestRequest, RuleRequestStatus } from "@wardnet/js";

import { ruleRequestService } from "../lib/sdk";

/** The calling device's own rule requests (device, by IP). */
export function useMyRuleRequests() {
  return useQuery({
    queryKey: ["rule-requests", "me"],
    queryFn: () => ruleRequestService.listMine(),
  });
}

/** Submit a block/allow request to the admin (device, by IP). */
export function useCreateRuleRequest() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateRuleRequestRequest) =>
      ruleRequestService.createMine(body),
    onSuccess: () => {
      toast.success("Request sent to your administrator");
      qc.invalidateQueries({ queryKey: ["rule-requests", "me"] });
    },
    onError: () => toast.error("Failed to send request"),
  });
}

/** Admin: all rule requests, optionally filtered by status. */
export function useRuleRequests(status?: RuleRequestStatus) {
  return useQuery({
    queryKey: ["rule-requests", "all", status ?? "any"],
    queryFn: () => ruleRequestService.list(status),
  });
}

/** Admin: approve or reject a rule request. */
export function useDecideRuleRequest() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, status }: { id: string; status: RuleRequestStatus }) =>
      ruleRequestService.decide(id, status),
    onSuccess: (_data, { status }) => {
      toast.success(
        status === "approved" ? "Request approved" : "Request rejected",
      );
      qc.invalidateQueries({ queryKey: ["rule-requests"] });
    },
    onError: () => toast.error("Failed to update request"),
  });
}
