import type { WardnetClient } from "../client.js";
import type {
  ListProfilesResponse,
  GetProfileResponse,
  CreateProfileRequest,
  CreateProfileResponse,
  UpdateProfileRequest,
  UpdateProfileResponse,
  DeleteProfileResponse,
  ListBlocklistsResponse,
  CreateBlocklistRequest,
  CreateBlocklistResponse,
  UpdateBlocklistRequest,
  UpdateBlocklistResponse,
  DeleteBlocklistResponse,
  ListAllowlistResponse,
  CreateAllowlistRequest,
  CreateAllowlistResponse,
  DeleteAllowlistResponse,
  ListFilterRulesResponse,
  CreateFilterRuleRequest,
  CreateFilterRuleResponse,
  UpdateFilterRuleRequest,
  UpdateFilterRuleResponse,
  DeleteFilterRuleResponse,
  ListDeviceFilterSettingsParams,
  ListDeviceFilterSettingsResponse,
  GetDeviceFilterSettingsResponse,
  UpdateDeviceFilterSettingsRequest,
  UpdateDeviceFilterSettingsResponse,
  DnsFilterConfigResponse,
  UpdateDnsFilterConfigRequest,
} from "../types/dns-filter.js";
import type { JobDispatchedResponse } from "../types/jobs.js";

/** DNS filtering management — profiles, sources, per-device assignments. */
export class DnsFilterService {
  constructor(private readonly client: WardnetClient) {}

  // --- Profiles ---

  /** List every DNS filter profile (admin only). */
  async listProfiles(): Promise<ListProfilesResponse> {
    return this.client.request<ListProfilesResponse>("/dns/filter/profiles");
  }

  /** Fetch a single DNS filter profile (admin only). */
  async getProfile(profileId: string): Promise<GetProfileResponse> {
    return this.client.request<GetProfileResponse>(`/dns/filter/profiles/${profileId}`);
  }

  /** Create a new DNS filter profile (admin only). */
  async createProfile(body: CreateProfileRequest): Promise<CreateProfileResponse> {
    return this.client.request<CreateProfileResponse>("/dns/filter/profiles", {
      method: "POST",
      body: JSON.stringify(body),
    });
  }

  /** Rename a DNS filter profile (admin only). */
  async updateProfile(
    profileId: string,
    body: UpdateProfileRequest,
  ): Promise<UpdateProfileResponse> {
    return this.client.request<UpdateProfileResponse>(`/dns/filter/profiles/${profileId}`, {
      method: "PUT",
      body: JSON.stringify(body),
    });
  }

  /** Delete a non-builtin DNS filter profile. Returns 409 for builtin profiles (admin only). */
  async deleteProfile(profileId: string): Promise<DeleteProfileResponse> {
    return this.client.request<DeleteProfileResponse>(`/dns/filter/profiles/${profileId}`, {
      method: "DELETE",
    });
  }

  // --- Profile-scoped blocklists ---

  /** List every blocklist scoped to a profile (admin only). */
  async listBlocklists(profileId: string): Promise<ListBlocklistsResponse> {
    return this.client.request<ListBlocklistsResponse>(
      `/dns/filter/profiles/${profileId}/blocklists`,
    );
  }

  /** Add a blocklist under a profile (admin only). */
  async createBlocklist(
    profileId: string,
    body: CreateBlocklistRequest,
  ): Promise<CreateBlocklistResponse> {
    return this.client.request<CreateBlocklistResponse>(
      `/dns/filter/profiles/${profileId}/blocklists`,
      {
        method: "POST",
        body: JSON.stringify(body),
      },
    );
  }

  /** Update a blocklist within a profile (admin only). */
  async updateBlocklist(
    profileId: string,
    id: string,
    body: UpdateBlocklistRequest,
  ): Promise<UpdateBlocklistResponse> {
    return this.client.request<UpdateBlocklistResponse>(
      `/dns/filter/profiles/${profileId}/blocklists/${id}`,
      {
        method: "PUT",
        body: JSON.stringify(body),
      },
    );
  }

  /** Delete a blocklist from a profile (admin only). */
  async deleteBlocklist(profileId: string, id: string): Promise<DeleteBlocklistResponse> {
    return this.client.request<DeleteBlocklistResponse>(
      `/dns/filter/profiles/${profileId}/blocklists/${id}`,
      {
        method: "DELETE",
      },
    );
  }

  /** Trigger an immediate blocklist refresh job (admin only).
   *
   *  Dispatches a background job that fetches, parses, and stores the
   *  blocklist. Returns immediately with the job id; poll `JobsService.get`
   *  for progress and completion. */
  async refreshBlocklist(profileId: string, id: string): Promise<JobDispatchedResponse> {
    return this.client.request<JobDispatchedResponse>(
      `/dns/filter/profiles/${profileId}/blocklists/${id}/refresh`,
      {
        method: "POST",
      },
    );
  }

  // --- Profile-scoped allowlist ---

  /** List allowlist entries scoped to a profile (admin only). */
  async listAllowlist(profileId: string): Promise<ListAllowlistResponse> {
    return this.client.request<ListAllowlistResponse>(
      `/dns/filter/profiles/${profileId}/allowlist`,
    );
  }

  /** Add a domain to a profile's allowlist (admin only). */
  async createAllowlistEntry(
    profileId: string,
    body: CreateAllowlistRequest,
  ): Promise<CreateAllowlistResponse> {
    return this.client.request<CreateAllowlistResponse>(
      `/dns/filter/profiles/${profileId}/allowlist`,
      {
        method: "POST",
        body: JSON.stringify(body),
      },
    );
  }

  /** Remove a domain from a profile's allowlist (admin only). */
  async deleteAllowlistEntry(profileId: string, id: string): Promise<DeleteAllowlistResponse> {
    return this.client.request<DeleteAllowlistResponse>(
      `/dns/filter/profiles/${profileId}/allowlist/${id}`,
      {
        method: "DELETE",
      },
    );
  }

  // --- Profile-scoped custom rules ---

  /** List a profile's custom filter rules (admin only). */
  async listFilterRules(profileId: string): Promise<ListFilterRulesResponse> {
    return this.client.request<ListFilterRulesResponse>(`/dns/filter/profiles/${profileId}/rules`);
  }

  /** Add a custom filter rule to a profile (admin only). */
  async createFilterRule(
    profileId: string,
    body: CreateFilterRuleRequest,
  ): Promise<CreateFilterRuleResponse> {
    return this.client.request<CreateFilterRuleResponse>(
      `/dns/filter/profiles/${profileId}/rules`,
      {
        method: "POST",
        body: JSON.stringify(body),
      },
    );
  }

  /** Update a custom filter rule (admin only). */
  async updateFilterRule(
    profileId: string,
    id: string,
    body: UpdateFilterRuleRequest,
  ): Promise<UpdateFilterRuleResponse> {
    return this.client.request<UpdateFilterRuleResponse>(
      `/dns/filter/profiles/${profileId}/rules/${id}`,
      {
        method: "PUT",
        body: JSON.stringify(body),
      },
    );
  }

  /** Delete a custom filter rule from a profile (admin only). */
  async deleteFilterRule(profileId: string, id: string): Promise<DeleteFilterRuleResponse> {
    return this.client.request<DeleteFilterRuleResponse>(
      `/dns/filter/profiles/${profileId}/rules/${id}`,
      {
        method: "DELETE",
      },
    );
  }

  // --- Per-device filter settings ---

  /** List devices with explicit DNS filter settings or profile assignments
   *  (admin only). Pass `{ enabled: false }` to restrict to devices where the
   *  kill switch is off. */
  async listDeviceSettings(
    params: ListDeviceFilterSettingsParams = {},
  ): Promise<ListDeviceFilterSettingsResponse> {
    const parts: string[] = [];
    if (params.enabled !== undefined) parts.push(`enabled=${params.enabled}`);
    const path =
      parts.length === 0 ? "/dns/filter/devices" : `/dns/filter/devices?${parts.join("&")}`;
    return this.client.request<ListDeviceFilterSettingsResponse>(path);
  }

  /** Get a device's DNS filter settings (admin only). */
  async getDeviceSettings(deviceId: string): Promise<GetDeviceFilterSettingsResponse> {
    return this.client.request<GetDeviceFilterSettingsResponse>(`/dns/filter/devices/${deviceId}`);
  }

  /** Update a device's DNS filter settings — kill switch + profile assignments
   *  (admin only). */
  async updateDeviceSettings(
    deviceId: string,
    body: UpdateDeviceFilterSettingsRequest,
  ): Promise<UpdateDeviceFilterSettingsResponse> {
    return this.client.request<UpdateDeviceFilterSettingsResponse>(
      `/dns/filter/devices/${deviceId}`,
      {
        method: "PUT",
        body: JSON.stringify(body),
      },
    );
  }

  // --- Global filter config ---

  /** Read the global DNS filter config — emergency stop + default profile pointer
   *  (admin only). */
  async getConfig(): Promise<DnsFilterConfigResponse> {
    return this.client.request<DnsFilterConfigResponse>("/dns/filter/config");
  }

  /** Update the global DNS filter config (admin only). */
  async updateConfig(body: UpdateDnsFilterConfigRequest): Promise<DnsFilterConfigResponse> {
    return this.client.request<DnsFilterConfigResponse>("/dns/filter/config", {
      method: "PUT",
      body: JSON.stringify(body),
    });
  }
}
