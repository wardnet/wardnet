import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "@wardnet/ui";
import { routingProfilesService } from "../lib/sdk";
import type {
  ListRoutingProfilesResponse,
  GetRoutingProfileResponse,
  CreateRoutingProfileRequest,
  UpdateRoutingProfileRequest,
  ListDomainRoutingRulesResponse,
  CreateDomainRoutingRuleRequest,
  UpdateDomainRoutingRuleRequest,
  GetDeviceRoutingProfilesResponse,
  ListProfileDevicesResponse,
} from "@wardnet/js";

// Query-key namespace. Every routing-profile query hangs off `["routing-profiles"]`
// so a single broad invalidation after any mutation refreshes the lists, the
// per-device assignment, and the reverse "used by" view together.
const ROOT = "routing-profiles";

// ---------------------------------------------------------------------------
// Profiles
// ---------------------------------------------------------------------------

export function useRoutingProfiles() {
  return useQuery<ListRoutingProfilesResponse>({
    queryKey: [ROOT, "profiles"],
    queryFn: () => routingProfilesService.listProfiles(),
  });
}

export function useRoutingProfile(profileId: string | undefined) {
  return useQuery<GetRoutingProfileResponse>({
    queryKey: [ROOT, "profile", profileId],
    queryFn: () => routingProfilesService.getProfile(profileId!),
    enabled: !!profileId,
  });
}

export function useCreateRoutingProfile() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateRoutingProfileRequest) =>
      routingProfilesService.createProfile(body),
    onSuccess: (data) => {
      toast.success(data.message || "Profile created");
      qc.invalidateQueries({ queryKey: [ROOT] });
    },
    onError: () => toast.error("Failed to create profile"),
  });
}

export function useUpdateRoutingProfile() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      id,
      body,
    }: {
      id: string;
      body: UpdateRoutingProfileRequest;
    }) => routingProfilesService.updateProfile(id, body),
    onSuccess: (data) => {
      toast.success(data.message || "Profile updated");
      qc.invalidateQueries({ queryKey: [ROOT] });
    },
    onError: () => toast.error("Failed to update profile"),
  });
}

export function useDeleteRoutingProfile() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => routingProfilesService.deleteProfile(id),
    onSuccess: (data) => {
      toast.success(data.message || "Profile deleted");
      qc.invalidateQueries({ queryKey: [ROOT] });
    },
    onError: () => toast.error("Failed to delete profile"),
  });
}

// ---------------------------------------------------------------------------
// Rules
// ---------------------------------------------------------------------------

export function useDomainRoutingRules(profileId: string | undefined) {
  return useQuery<ListDomainRoutingRulesResponse>({
    queryKey: [ROOT, "rules", profileId],
    queryFn: () => routingProfilesService.listRules(profileId!),
    enabled: !!profileId,
  });
}

export function useCreateDomainRoutingRule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      profileId,
      body,
    }: {
      profileId: string;
      body: CreateDomainRoutingRuleRequest;
    }) => routingProfilesService.createRule(profileId, body),
    onSuccess: (data) => {
      toast.success(data.message || "Rule added");
      qc.invalidateQueries({ queryKey: [ROOT] });
    },
    onError: () => toast.error("Failed to add rule"),
  });
}

export function useUpdateDomainRoutingRule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      ruleId,
      body,
    }: {
      ruleId: string;
      body: UpdateDomainRoutingRuleRequest;
    }) => routingProfilesService.updateRule(ruleId, body),
    onSuccess: (data) => {
      toast.success(data.message || "Rule updated");
      qc.invalidateQueries({ queryKey: [ROOT] });
    },
    onError: () => toast.error("Failed to update rule"),
  });
}

export function useDeleteDomainRoutingRule() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (ruleId: string) => routingProfilesService.deleteRule(ruleId),
    onSuccess: (data) => {
      toast.success(data.message || "Rule deleted");
      qc.invalidateQueries({ queryKey: [ROOT] });
    },
    onError: () => toast.error("Failed to delete rule"),
  });
}

// ---------------------------------------------------------------------------
// Device assignment (ordered) + reverse lookup
// ---------------------------------------------------------------------------

export function useDeviceRoutingProfiles(deviceId: string | undefined) {
  return useQuery<GetDeviceRoutingProfilesResponse>({
    queryKey: [ROOT, "device", deviceId],
    queryFn: () => routingProfilesService.getDeviceProfiles(deviceId!),
    enabled: !!deviceId,
  });
}

export function useSetDeviceRoutingProfiles() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      deviceId,
      profileIds,
    }: {
      deviceId: string;
      profileIds: string[];
    }) =>
      routingProfilesService.setDeviceProfiles(deviceId, {
        profile_ids: profileIds,
      }),
    onSuccess: (data) => {
      toast.success(data.message || "Routing profiles updated");
      qc.invalidateQueries({ queryKey: [ROOT] });
    },
    onError: () => toast.error("Failed to update routing profiles"),
  });
}

/** Reverse lookup: the devices a profile is assigned to (its "used by" list). */
export function useProfileDevices(profileId: string | undefined) {
  return useQuery<ListProfileDevicesResponse>({
    queryKey: [ROOT, "profile-devices", profileId],
    queryFn: () => routingProfilesService.listProfileDevices(profileId!),
    enabled: !!profileId,
  });
}
