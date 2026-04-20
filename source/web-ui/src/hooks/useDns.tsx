import { useEffect, useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { dnsService, jobsService } from "@/lib/sdk";
import { JobProgressDescription } from "@/components/compound/JobProgressDescription";
import type {
  DnsConfigResponse,
  DnsStatusResponse,
  UpdateDnsConfigRequest,
  DnsCacheFlushResponse,
  ListBlocklistsResponse,
  CreateBlocklistRequest,
  UpdateBlocklistRequest,
  ListAllowlistResponse,
  CreateAllowlistRequest,
  ListFilterRulesResponse,
  CreateFilterRuleRequest,
  UpdateFilterRuleRequest,
  Job,
} from "@wardnet/js";
import { isJobTerminal } from "@wardnet/js";

export function useDnsStatus() {
  return useQuery<DnsStatusResponse>({
    queryKey: ["dns", "status"],
    queryFn: () => dnsService.status(),
    refetchInterval: 15_000,
  });
}

export function useDnsConfig() {
  return useQuery<DnsConfigResponse>({
    queryKey: ["dns", "config"],
    queryFn: () => dnsService.getConfig(),
  });
}

export function useToggleDns() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (enabled: boolean) => dnsService.toggle({ enabled }),
    onSuccess: (data) => {
      toast.success(data.config.enabled ? "DNS server enabled" : "DNS server disabled");
      qc.invalidateQueries({ queryKey: ["dns"] });
    },
    onError: () => toast.error("Failed to toggle DNS server"),
  });
}

export function useUpdateDnsConfig() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: UpdateDnsConfigRequest) => dnsService.updateConfig(body),
    onSuccess: () => {
      toast.success("DNS configuration updated");
      qc.invalidateQueries({ queryKey: ["dns"] });
    },
    onError: () => toast.error("Failed to update DNS configuration"),
  });
}

export function useFlushDnsCache() {
  const qc = useQueryClient();
  return useMutation<DnsCacheFlushResponse>({
    mutationFn: () => dnsService.flushCache(),
    onSuccess: (data) => {
      toast.success(`Cache flushed (${data.entries_cleared} entries cleared)`);
      qc.invalidateQueries({ queryKey: ["dns", "status"] });
    },
    onError: () => toast.error("Failed to flush DNS cache"),
  });
}

// ---------------------------------------------------------------------------
// Blocklists
// ---------------------------------------------------------------------------

export function useBlocklists() {
  return useQuery<ListBlocklistsResponse>({
    queryKey: ["dns", "blocklists"],
    queryFn: () => dnsService.listBlocklists(),
  });
}

export function useCreateBlocklist() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateBlocklistRequest) => dnsService.createBlocklist(body),
    onSuccess: (data) => {
      toast.success(data.message || "Blocklist added");
      qc.invalidateQueries({ queryKey: ["dns", "blocklists"] });
    },
    onError: () => toast.error("Failed to add blocklist"),
  });
}

export function useUpdateBlocklist() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, body }: { id: string; body: UpdateBlocklistRequest }) =>
      dnsService.updateBlocklist(id, body),
    onSuccess: (data) => {
      toast.success(data.message || "Blocklist updated");
      qc.invalidateQueries({ queryKey: ["dns", "blocklists"] });
    },
    onError: () => toast.error("Failed to update blocklist"),
  });
}

export function useDeleteBlocklist() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => dnsService.deleteBlocklist(id),
    onSuccess: (data) => {
      toast.success(data.message || "Blocklist deleted");
      qc.invalidateQueries({ queryKey: ["dns", "blocklists"] });
    },
    onError: () => toast.error("Failed to delete blocklist"),
  });
}

/** Trigger a blocklist refresh and surface progress in a sonner toast.
 *
 *  The server dispatches a background job and returns immediately with its
 *  id; this hook polls the job and updates the toast through its lifecycle
 *  (loading → success/error). Blocklists query is invalidated on completion
 *  so the row's `last_updated` + `entry_count` refresh.
 *
 *  Only one refresh is tracked at a time — the `variables` / `isPending`
 *  shape mirrors `useMutation` so callers that used the old hook keep
 *  working without changes. */
export function useUpdateBlocklistNow() {
  const qc = useQueryClient();
  const [active, setActive] = useState<{ jobId: string; blocklistId: string } | null>(null);

  const dispatch = useMutation({
    mutationFn: async (blocklistId: string) => {
      const res = await dnsService.updateBlocklistNow(blocklistId);
      return { blocklistId, jobId: res.job_id };
    },
    onSuccess: ({ blocklistId, jobId }) => {
      setActive({ jobId, blocklistId });
      toast.loading("Refreshing blocklist…", {
        id: jobId,
        description: <JobProgressDescription percentage={0} />,
      });
    },
    onError: () => toast.error("Failed to trigger blocklist refresh"),
  });

  const jobQuery = useQuery<Job>({
    queryKey: ["job", active?.jobId],
    queryFn: () => jobsService.get(active!.jobId),
    enabled: !!active,
    refetchInterval: (q) => {
      const s = q.state.data?.status;
      return s && isJobTerminal(s) ? false : 1000;
    },
  });

  useEffect(() => {
    const job = jobQuery.data;
    if (!job || !active) return;

    if (job.status === "RUNNING" || job.status === "PENDING") {
      toast.loading("Refreshing blocklist…", {
        id: active.jobId,
        description: <JobProgressDescription percentage={job.percentage_done} />,
      });
    } else if (job.status === "SUCCEED") {
      toast.success("Blocklist refreshed", {
        id: active.jobId,
        description: undefined,
      });
      qc.invalidateQueries({ queryKey: ["dns", "blocklists"] });
      // One-shot terminal transition: clear so the query disables and the
      // next dispatch starts a fresh cycle. Safe — next render short-circuits
      // because `active` is null.
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setActive(null);
    } else if (job.status === "TERMINATED_WITH_ERRORS") {
      toast.error(job.error || "Blocklist refresh failed", {
        id: active.jobId,
        description: undefined,
      });
      qc.invalidateQueries({ queryKey: ["dns", "blocklists"] });
      setActive(null);
    }
  }, [jobQuery.data, active, qc]);

  return {
    mutate: dispatch.mutate,
    isPending: dispatch.isPending || !!active,
    variables: active?.blocklistId ?? dispatch.variables,
  };
}

// ---------------------------------------------------------------------------
// Allowlist
// ---------------------------------------------------------------------------

export function useAllowlist() {
  return useQuery<ListAllowlistResponse>({
    queryKey: ["dns", "allowlist"],
    queryFn: () => dnsService.listAllowlist(),
  });
}

export function useCreateAllowlistEntry() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateAllowlistRequest) => dnsService.createAllowlistEntry(body),
    onSuccess: (data) => {
      toast.success(data.message || "Domain allowlisted");
      qc.invalidateQueries({ queryKey: ["dns", "allowlist"] });
    },
    onError: () => toast.error("Failed to add allowlist entry"),
  });
}

export function useDeleteAllowlistEntry() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => dnsService.deleteAllowlistEntry(id),
    onSuccess: (data) => {
      toast.success(data.message || "Allowlist entry removed");
      qc.invalidateQueries({ queryKey: ["dns", "allowlist"] });
    },
    onError: () => toast.error("Failed to remove allowlist entry"),
  });
}

// ---------------------------------------------------------------------------
// Custom filter rules
// ---------------------------------------------------------------------------

export function useFilterRules() {
  return useQuery<ListFilterRulesResponse>({
    queryKey: ["dns", "filter-rules"],
    queryFn: () => dnsService.listFilterRules(),
  });
}

export function useCreateFilterRule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateFilterRuleRequest) => dnsService.createFilterRule(body),
    onSuccess: (data) => {
      toast.success(data.message || "Filter rule added");
      qc.invalidateQueries({ queryKey: ["dns", "filter-rules"] });
    },
    onError: () => toast.error("Failed to add filter rule"),
  });
}

export function useUpdateFilterRule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, body }: { id: string; body: UpdateFilterRuleRequest }) =>
      dnsService.updateFilterRule(id, body),
    onSuccess: (data) => {
      toast.success(data.message || "Filter rule updated");
      qc.invalidateQueries({ queryKey: ["dns", "filter-rules"] });
    },
    onError: () => toast.error("Failed to update filter rule"),
  });
}

export function useDeleteFilterRule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => dnsService.deleteFilterRule(id),
    onSuccess: (data) => {
      toast.success(data.message || "Filter rule deleted");
      qc.invalidateQueries({ queryKey: ["dns", "filter-rules"] });
    },
    onError: () => toast.error("Failed to delete filter rule"),
  });
}
