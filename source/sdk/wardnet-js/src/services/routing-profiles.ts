import type { WardnetClient } from "../client.js";
import { apiClient, type ApiClient } from "../internal/client.js";
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
  private readonly api: ApiClient;

  constructor(client: WardnetClient) {
    this.api = apiClient(client);
  }

  // --- Profiles ---

  /** List every routing profile. */
  async listProfiles(): Promise<ListRoutingProfilesResponse> {
    return this.api.get("/routing/profiles");
  }

  /** Fetch a single routing profile. */
  async getProfile(profileId: string): Promise<GetRoutingProfileResponse> {
    return this.api.get("/routing/profiles/{id}", { path: { id: profileId } });
  }

  /** Create a routing profile. */
  async createProfile(body: CreateRoutingProfileRequest): Promise<CreateRoutingProfileResponse> {
    return this.api.post("/routing/profiles", { body });
  }

  /** Rename a routing profile. */
  async updateProfile(
    profileId: string,
    body: UpdateRoutingProfileRequest,
  ): Promise<UpdateRoutingProfileResponse> {
    return this.api.put("/routing/profiles/{id}", { path: { id: profileId }, body });
  }

  /** Delete a routing profile and its rules and assignments. */
  async deleteProfile(profileId: string): Promise<DeleteRoutingProfileResponse> {
    return this.api.del("/routing/profiles/{id}", { path: { id: profileId } });
  }

  // --- Rules ---

  /** List the domain rules in a routing profile. */
  async listRules(profileId: string): Promise<ListDomainRoutingRulesResponse> {
    return this.api.get("/routing/profiles/{id}/rules", { path: { id: profileId } });
  }

  /** Add a domain rule to a routing profile. */
  async createRule(
    profileId: string,
    body: CreateDomainRoutingRuleRequest,
  ): Promise<CreateDomainRoutingRuleResponse> {
    return this.api.post("/routing/profiles/{id}/rules", { path: { id: profileId }, body });
  }

  /** Update a domain routing rule. */
  async updateRule(
    ruleId: string,
    body: UpdateDomainRoutingRuleRequest,
  ): Promise<UpdateDomainRoutingRuleResponse> {
    return this.api.put("/routing/rules/{id}", { path: { id: ruleId }, body });
  }

  /** Delete a domain routing rule. */
  async deleteRule(ruleId: string): Promise<DeleteDomainRoutingRuleResponse> {
    return this.api.del("/routing/rules/{id}", { path: { id: ruleId } });
  }

  // --- Device assignment (ordered) ---

  /** List a device's assigned routing profiles, in priority order. */
  async getDeviceProfiles(deviceId: string): Promise<GetDeviceRoutingProfilesResponse> {
    return this.api.get("/routing/devices/{device_id}/profiles", { path: { device_id: deviceId } });
  }

  /**
   * Replace a device's routing-profile assignment. The array order is the
   * priority (first = highest).
   */
  async setDeviceProfiles(
    deviceId: string,
    body: SetDeviceRoutingProfilesRequest,
  ): Promise<SetDeviceRoutingProfilesResponse> {
    return this.api.put("/routing/devices/{device_id}/profiles", {
      path: { device_id: deviceId },
      body,
    });
  }

  /** List the devices a routing profile is currently assigned to. */
  async listProfileDevices(profileId: string): Promise<ListProfileDevicesResponse> {
    return this.api.get("/routing/profiles/{id}/devices", { path: { id: profileId } });
  }
}
