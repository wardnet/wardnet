// ---------------------------------------------------------------------------
// DNS Filter — domain types
// ---------------------------------------------------------------------------

/** A named bundle of DNS filter sources (blocklists, allowlist, custom rules). */
export interface DnsFilterProfile {
  id: string;
  name: string;
  builtin: boolean;
  created_at: string;
  updated_at: string;
}

/**
 * Per-device DNS filtering settings.
 *
 * `enabled = false` is the kill switch; the device's queries skip filtering
 * entirely. When `enabled = true` and `profile_ids` is empty, the device
 * inherits the global default profile from `DnsFilterConfig`.
 */
export interface DeviceDnsFilterSettings {
  device_id: string;
  enabled: boolean;
  /** Explicit profile assignments. Empty means "follow the default profile". */
  profile_ids: string[];
  updated_at: string;
}

/** Global DNS filtering configuration (emergency stop + default profile). */
export interface DnsFilterConfig {
  /** Global emergency stop. When `false`, every query short-circuits to `Pass`. */
  enabled: boolean;
  /** Profile applied to devices with no explicit assignment. `null` = unfiltered. */
  default_profile_id: string | null;
}

/** A URL-sourced domain blocklist scoped to a DNS filter profile. */
export interface Blocklist {
  id: string;
  profile_id: string;
  name: string;
  url: string;
  enabled: boolean;
  entry_count: number;
  last_updated: string | null;
  cron_schedule: string;
  last_error: string | null;
  last_error_at: string | null;
  created_at: string;
  updated_at: string;
}

/** An allowlist entry, scoped to a profile, that overrides blocklist matches. */
export interface AllowlistEntry {
  id: string;
  profile_id: string;
  domain: string;
  reason: string | null;
  created_at: string;
}

/** A user-created AdGuard-syntax filter rule, scoped to a profile. */
export interface CustomFilterRule {
  id: string;
  profile_id: string;
  rule_text: string;
  enabled: boolean;
  comment: string | null;
  created_at: string;
  updated_at: string;
}

// ---------------------------------------------------------------------------
// Profile CRUD
// ---------------------------------------------------------------------------

export interface ListProfilesResponse {
  profiles: DnsFilterProfile[];
}

export interface GetProfileResponse {
  profile: DnsFilterProfile;
}

export interface CreateProfileRequest {
  name: string;
}

export interface CreateProfileResponse {
  profile: DnsFilterProfile;
  message: string;
}

export interface UpdateProfileRequest {
  name?: string;
}

export interface UpdateProfileResponse {
  profile: DnsFilterProfile;
  message: string;
}

export interface DeleteProfileResponse {
  message: string;
}

// ---------------------------------------------------------------------------
// Profile-scoped blocklists / allowlist / custom rules
// ---------------------------------------------------------------------------

export interface ListBlocklistsResponse {
  blocklists: Blocklist[];
}

export interface CreateBlocklistRequest {
  name: string;
  url: string;
  cron_schedule: string;
  enabled: boolean;
}

export interface CreateBlocklistResponse {
  blocklist: Blocklist;
  message: string;
}

export interface UpdateBlocklistRequest {
  name?: string;
  url?: string;
  cron_schedule?: string;
  enabled?: boolean;
}

export interface UpdateBlocklistResponse {
  blocklist: Blocklist;
  message: string;
}

export interface DeleteBlocklistResponse {
  message: string;
}

export interface ListAllowlistResponse {
  entries: AllowlistEntry[];
}

export interface CreateAllowlistRequest {
  domain: string;
  reason?: string;
}

export interface CreateAllowlistResponse {
  entry: AllowlistEntry;
  message: string;
}

export interface DeleteAllowlistResponse {
  message: string;
}

export interface ListFilterRulesResponse {
  rules: CustomFilterRule[];
}

export interface CreateFilterRuleRequest {
  rule_text: string;
  comment?: string;
  enabled: boolean;
}

export interface CreateFilterRuleResponse {
  rule: CustomFilterRule;
  message: string;
}

export interface UpdateFilterRuleRequest {
  rule_text?: string;
  comment?: string;
  enabled?: boolean;
}

export interface UpdateFilterRuleResponse {
  rule: CustomFilterRule;
  message: string;
}

export interface DeleteFilterRuleResponse {
  message: string;
}

// ---------------------------------------------------------------------------
// Per-device filter settings
// ---------------------------------------------------------------------------

export interface ListDeviceFilterSettingsParams {
  /** When `false`, restrict to devices where the kill switch is off. */
  enabled?: boolean;
}

export interface ListDeviceFilterSettingsResponse {
  devices: DeviceDnsFilterSettings[];
}

export interface GetDeviceFilterSettingsResponse {
  settings: DeviceDnsFilterSettings;
}

export interface UpdateDeviceFilterSettingsRequest {
  enabled: boolean;
  profile_ids: string[];
}

export interface UpdateDeviceFilterSettingsResponse {
  settings: DeviceDnsFilterSettings;
  message: string;
}

// ---------------------------------------------------------------------------
// Global filter config
// ---------------------------------------------------------------------------

export interface DnsFilterConfigResponse {
  config: DnsFilterConfig;
}

/**
 * Request body for PUT /api/dns/filter/config.
 *
 * `default_profile_id` honors three states: omitted = no change, `null` =
 * clear the default, `"<uuid>"` = set.
 */
export interface UpdateDnsFilterConfigRequest {
  enabled?: boolean;
  default_profile_id?: string | null;
}
