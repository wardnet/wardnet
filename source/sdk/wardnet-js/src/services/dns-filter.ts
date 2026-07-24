import type { WardnetClient } from "../client.js";
import { apiClient, type ApiClient } from "../internal/client.js";
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
  private readonly api: ApiClient;

  constructor(client: WardnetClient) {
    this.api = apiClient(client);
  }

  // --- Profiles ---

  /** List every DNS filter profile (admin only). */
  async listProfiles(): Promise<ListProfilesResponse> {
    return this.api.get("/dns/filter/profiles");
  }

  /** Fetch a single DNS filter profile (admin only). */
  async getProfile(profileId: string): Promise<GetProfileResponse> {
    return this.api.get("/dns/filter/profiles/{profile_id}", { path: { profile_id: profileId } });
  }

  /** Create a new DNS filter profile (admin only). */
  async createProfile(body: CreateProfileRequest): Promise<CreateProfileResponse> {
    return this.api.post("/dns/filter/profiles", { body });
  }

  /** Rename a DNS filter profile (admin only). */
  async updateProfile(
    profileId: string,
    body: UpdateProfileRequest,
  ): Promise<UpdateProfileResponse> {
    return this.api.put("/dns/filter/profiles/{profile_id}", {
      path: { profile_id: profileId },
      body,
    });
  }

  /** Delete a non-builtin DNS filter profile. Returns 409 for builtin profiles (admin only). */
  async deleteProfile(profileId: string): Promise<DeleteProfileResponse> {
    return this.api.del("/dns/filter/profiles/{profile_id}", { path: { profile_id: profileId } });
  }

  // --- Profile-scoped blocklists ---

  /** List every blocklist scoped to a profile (admin only). */
  async listBlocklists(profileId: string): Promise<ListBlocklistsResponse> {
    return this.api.get("/dns/filter/profiles/{profile_id}/blocklists", {
      path: { profile_id: profileId },
    });
  }

  /** Add a blocklist under a profile (admin only). */
  async createBlocklist(
    profileId: string,
    body: CreateBlocklistRequest,
  ): Promise<CreateBlocklistResponse> {
    return this.api.post("/dns/filter/profiles/{profile_id}/blocklists", {
      path: { profile_id: profileId },
      body,
    });
  }

  /** Update a blocklist within a profile (admin only). */
  async updateBlocklist(
    profileId: string,
    id: string,
    body: UpdateBlocklistRequest,
  ): Promise<UpdateBlocklistResponse> {
    return this.api.put("/dns/filter/profiles/{profile_id}/blocklists/{id}", {
      path: { profile_id: profileId, id },
      body,
    });
  }

  /** Delete a blocklist from a profile (admin only). */
  async deleteBlocklist(profileId: string, id: string): Promise<DeleteBlocklistResponse> {
    return this.api.del("/dns/filter/profiles/{profile_id}/blocklists/{id}", {
      path: { profile_id: profileId, id },
    });
  }

  /** Trigger an immediate blocklist refresh job (admin only).
   *
   *  Dispatches a background job that fetches, parses, and stores the
   *  blocklist. Returns immediately with the job id; poll `JobsService.get`
   *  for progress and completion. */
  async refreshBlocklist(profileId: string, id: string): Promise<JobDispatchedResponse> {
    return this.api.post("/dns/filter/profiles/{profile_id}/blocklists/{id}/refresh", {
      path: { profile_id: profileId, id },
    });
  }

  // --- Profile-scoped allowlist ---

  /** List allowlist entries scoped to a profile (admin only). */
  async listAllowlist(profileId: string): Promise<ListAllowlistResponse> {
    return this.api.get("/dns/filter/profiles/{profile_id}/allowlist", {
      path: { profile_id: profileId },
    });
  }

  /** Add a domain to a profile's allowlist (admin only). */
  async createAllowlistEntry(
    profileId: string,
    body: CreateAllowlistRequest,
  ): Promise<CreateAllowlistResponse> {
    return this.api.post("/dns/filter/profiles/{profile_id}/allowlist", {
      path: { profile_id: profileId },
      body,
    });
  }

  /** Remove a domain from a profile's allowlist (admin only). */
  async deleteAllowlistEntry(profileId: string, id: string): Promise<DeleteAllowlistResponse> {
    return this.api.del("/dns/filter/profiles/{profile_id}/allowlist/{id}", {
      path: { profile_id: profileId, id },
    });
  }

  // --- Profile-scoped custom rules ---

  /** List a profile's custom filter rules (admin only). */
  async listFilterRules(profileId: string): Promise<ListFilterRulesResponse> {
    return this.api.get("/dns/filter/profiles/{profile_id}/rules", {
      path: { profile_id: profileId },
    });
  }

  /** Add a custom filter rule to a profile (admin only). */
  async createFilterRule(
    profileId: string,
    body: CreateFilterRuleRequest,
  ): Promise<CreateFilterRuleResponse> {
    return this.api.post("/dns/filter/profiles/{profile_id}/rules", {
      path: { profile_id: profileId },
      body,
    });
  }

  /** Update a custom filter rule (admin only). */
  async updateFilterRule(
    profileId: string,
    id: string,
    body: UpdateFilterRuleRequest,
  ): Promise<UpdateFilterRuleResponse> {
    return this.api.put("/dns/filter/profiles/{profile_id}/rules/{id}", {
      path: { profile_id: profileId, id },
      body,
    });
  }

  /** Delete a custom filter rule from a profile (admin only). */
  async deleteFilterRule(profileId: string, id: string): Promise<DeleteFilterRuleResponse> {
    return this.api.del("/dns/filter/profiles/{profile_id}/rules/{id}", {
      path: { profile_id: profileId, id },
    });
  }

  // --- Per-device filter settings ---

  /** List devices with explicit DNS filter settings or profile assignments
   *  (admin only). Pass `{ enabled: false }` to restrict to devices where the
   *  kill switch is off. */
  async listDeviceSettings(
    params: ListDeviceFilterSettingsParams = {},
  ): Promise<ListDeviceFilterSettingsResponse> {
    return this.api.get("/dns/filter/devices", { query: params });
  }

  /** Get a device's DNS filter settings (admin only). */
  async getDeviceSettings(deviceId: string): Promise<GetDeviceFilterSettingsResponse> {
    return this.api.get("/dns/filter/devices/{device_id}", { path: { device_id: deviceId } });
  }

  /** Update a device's DNS filter settings — kill switch + profile assignments
   *  (admin only). */
  async updateDeviceSettings(
    deviceId: string,
    body: UpdateDeviceFilterSettingsRequest,
  ): Promise<UpdateDeviceFilterSettingsResponse> {
    return this.api.put("/dns/filter/devices/{device_id}", {
      path: { device_id: deviceId },
      body,
    });
  }

  // --- Global filter config ---

  /** Read the global DNS filter config — emergency stop + default profile pointer
   *  (admin only). */
  async getConfig(): Promise<DnsFilterConfigResponse> {
    return this.api.get("/dns/filter/config");
  }

  /** Update the global DNS filter config (admin only). */
  async updateConfig(body: UpdateDnsFilterConfigRequest): Promise<DnsFilterConfigResponse> {
    return this.api.put("/dns/filter/config", { body });
  }
}
