import { useEffect, useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { dnsFilterService, jobsService } from "../lib/sdk";
import { JobProgressDescription } from "../components/JobProgressDescription";
import type {
  ListProfilesResponse,
  GetProfileResponse,
  CreateProfileRequest,
  UpdateProfileRequest,
  ListBlocklistsResponse,
  CreateBlocklistRequest,
  UpdateBlocklistRequest,
  ListAllowlistResponse,
  CreateAllowlistRequest,
  ListFilterRulesResponse,
  CreateFilterRuleRequest,
  UpdateFilterRuleRequest,
  ListDeviceFilterSettingsParams,
  ListDeviceFilterSettingsResponse,
  GetDeviceFilterSettingsResponse,
  UpdateDeviceFilterSettingsRequest,
  DnsFilterConfigResponse,
  UpdateDnsFilterConfigRequest,
  Job,
} from "@wardnet/js";
import { isJobTerminal } from "@wardnet/js";

// ---------------------------------------------------------------------------
// Profiles
// ---------------------------------------------------------------------------

export function useDnsFilterProfiles() {
  return useQuery<ListProfilesResponse>({
    queryKey: ["dns-filter", "profiles"],
    queryFn: () => dnsFilterService.listProfiles(),
  });
}

export function useDnsFilterProfile(profileId: string | undefined) {
  return useQuery<GetProfileResponse>({
    queryKey: ["dns-filter", "profile", profileId],
    queryFn: () => dnsFilterService.getProfile(profileId!),
    enabled: !!profileId,
  });
}

export function useCreateDnsFilterProfile() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateProfileRequest) =>
      dnsFilterService.createProfile(body),
    onSuccess: (data) => {
      toast.success(data.message || "Profile created");
      qc.invalidateQueries({ queryKey: ["dns-filter"] });
    },
    onError: () => toast.error("Failed to create profile"),
  });
}

export function useUpdateDnsFilterProfile() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, body }: { id: string; body: UpdateProfileRequest }) =>
      dnsFilterService.updateProfile(id, body),
    onSuccess: (data) => {
      toast.success(data.message || "Profile updated");
      qc.invalidateQueries({ queryKey: ["dns-filter"] });
    },
    onError: () => toast.error("Failed to update profile"),
  });
}

export function useDeleteDnsFilterProfile() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => dnsFilterService.deleteProfile(id),
    onSuccess: (data) => {
      toast.success(data.message || "Profile deleted");
      qc.invalidateQueries({ queryKey: ["dns-filter"] });
    },
    onError: (err: unknown) => {
      const status = (err as { status?: number } | null)?.status;
      if (status === 409) {
        toast.error("Builtin profiles cannot be deleted");
      } else {
        toast.error("Failed to delete profile");
      }
    },
  });
}

// ---------------------------------------------------------------------------
// Profile-scoped blocklists
// ---------------------------------------------------------------------------

export function useBlocklists(profileId: string | undefined) {
  return useQuery<ListBlocklistsResponse>({
    queryKey: ["dns-filter", "profile", profileId, "blocklists"],
    queryFn: () => dnsFilterService.listBlocklists(profileId!),
    enabled: !!profileId,
  });
}

export function useCreateBlocklist(profileId: string | undefined) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateBlocklistRequest) =>
      dnsFilterService.createBlocklist(profileId!, body),
    onSuccess: (data) => {
      toast.success(data.message || "Blocklist added");
      qc.invalidateQueries({
        queryKey: ["dns-filter", "profile", profileId, "blocklists"],
      });
    },
    onError: () => toast.error("Failed to add blocklist"),
  });
}

export function useUpdateBlocklist(profileId: string | undefined) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, body }: { id: string; body: UpdateBlocklistRequest }) =>
      dnsFilterService.updateBlocklist(profileId!, id, body),
    onSuccess: (data) => {
      toast.success(data.message || "Blocklist updated");
      qc.invalidateQueries({
        queryKey: ["dns-filter", "profile", profileId, "blocklists"],
      });
    },
    onError: () => toast.error("Failed to update blocklist"),
  });
}

export function useDeleteBlocklist(profileId: string | undefined) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) =>
      dnsFilterService.deleteBlocklist(profileId!, id),
    onSuccess: (data) => {
      toast.success(data.message || "Blocklist deleted");
      qc.invalidateQueries({
        queryKey: ["dns-filter", "profile", profileId, "blocklists"],
      });
    },
    onError: () => toast.error("Failed to delete blocklist"),
  });
}

/** Trigger a blocklist refresh and surface progress in a sonner toast.
 *
 *  The server dispatches a background job and returns immediately with its
 *  id; this hook polls the job and updates the toast through its lifecycle
 *  (loading → success/error). The `["dns-filter", "profile", id, "blocklists"]`
 *  query is invalidated on completion so the row's `last_updated` /
 *  `entry_count` refresh. Only one refresh is tracked at a time. */
export function useRefreshBlocklist(profileId: string | undefined) {
  const qc = useQueryClient();
  const [active, setActive] = useState<{
    jobId: string;
    blocklistId: string;
  } | null>(null);

  const dispatch = useMutation({
    mutationFn: async (blocklistId: string) => {
      const res = await dnsFilterService.refreshBlocklist(
        profileId!,
        blocklistId,
      );
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
        description: (
          <JobProgressDescription percentage={job.percentage_done} />
        ),
      });
    } else if (job.status === "SUCCEED") {
      toast.success("Blocklist refreshed", {
        id: active.jobId,
        description: undefined,
      });
      qc.invalidateQueries({
        queryKey: ["dns-filter", "profile", profileId, "blocklists"],
      });
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setActive(null);
    } else if (job.status === "TERMINATED_WITH_ERRORS") {
      toast.error(job.error || "Blocklist refresh failed", {
        id: active.jobId,
        description: undefined,
      });
      qc.invalidateQueries({
        queryKey: ["dns-filter", "profile", profileId, "blocklists"],
      });
      setActive(null);
    }
  }, [jobQuery.data, active, qc, profileId]);

  return {
    mutate: dispatch.mutate,
    isPending: dispatch.isPending || !!active,
    variables: active?.blocklistId ?? dispatch.variables,
  };
}

// ---------------------------------------------------------------------------
// Profile-scoped allowlist
// ---------------------------------------------------------------------------

export function useAllowlist(profileId: string | undefined) {
  return useQuery<ListAllowlistResponse>({
    queryKey: ["dns-filter", "profile", profileId, "allowlist"],
    queryFn: () => dnsFilterService.listAllowlist(profileId!),
    enabled: !!profileId,
  });
}

export function useCreateAllowlistEntry(profileId: string | undefined) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateAllowlistRequest) =>
      dnsFilterService.createAllowlistEntry(profileId!, body),
    onSuccess: (data) => {
      toast.success(data.message || "Domain allowlisted");
      qc.invalidateQueries({
        queryKey: ["dns-filter", "profile", profileId, "allowlist"],
      });
    },
    onError: () => toast.error("Failed to add allowlist entry"),
  });
}

export function useDeleteAllowlistEntry(profileId: string | undefined) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) =>
      dnsFilterService.deleteAllowlistEntry(profileId!, id),
    onSuccess: (data) => {
      toast.success(data.message || "Allowlist entry removed");
      qc.invalidateQueries({
        queryKey: ["dns-filter", "profile", profileId, "allowlist"],
      });
    },
    onError: () => toast.error("Failed to remove allowlist entry"),
  });
}

// ---------------------------------------------------------------------------
// Profile-scoped custom rules
// ---------------------------------------------------------------------------

export function useFilterRules(profileId: string | undefined) {
  return useQuery<ListFilterRulesResponse>({
    queryKey: ["dns-filter", "profile", profileId, "rules"],
    queryFn: () => dnsFilterService.listFilterRules(profileId!),
    enabled: !!profileId,
  });
}

export function useCreateFilterRule(profileId: string | undefined) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateFilterRuleRequest) =>
      dnsFilterService.createFilterRule(profileId!, body),
    onSuccess: (data) => {
      toast.success(data.message || "Filter rule added");
      qc.invalidateQueries({
        queryKey: ["dns-filter", "profile", profileId, "rules"],
      });
    },
    onError: () => toast.error("Failed to add filter rule"),
  });
}

export function useUpdateFilterRule(profileId: string | undefined) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, body }: { id: string; body: UpdateFilterRuleRequest }) =>
      dnsFilterService.updateFilterRule(profileId!, id, body),
    onSuccess: (data) => {
      toast.success(data.message || "Filter rule updated");
      qc.invalidateQueries({
        queryKey: ["dns-filter", "profile", profileId, "rules"],
      });
    },
    onError: () => toast.error("Failed to update filter rule"),
  });
}

export function useDeleteFilterRule(profileId: string | undefined) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) =>
      dnsFilterService.deleteFilterRule(profileId!, id),
    onSuccess: (data) => {
      toast.success(data.message || "Filter rule deleted");
      qc.invalidateQueries({
        queryKey: ["dns-filter", "profile", profileId, "rules"],
      });
    },
    onError: () => toast.error("Failed to delete filter rule"),
  });
}

// ---------------------------------------------------------------------------
// Per-device settings
// ---------------------------------------------------------------------------

export function useDeviceFilterSettingsList(
  params: ListDeviceFilterSettingsParams = {},
) {
  // Stable cache key — `enabled === undefined` collapses to "all".
  const key =
    params.enabled === undefined ? "all" : params.enabled ? "true" : "false";
  return useQuery<ListDeviceFilterSettingsResponse>({
    queryKey: ["dns-filter", "devices", key],
    queryFn: () => dnsFilterService.listDeviceSettings(params),
  });
}

export function useDeviceFilterSettings(deviceId: string | undefined) {
  return useQuery<GetDeviceFilterSettingsResponse>({
    queryKey: ["dns-filter", "device", deviceId],
    queryFn: () => dnsFilterService.getDeviceSettings(deviceId!),
    enabled: !!deviceId,
  });
}

export function useUpdateDeviceFilterSettings() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      id,
      body,
    }: {
      id: string;
      body: UpdateDeviceFilterSettingsRequest;
    }) => dnsFilterService.updateDeviceSettings(id, body),
    onSuccess: (data, variables) => {
      toast.success(data.message || "DNS filter settings updated");
      qc.invalidateQueries({
        queryKey: ["dns-filter", "device", variables.id],
      });
      qc.invalidateQueries({ queryKey: ["dns-filter", "devices"] });
    },
    onError: () => toast.error("Failed to update DNS filter settings"),
  });
}

// ---------------------------------------------------------------------------
// Global config
// ---------------------------------------------------------------------------

export function useDnsFilterConfig() {
  return useQuery<DnsFilterConfigResponse>({
    queryKey: ["dns-filter", "config"],
    queryFn: () => dnsFilterService.getConfig(),
  });
}

export function useUpdateDnsFilterConfig() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: UpdateDnsFilterConfigRequest) =>
      dnsFilterService.updateConfig(body),
    onSuccess: () => {
      // Stable `id` collapses rapid successive calls (e.g. clicking
      // a default-profile toggle on and off quickly) into a single
      // toast slot. Without it sonner sometimes renders the second
      // toast with an empty body for a frame.
      toast.success("DNS filter configuration updated", {
        id: "dns-filter-config-update",
      });
      qc.invalidateQueries({ queryKey: ["dns-filter", "config"] });
    },
    onError: () =>
      toast.error("Failed to update DNS filter configuration", {
        id: "dns-filter-config-update",
      }),
  });
}
