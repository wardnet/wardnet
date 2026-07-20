// Routing profiles — per-domain routing rules grouped into profiles and
// assigned to devices in priority order (issue #241). Mirrors the Rust
// `wardnet-common` DTOs. CRUD surface under /api/routing/...

/**
 * Where a matched domain's traffic goes. A `tunnel` target routes it through
 * the named tunnel; `direct` carves it out of the device's tunnel back to the
 * WAN. Serialised as an internally-tagged union (`{ "type": "tunnel", ... }`).
 */
export type DomainRoutingTarget = { type: "tunnel"; tunnel_id: string } | { type: "direct" };

/** A named bundle of domain routing rules. */
export interface RoutingProfile {
  id: string;
  name: string;
  created_at: string;
  updated_at: string;
}

/** A single domain rule within a routing profile. */
export interface DomainRoutingRule {
  id: string;
  profile_id: string;
  /**
   * Domain pattern. Either an exact name (`netflix.com`) or a wildcard
   * (`*.netflix.com`) that also covers the apex.
   */
  pattern: string;
  target: DomainRoutingTarget;
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

// --- Profiles ---

export interface ListRoutingProfilesResponse {
  profiles: RoutingProfile[];
}

export interface GetRoutingProfileResponse {
  profile: RoutingProfile;
}

export interface CreateRoutingProfileRequest {
  name: string;
}

export interface CreateRoutingProfileResponse {
  profile: RoutingProfile;
  message: string;
}

export interface UpdateRoutingProfileRequest {
  name?: string;
}

export interface UpdateRoutingProfileResponse {
  profile: RoutingProfile;
  message: string;
}

export interface DeleteRoutingProfileResponse {
  message: string;
}

// --- Rules ---

export interface ListDomainRoutingRulesResponse {
  rules: DomainRoutingRule[];
}

export interface CreateDomainRoutingRuleRequest {
  pattern: string;
  target: DomainRoutingTarget;
  enabled: boolean;
}

export interface CreateDomainRoutingRuleResponse {
  rule: DomainRoutingRule;
  message: string;
}

export interface UpdateDomainRoutingRuleRequest {
  pattern?: string;
  target?: DomainRoutingTarget;
  enabled?: boolean;
}

export interface UpdateDomainRoutingRuleResponse {
  rule: DomainRoutingRule;
  message: string;
}

export interface DeleteDomainRoutingRuleResponse {
  message: string;
}

// --- Device assignment (ordered) ---

export interface GetDeviceRoutingProfilesResponse {
  /** Assigned profile ids, in priority order (first = highest priority). */
  profile_ids: string[];
}

export interface SetDeviceRoutingProfilesRequest {
  /** Profile ids in priority order (first = highest priority). */
  profile_ids: string[];
}

export interface SetDeviceRoutingProfilesResponse {
  message: string;
}
