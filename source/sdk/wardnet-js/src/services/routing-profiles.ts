import type { WardnetClient } from "../client.js";
import type {
  ListRoutingProfilesResponse,
  GetRoutingProfileResponse,
  CreateRoutingProfileRequest,
  CreateRoutingProfileResponse,
  UpdateRoutingProfileRequest,
  UpdateRoutingProfileResponse,
  DeleteRoutingProfileResponse,
  ListDomainRoutingRulesResponse,
  CreateDomainRoutingRuleRequest,
  CreateDomainRoutingRuleResponse,
  UpdateDomainRoutingRuleRequest,
  UpdateDomainRoutingRuleResponse,
  DeleteDomainRoutingRuleResponse,
  GetDeviceRoutingProfilesResponse,
  SetDeviceRoutingProfilesRequest,
  SetDeviceRoutingProfilesResponse,
  ListProfileDevicesResponse,
} from "../types/routing-profiles.js";

/**
 * Routing profiles — per-domain routing rules grouped into profiles and
 * assigned to devices in priority order (issue #241). Admin only.
 */
export class RoutingProfilesService {
  constructor(private readonly client: WardnetClient) {}

  // --- Profiles ---

  /** List every routing profile. */
  async listProfiles(): Promise<ListRoutingProfilesResponse> {
    return this.client.request<ListRoutingProfilesResponse>("/routing/profiles");
  }

  /** Fetch a single routing profile. */
  async getProfile(profileId: string): Promise<GetRoutingProfileResponse> {
    return this.client.request<GetRoutingProfileResponse>(`/routing/profiles/${profileId}`);
  }

  /** Create a routing profile. */
  async createProfile(body: CreateRoutingProfileRequest): Promise<CreateRoutingProfileResponse> {
    return this.client.request<CreateRoutingProfileResponse>("/routing/profiles", {
      method: "POST",
      body: JSON.stringify(body),
    });
  }

  /** Rename a routing profile. */
  async updateProfile(
    profileId: string,
    body: UpdateRoutingProfileRequest,
  ): Promise<UpdateRoutingProfileResponse> {
    return this.client.request<UpdateRoutingProfileResponse>(`/routing/profiles/${profileId}`, {
      method: "PUT",
      body: JSON.stringify(body),
    });
  }

  /** Delete a routing profile and its rules and assignments. */
  async deleteProfile(profileId: string): Promise<DeleteRoutingProfileResponse> {
    return this.client.request<DeleteRoutingProfileResponse>(`/routing/profiles/${profileId}`, {
      method: "DELETE",
    });
  }

  // --- Rules ---

  /** List the domain rules in a routing profile. */
  async listRules(profileId: string): Promise<ListDomainRoutingRulesResponse> {
    return this.client.request<ListDomainRoutingRulesResponse>(
      `/routing/profiles/${profileId}/rules`,
    );
  }

  /** Add a domain rule to a routing profile. */
  async createRule(
    profileId: string,
    body: CreateDomainRoutingRuleRequest,
  ): Promise<CreateDomainRoutingRuleResponse> {
    return this.client.request<CreateDomainRoutingRuleResponse>(
      `/routing/profiles/${profileId}/rules`,
      { method: "POST", body: JSON.stringify(body) },
    );
  }

  /** Update a domain routing rule. */
  async updateRule(
    ruleId: string,
    body: UpdateDomainRoutingRuleRequest,
  ): Promise<UpdateDomainRoutingRuleResponse> {
    return this.client.request<UpdateDomainRoutingRuleResponse>(`/routing/rules/${ruleId}`, {
      method: "PUT",
      body: JSON.stringify(body),
    });
  }

  /** Delete a domain routing rule. */
  async deleteRule(ruleId: string): Promise<DeleteDomainRoutingRuleResponse> {
    return this.client.request<DeleteDomainRoutingRuleResponse>(`/routing/rules/${ruleId}`, {
      method: "DELETE",
    });
  }

  // --- Device assignment (ordered) ---

  /** List a device's assigned routing profiles, in priority order. */
  async getDeviceProfiles(deviceId: string): Promise<GetDeviceRoutingProfilesResponse> {
    return this.client.request<GetDeviceRoutingProfilesResponse>(
      `/routing/devices/${deviceId}/profiles`,
    );
  }

  /**
   * Replace a device's routing-profile assignment. The array order is the
   * priority (first = highest).
   */
  async setDeviceProfiles(
    deviceId: string,
    body: SetDeviceRoutingProfilesRequest,
  ): Promise<SetDeviceRoutingProfilesResponse> {
    return this.client.request<SetDeviceRoutingProfilesResponse>(
      `/routing/devices/${deviceId}/profiles`,
      { method: "PUT", body: JSON.stringify(body) },
    );
  }

  /** List the devices a routing profile is currently assigned to. */
  async listProfileDevices(profileId: string): Promise<ListProfileDevicesResponse> {
    return this.client.request<ListProfileDevicesResponse>(
      `/routing/profiles/${profileId}/devices`,
    );
  }
}
