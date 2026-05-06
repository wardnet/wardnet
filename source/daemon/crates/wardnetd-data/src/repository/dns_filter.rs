//! Repository for the per-device DNS filtering subsystem.
//!
//! Owns the storage half of the DNS Filter Profile model introduced in
//! issue #221. CRUD for profiles, profile-scoped filter sources
//! (blocklists, allowlist, custom rules), per-device kill switch and
//! profile assignments, and the global filter config.
//!
//! This trait sits next to `DnsRepository`, which after the Stage 7 split
//! covers only DNS server config + query log.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use wardnet_common::dns::{AllowlistEntry, Blocklist, CustomFilterRule};
use wardnet_common::dns_filter::{DeviceDnsFilterSettings, DnsFilterConfig, DnsFilterProfile};

/// Inputs needed to compile a single profile's `DnsFilter`.
#[derive(Debug, Clone, Default)]
pub struct ProfileFilterInputs {
    /// Deduplicated, lowercased domains from this profile's enabled blocklists.
    pub blocked_domains: Vec<String>,
    /// Allowlist domains for this profile.
    pub allowlist: Vec<String>,
    /// Raw rule text from this profile's enabled custom rules.
    pub custom_rules: Vec<String>,
}

/// Pair of `(device settings, current IP)` returned by
/// [`DnsFilterRepository::list_device_settings_with_ips`].
#[derive(Debug, Clone)]
pub struct DeviceSettingsWithIp {
    pub settings: DeviceDnsFilterSettings,
    /// Current `last_ip` from the `devices` table, if known.
    pub ip: Option<String>,
}

/// Insert struct for a profile-scoped blocklist.
#[derive(Debug, Clone)]
pub struct BlocklistRow {
    pub id: String,
    pub profile_id: String,
    pub name: String,
    pub url: String,
    pub enabled: bool,
    pub cron_schedule: String,
}

/// Partial update for a blocklist.
#[derive(Debug, Clone, Default)]
pub struct BlocklistUpdate {
    pub name: Option<String>,
    pub url: Option<String>,
    pub enabled: Option<bool>,
    pub cron_schedule: Option<String>,
}

/// Insert struct for a profile-scoped allowlist entry.
#[derive(Debug, Clone)]
pub struct AllowlistRow {
    pub id: String,
    pub profile_id: String,
    pub domain: String,
    pub reason: Option<String>,
}

/// Insert struct for a profile-scoped custom rule.
#[derive(Debug, Clone)]
pub struct CustomRuleRow {
    pub id: String,
    pub profile_id: String,
    pub rule_text: String,
    pub enabled: bool,
    pub comment: Option<String>,
}

/// Partial update for a custom rule.
#[derive(Debug, Clone, Default)]
pub struct CustomRuleUpdate {
    pub rule_text: Option<String>,
    pub enabled: Option<bool>,
    pub comment: Option<String>,
}

/// Insert/update struct for a device's filter settings.
#[derive(Debug, Clone)]
pub struct DeviceSettingsRow {
    pub device_id: String,
    pub enabled: bool,
}

#[async_trait]
pub trait DnsFilterRepository: Send + Sync {
    // ── Profiles ────────────────────────────────────────────────────────

    async fn list_profiles(&self) -> anyhow::Result<Vec<DnsFilterProfile>>;
    async fn get_profile(&self, id: Uuid) -> anyhow::Result<Option<DnsFilterProfile>>;
    async fn create_profile(&self, id: Uuid, name: &str) -> anyhow::Result<DnsFilterProfile>;
    async fn rename_profile(&self, id: Uuid, name: &str) -> anyhow::Result<bool>;
    /// Delete a non-builtin profile. Returns `Ok(false)` if no row matched.
    /// Builtin protection is enforced at the service layer (returns 409).
    async fn delete_profile(&self, id: Uuid) -> anyhow::Result<bool>;

    // ── Profile-scoped blocklists ───────────────────────────────────────

    async fn list_blocklists(&self, profile_id: Uuid) -> anyhow::Result<Vec<Blocklist>>;
    async fn get_blocklist(&self, id: Uuid) -> anyhow::Result<Option<Blocklist>>;
    async fn create_blocklist(&self, row: &BlocklistRow) -> anyhow::Result<()>;
    async fn update_blocklist(&self, id: Uuid, row: &BlocklistUpdate) -> anyhow::Result<()>;
    async fn delete_blocklist(&self, id: Uuid) -> anyhow::Result<bool>;
    async fn replace_blocklist_domains(&self, id: Uuid, domains: &[String]) -> anyhow::Result<u64>;
    async fn set_blocklist_error(&self, id: Uuid, error: Option<&str>) -> anyhow::Result<()>;

    // ── Profile-scoped allowlist ────────────────────────────────────────

    async fn list_allowlist(&self, profile_id: Uuid) -> anyhow::Result<Vec<AllowlistEntry>>;
    async fn create_allowlist_entry(&self, row: &AllowlistRow) -> anyhow::Result<()>;
    async fn delete_allowlist_entry(&self, id: Uuid) -> anyhow::Result<bool>;

    // ── Profile-scoped custom rules ─────────────────────────────────────

    async fn list_custom_rules(&self, profile_id: Uuid) -> anyhow::Result<Vec<CustomFilterRule>>;
    async fn get_custom_rule(&self, id: Uuid) -> anyhow::Result<Option<CustomFilterRule>>;
    async fn create_custom_rule(&self, row: &CustomRuleRow) -> anyhow::Result<()>;
    async fn update_custom_rule(&self, id: Uuid, row: &CustomRuleUpdate) -> anyhow::Result<()>;
    async fn delete_custom_rule(&self, id: Uuid) -> anyhow::Result<bool>;

    /// Aggregate the inputs needed to compile this profile's `DnsFilter`.
    /// Pulls only `enabled = 1` blocklists and rules.
    async fn load_filter_inputs_for_profile(
        &self,
        profile_id: Uuid,
    ) -> anyhow::Result<ProfileFilterInputs>;

    /// Load the domain list backing one blocklist. Empty when the blocklist
    /// has not yet been downloaded or has been disabled.
    async fn load_blocklist_domains(&self, blocklist_id: Uuid) -> anyhow::Result<Vec<String>>;

    // ── Per-device settings ─────────────────────────────────────────────

    /// Fetch a device's filter settings + assigned profile ids.
    /// Returns `None` when the device has no row in `dns_filter_device_settings`
    /// AND no rows in `dns_filter_device_profile`.
    async fn find_device_settings(
        &self,
        device_id: Uuid,
    ) -> anyhow::Result<Option<DeviceDnsFilterSettings>>;

    /// Upsert the device-level kill switch flag.
    async fn upsert_device_settings(&self, row: &DeviceSettingsRow) -> anyhow::Result<()>;

    /// Atomically replace the set of profile assignments for `device_id`.
    async fn set_device_profiles(
        &self,
        device_id: Uuid,
        profile_ids: &[Uuid],
    ) -> anyhow::Result<()>;

    /// All devices that have an explicit settings row OR profile assignment,
    /// joined with their last-known IP. When `only_filtering_active` is `true`,
    /// rows where `enabled = 0` are excluded.
    async fn list_device_settings_with_ips(
        &self,
        only_filtering_active: bool,
    ) -> anyhow::Result<Vec<DeviceSettingsWithIp>>;

    // ── Global filter config ────────────────────────────────────────────

    /// Read `dns_filtering_enabled` + `dns_default_filter_profile_id` from
    /// `system_config`.
    async fn get_dns_filter_config(&self) -> anyhow::Result<DnsFilterConfig>;

    /// Persist both fields. `default_profile_id = None` writes an empty
    /// string to `system_config`, preserving the "unset" semantic.
    async fn set_dns_filter_config(&self, config: &DnsFilterConfig) -> anyhow::Result<()>;
}

/// Helper type alias for repository test fixtures.
pub type ProfileMeta = (Uuid, String, bool, DateTime<Utc>, DateTime<Utc>);
