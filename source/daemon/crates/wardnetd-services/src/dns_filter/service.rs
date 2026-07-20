//! [`DnsFilterService`] — the public API + hot-path entry point for the
//! per-device DNS filtering subsystem (issue #221).
//!
//! Owns:
//! - profile / blocklist / allowlist / custom-rule CRUD (admin-gated)
//! - per-device filter settings (kill switch + profile assignments)
//! - the global emergency stop + default-profile pointer
//! - the multi-level runtime cache the DNS server hot path consults
//!
//! Hot path: [`Self::check`] returns a [`FilterAction`] plus a
//! `would_have_blocked` hint so the server can stamp
//! `dns_query_log.result = "blocked_skipped"` for queries the kill switch
//! suppressed.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;

use arc_swap::ArcSwap;
use async_trait::async_trait;
use chrono::Utc;
use hickory_proto::rr::RecordType;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::{RwLock, Semaphore};
use uuid::Uuid;

use wardnet_common::api::{
    CreateAllowlistRequest, CreateAllowlistResponse, CreateBlocklistRequest,
    CreateBlocklistResponse, CreateFilterRuleRequest, CreateFilterRuleResponse,
    CreateProfileRequest, CreateProfileResponse, DeleteAllowlistResponse, DeleteBlocklistResponse,
    DeleteFilterRuleResponse, DeleteProfileResponse, DnsFilterConfigResponse,
    GetDeviceFilterSettingsResponse, GetProfileResponse, ListAllowlistResponse,
    ListBlocklistsResponse, ListDeviceFilterSettingsParams, ListDeviceFilterSettingsResponse,
    ListFilterRulesResponse, ListProfilesResponse, UpdateBlocklistRequest, UpdateBlocklistResponse,
    UpdateDeviceFilterSettingsRequest, UpdateDeviceFilterSettingsResponse,
    UpdateDnsFilterConfigRequest, UpdateFilterRuleRequest, UpdateFilterRuleResponse,
    UpdateProfileRequest, UpdateProfileResponse,
};
use wardnet_common::dns::FilterAction;
use wardnet_common::dns_filter::{DeviceDnsFilterSettings, DnsFilterConfig, DnsFilterProfile};
use wardnet_common::event::{DnsFilterChange, WardnetEvent};
use wardnet_common::jobs::{JobDispatchedResponse, JobKind};

use wardnetd_data::repository::{
    AllowlistRow, BlocklistRow, BlocklistUpdate, CustomRuleRow, CustomRuleUpdate, DeviceRepository,
    DeviceSettingsRow, DnsFilterRepository,
};

use crate::auth_context;
use crate::dns_filter::blocklist_downloader::{self, BlocklistFetcher};
use crate::dns_filter::device_context::DeviceFilterContext;
use crate::dns_filter::filter::{DnsFilter, DnsFilterInputs};
use crate::dns_filter::filter_profile::RuntimeDnsFilterProfile;
use crate::error::AppError;
use crate::event::EventPublisher;
use crate::jobs::{JobService, JobServiceExt};

/// IDs of the three builtin profiles seeded by the migration.
const BUILTIN_AD_BLOCKING: &str = "00000000-0000-0000-0000-000000000100";

/// Progress reported by a refresh job while it waits for the import permit, so
/// a queued job is visibly queued rather than frozen at zero.
const QUEUED_PROGRESS: u8 = 1;

/// Outcome of a hot-path filter check.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CheckOutcome {
    pub action: FilterAction,
    /// `true` when the filter pipeline *would* have blocked the query but
    /// the per-device kill switch (or the global emergency stop) suppressed
    /// it — surfaced in the query log as `result = "blocked_skipped"`.
    pub would_have_blocked: bool,
}

#[async_trait]
pub trait DnsFilterService: Send + Sync {
    // ── Hot path ────────────────────────────────────────────────────────

    /// Evaluate one query. Called from the DNS server hot path on every
    /// request — must not allocate or take heavy locks.
    async fn check(&self, domain: &str, qtype: RecordType, client: IpAddr) -> CheckOutcome;

    /// Evaluate one query for a device the transport authenticated out of
    /// band (the `DoT` listener's SNI token, issue #912). Prefers the
    /// device-keyed context so the granted device's own profiles apply even
    /// when `client` is not a LAN address the per-IP map covers (e.g. a
    /// relayed roaming connection); falls back to the per-IP path, so a LAN
    /// `DoT` client with a real source IP behaves exactly like its UDP self.
    ///
    /// The default delegates to [`Self::check`] — only implementations that
    /// maintain a device-keyed cache need to override it.
    async fn check_for_device(
        &self,
        domain: &str,
        qtype: RecordType,
        _device_id: Uuid,
        client: IpAddr,
    ) -> CheckOutcome {
        self.check(domain, qtype, client).await
    }

    /// Bootstrap the in-memory cache from persisted state. Called once on
    /// daemon startup, from the DNS filter runner's own task — the DNS server
    /// binds and starts answering *before* this completes, so queries during
    /// the window resolve through an empty filter set (fail-open). Callers can
    /// observe that window via `filters_ready` on the config response.
    async fn rebuild_all(&self) -> Result<(), AppError>;

    // ── Profiles ────────────────────────────────────────────────────────

    async fn list_profiles(&self) -> Result<ListProfilesResponse, AppError>;
    async fn get_profile(&self, id: Uuid) -> Result<GetProfileResponse, AppError>;
    async fn create_profile(
        &self,
        req: CreateProfileRequest,
    ) -> Result<CreateProfileResponse, AppError>;
    async fn update_profile(
        &self,
        id: Uuid,
        req: UpdateProfileRequest,
    ) -> Result<UpdateProfileResponse, AppError>;
    async fn delete_profile(&self, id: Uuid) -> Result<DeleteProfileResponse, AppError>;

    // ── Profile-scoped blocklists ───────────────────────────────────────

    async fn list_blocklists(&self, profile_id: Uuid) -> Result<ListBlocklistsResponse, AppError>;
    async fn create_blocklist(
        &self,
        profile_id: Uuid,
        req: CreateBlocklistRequest,
    ) -> Result<CreateBlocklistResponse, AppError>;
    async fn update_blocklist(
        &self,
        profile_id: Uuid,
        id: Uuid,
        req: UpdateBlocklistRequest,
    ) -> Result<UpdateBlocklistResponse, AppError>;
    async fn delete_blocklist(
        &self,
        profile_id: Uuid,
        id: Uuid,
    ) -> Result<DeleteBlocklistResponse, AppError>;
    async fn refresh_blocklist(
        &self,
        profile_id: Uuid,
        id: Uuid,
    ) -> Result<JobDispatchedResponse, AppError>;

    // ── Profile-scoped allowlist ────────────────────────────────────────

    async fn list_allowlist(&self, profile_id: Uuid) -> Result<ListAllowlistResponse, AppError>;
    async fn create_allowlist_entry(
        &self,
        profile_id: Uuid,
        req: CreateAllowlistRequest,
    ) -> Result<CreateAllowlistResponse, AppError>;
    async fn delete_allowlist_entry(
        &self,
        profile_id: Uuid,
        id: Uuid,
    ) -> Result<DeleteAllowlistResponse, AppError>;

    // ── Profile-scoped custom rules ─────────────────────────────────────

    async fn list_custom_rules(
        &self,
        profile_id: Uuid,
    ) -> Result<ListFilterRulesResponse, AppError>;
    async fn create_custom_rule(
        &self,
        profile_id: Uuid,
        req: CreateFilterRuleRequest,
    ) -> Result<CreateFilterRuleResponse, AppError>;
    async fn update_custom_rule(
        &self,
        profile_id: Uuid,
        id: Uuid,
        req: UpdateFilterRuleRequest,
    ) -> Result<UpdateFilterRuleResponse, AppError>;
    async fn delete_custom_rule(
        &self,
        profile_id: Uuid,
        id: Uuid,
    ) -> Result<DeleteFilterRuleResponse, AppError>;

    // ── Per-device settings ─────────────────────────────────────────────

    async fn list_device_settings(
        &self,
        params: ListDeviceFilterSettingsParams,
    ) -> Result<ListDeviceFilterSettingsResponse, AppError>;
    async fn get_device_settings(
        &self,
        device_id: Uuid,
    ) -> Result<GetDeviceFilterSettingsResponse, AppError>;
    async fn update_device_settings(
        &self,
        device_id: Uuid,
        req: UpdateDeviceFilterSettingsRequest,
    ) -> Result<UpdateDeviceFilterSettingsResponse, AppError>;

    // ── Global config ───────────────────────────────────────────────────

    async fn get_filter_config(&self) -> Result<DnsFilterConfigResponse, AppError>;
    async fn update_filter_config(
        &self,
        req: UpdateDnsFilterConfigRequest,
    ) -> Result<DnsFilterConfigResponse, AppError>;

    // ── Internal: rebuild hooks for the runner ──────────────────────────

    async fn rebuild_blocklist_filter(&self, blocklist_id: Uuid) -> Result<(), AppError>;
    async fn rebuild_profile(&self, profile_id: Uuid) -> Result<(), AppError>;
    async fn rebuild_device(&self, device_id: Uuid) -> Result<(), AppError>;
    async fn rebuild_default_context(&self) -> Result<(), AppError>;
    async fn handle_device_ip_changed(
        &self,
        device_id: Uuid,
        old_ip: &str,
        new_ip: &str,
    ) -> Result<(), AppError>;
}

/// Default implementation.
pub struct DnsFilterServiceImpl {
    repo: Arc<dyn DnsFilterRepository>,
    device_repo: Arc<dyn DeviceRepository>,
    events: Arc<dyn EventPublisher>,
    jobs: Arc<dyn JobService>,
    fetcher: Arc<dyn BlocklistFetcher>,

    /// Serialises blocklist imports. Enabling a profile dispatches one refresh
    /// job per blocklist at once, and a threat-intel feed is millions of
    /// domains — running them concurrently means holding several multi-million
    /// entry `Vec<String>` in memory and interleaving their write batches for
    /// no gain, since `SQLite` admits one writer anyway.
    refresh_lock: Arc<Semaphore>,

    /// Set once `rebuild_all` has compiled the filter cache. The DNS server
    /// binds and answers queries before this happens (the runner bootstraps in
    /// its own task), so until it flips, queries resolve through an empty
    /// filter set — fail-open. Surfaced to the admin rather than hidden.
    bootstrapped: Arc<AtomicBool>,

    /// Per-source compiled filters keyed by blocklist id (allowlist sources
    /// and custom-rules sources are inlined into one synthetic filter per
    /// profile keyed under a synthesised profile-scoped uuid; see
    /// [`Self::profile_filters_keys`]).
    blocklist_filters: Arc<RwLock<HashMap<Uuid, Arc<ArcSwap<DnsFilter>>>>>,
    /// One synthetic per-profile aggregator that holds a profile's allowlist
    /// + custom rules together (avoids one extra repo round-trip per source).
    profile_aux_filters: Arc<RwLock<HashMap<Uuid, Arc<ArcSwap<DnsFilter>>>>>,
    /// Compiled profiles keyed by id.
    profiles: Arc<RwLock<HashMap<Uuid, Arc<ArcSwap<RuntimeDnsFilterProfile>>>>>,
    /// Per-IP device contexts for fast hot-path lookup.
    contexts: Arc<RwLock<HashMap<IpAddr, Arc<DeviceFilterContext>>>>,
    /// The same contexts keyed by device id, for transports that
    /// authenticate the device out of band (`DoT` SNI, issue #912) and whose
    /// peer address the per-IP map may not cover. Rebuilt alongside
    /// `contexts` on every device/profile change.
    device_contexts: Arc<RwLock<HashMap<Uuid, Arc<DeviceFilterContext>>>>,
    /// Default context for unassigned devices and unknown IPs.
    default_context: Arc<ArcSwap<DeviceFilterContext>>,
    /// Global emergency stop + default-profile pointer.
    config: Arc<ArcSwap<DnsFilterConfig>>,
}

impl DnsFilterServiceImpl {
    pub fn new(
        repo: Arc<dyn DnsFilterRepository>,
        device_repo: Arc<dyn DeviceRepository>,
        events: Arc<dyn EventPublisher>,
        jobs: Arc<dyn JobService>,
        fetcher: Arc<dyn BlocklistFetcher>,
    ) -> Self {
        Self {
            repo,
            device_repo,
            events,
            jobs,
            fetcher,
            refresh_lock: Arc::new(Semaphore::new(1)),
            bootstrapped: Arc::new(AtomicBool::new(false)),
            blocklist_filters: Arc::new(RwLock::new(HashMap::new())),
            profile_aux_filters: Arc::new(RwLock::new(HashMap::new())),
            profiles: Arc::new(RwLock::new(HashMap::new())),
            contexts: Arc::new(RwLock::new(HashMap::new())),
            device_contexts: Arc::new(RwLock::new(HashMap::new())),
            default_context: Arc::new(ArcSwap::from_pointee(DeviceFilterContext::unfiltered())),
            config: Arc::new(ArcSwap::from_pointee(DnsFilterConfig::default())),
        }
    }

    fn publish(&self, change: DnsFilterChange) {
        self.events.publish(WardnetEvent::DnsFilterChanged {
            change,
            timestamp: Utc::now(),
        });
    }

    /// Announce that a compiled filter has just been swapped in. Emitted
    /// from each mutating `rebuild_*_inner` immediately after the swap,
    /// so `UdpDnsServer` (and any other cache holder) can invalidate
    /// answers that were forwarded under the previous filter (issue
    /// #341). Coarse by design — subscribers wanting fine-grained
    /// behaviour can listen to `DnsFilterChanged` instead.
    fn publish_rebuilt(&self) {
        self.events.publish(WardnetEvent::DnsFilterRebuilt {
            timestamp: Utc::now(),
        });
    }

    fn validate_domain(domain: &str) -> Result<(), AppError> {
        if domain.is_empty() {
            return Err(AppError::BadRequest("domain must not be empty".to_owned()));
        }
        if domain.len() > 253 {
            return Err(AppError::BadRequest(
                "domain must be <= 253 characters".to_owned(),
            ));
        }
        if !domain.contains('.') {
            return Err(AppError::BadRequest(
                "domain must contain at least one '.'".to_owned(),
            ));
        }
        if !domain
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
        {
            return Err(AppError::BadRequest(
                "domain contains invalid characters (allowed: alphanumeric, '.', '-', '_')"
                    .to_owned(),
            ));
        }
        Ok(())
    }

    fn validate_url(url: &str) -> Result<(), AppError> {
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(AppError::BadRequest(
                "URL must start with http:// or https://".to_owned(),
            ));
        }
        Ok(())
    }

    fn validate_cron(schedule: &str) -> Result<(), AppError> {
        crate::dns::cron_parse::parse_schedule(schedule)
            .map_err(|e| AppError::BadRequest(format!("Invalid cron expression: {e}")))?;
        Ok(())
    }

    fn validate_rule_text(rule_text: &str) -> Result<(), AppError> {
        match crate::dns::filter_parser::parse_line(rule_text) {
            Err(e) => Err(AppError::BadRequest(format!("Invalid filter rule: {e}"))),
            Ok(None) => Err(AppError::BadRequest(
                "Rule parses as a comment or blank line".to_owned(),
            )),
            Ok(Some(_)) => Ok(()),
        }
    }

    async fn ensure_profile_exists(&self, profile_id: Uuid) -> Result<DnsFilterProfile, AppError> {
        self.repo
            .get_profile(profile_id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound(format!("profile {profile_id} not found")))
    }

    async fn ensure_blocklist_in_profile(
        &self,
        profile_id: Uuid,
        id: Uuid,
    ) -> Result<wardnet_common::dns::Blocklist, AppError> {
        let bl = self
            .repo
            .get_blocklist(id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound(format!("blocklist {id} not found")))?;
        if bl.profile_id != profile_id {
            return Err(AppError::NotFound(format!(
                "blocklist {id} not found in profile {profile_id}"
            )));
        }
        Ok(bl)
    }

    async fn ensure_rule_in_profile(
        &self,
        profile_id: Uuid,
        id: Uuid,
    ) -> Result<wardnet_common::dns::CustomFilterRule, AppError> {
        let rule = self
            .repo
            .get_custom_rule(id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound(format!("rule {id} not found")))?;
        if rule.profile_id != profile_id {
            return Err(AppError::NotFound(format!(
                "rule {id} not found in profile {profile_id}"
            )));
        }
        Ok(rule)
    }

    /// Wrap a config in the response, stamping the fail-open state alongside it.
    ///
    /// Filtering is only actually in force once the cache has been compiled
    /// (`bootstrapped`) *and* every enabled blocklist has been imported at
    /// least once. A list that has never been downloaded is an empty list: it
    /// is "enabled" but blocks nothing, which is precisely the state the admin
    /// must not have to guess at.
    async fn config_response(
        &self,
        config: DnsFilterConfig,
    ) -> Result<DnsFilterConfigResponse, AppError> {
        let pending = self
            .repo
            .count_unimported_blocklists()
            .await
            .map_err(AppError::Internal)?;
        let bootstrapped = self.bootstrapped.load(Ordering::Acquire);

        Ok(DnsFilterConfigResponse {
            config,
            filters_ready: bootstrapped && pending == 0,
            pending_blocklists: u32::try_from(pending).unwrap_or(u32::MAX),
        })
    }

    /// Rebuild the per-blocklist filter from the repo. The aggregated
    /// per-profile filter does NOT need rebuilding — it indirectly
    /// references the same `Arc<ArcSwap<DnsFilter>>` we just swapped.
    async fn rebuild_blocklist_inner(&self, blocklist_id: Uuid) -> Result<(), AppError> {
        let bl = self
            .repo
            .get_blocklist(blocklist_id)
            .await
            .map_err(AppError::Internal)?;
        let Some(bl) = bl else {
            // Removed — drop the cache entry. We deliberately do NOT
            // publish `DnsFilterRebuilt` on this path: the response
            // cache only ever holds `Pass` answers (the Block/Rewrite
            // branches in the daemon's `QueryPipeline::handle` skip caching),
            // and removing a blocklist can only un-block, never
            // un-pass. Anything currently cached is still a valid
            // answer. If a future change starts caching Block answers
            // this invariant is gone and this path must publish.
            self.blocklist_filters.write().await.remove(&blocklist_id);
            return Ok(());
        };

        let inputs = if bl.enabled {
            let domains = self
                .repo
                .load_blocklist_domains(blocklist_id)
                .await
                .map_err(AppError::Internal)?;
            DnsFilterInputs {
                blocked_domains: domains,
                allowlist: Vec::new(),
                custom_rules: Vec::new(),
            }
        } else {
            DnsFilterInputs::default()
        };

        let new_filter = DnsFilter::build(inputs);
        let mut filters = self.blocklist_filters.write().await;
        if let Some(slot) = filters.get(&blocklist_id) {
            slot.store(Arc::new(new_filter));
        } else {
            filters.insert(blocklist_id, Arc::new(ArcSwap::from_pointee(new_filter)));
        }
        drop(filters);
        self.publish_rebuilt();
        Ok(())
    }

    /// Rebuild the per-profile aggregate (allowlist + custom rules) and
    /// re-stitch the `RuntimeDnsFilterProfile` from current blocklist arcs.
    async fn rebuild_profile_inner(&self, profile_id: Uuid) -> Result<(), AppError> {
        let Some(profile) = self
            .repo
            .get_profile(profile_id)
            .await
            .map_err(AppError::Internal)?
        else {
            // Profile gone — drop its in-memory entries. No
            // `DnsFilterRebuilt` for the same reason as the
            // blocklist-removed path above: cached answers are all
            // `Pass`, and removing a profile can only un-block, never
            // un-pass.
            self.profiles.write().await.remove(&profile_id);
            self.profile_aux_filters.write().await.remove(&profile_id);
            return Ok(());
        };

        let inputs = self
            .repo
            .load_filter_inputs_for_profile(profile_id)
            .await
            .map_err(AppError::Internal)?;

        // Blocked domains stay out of the aux filter: this profile's blocklists
        // are each compiled separately and composed in by reference below, so
        // folding their domains in here would double-store millions of entries.
        let aux_inputs = DnsFilterInputs {
            blocked_domains: Vec::new(),
            allowlist: inputs.allowlist,
            custom_rules: inputs.custom_rules,
        };
        let aux_filter = DnsFilter::build(aux_inputs);

        // Swap aux first so the new profile composition picks it up.
        let aux_arc = {
            let mut aux = self.profile_aux_filters.write().await;
            if let Some(slot) = aux.get(&profile_id) {
                slot.store(Arc::new(aux_filter));
                slot.clone()
            } else {
                let arc = Arc::new(ArcSwap::from_pointee(aux_filter));
                aux.insert(profile_id, arc.clone());
                arc
            }
        };

        // Collect this profile's blocklist filter arcs (build them on demand).
        let blocklists = self
            .repo
            .list_blocklists(profile_id)
            .await
            .map_err(AppError::Internal)?;
        let mut filters: Vec<Arc<ArcSwap<DnsFilter>>> = Vec::with_capacity(blocklists.len() + 1);
        for bl in &blocklists {
            if !bl.enabled {
                continue;
            }
            let entry = {
                let map = self.blocklist_filters.read().await;
                map.get(&bl.id).cloned()
            };
            if let Some(arc) = entry {
                filters.push(arc);
            } else {
                drop(self.rebuild_blocklist_inner(bl.id).await);
                if let Some(arc) = self.blocklist_filters.read().await.get(&bl.id).cloned() {
                    filters.push(arc);
                }
            }
        }
        filters.push(aux_arc);

        let runtime = RuntimeDnsFilterProfile::new(profile.id, profile.name, filters);
        let mut profiles = self.profiles.write().await;
        if let Some(slot) = profiles.get(&profile_id) {
            slot.store(Arc::new(runtime));
        } else {
            profiles.insert(profile_id, Arc::new(ArcSwap::from_pointee(runtime)));
        }
        drop(profiles);
        self.publish_rebuilt();
        Ok(())
    }

    async fn rebuild_device_inner(&self, device_id: Uuid) -> Result<(), AppError> {
        let settings = self
            .repo
            .find_device_settings(device_id)
            .await
            .map_err(AppError::Internal)?;
        let device = self
            .device_repo
            .find_by_id(&device_id.to_string())
            .await
            .map_err(AppError::Internal)?;

        let context = match settings {
            None => Arc::new(DeviceFilterContext::unfiltered()),
            Some(s) => Arc::new(self.materialise_context(&s).await),
        };

        // Drop any existing per-IP entries for this device, then insert under
        // the current ip (if any).
        let mut by_ip = self.contexts.write().await;
        let new_ip = device
            .as_ref()
            .and_then(|d| d.last_ip.parse::<IpAddr>().ok());

        if let Some(ip) = new_ip {
            by_ip.insert(ip, Arc::clone(&context));
        }
        drop(by_ip);
        self.device_contexts
            .write()
            .await
            .insert(device_id, context);
        self.publish_rebuilt();
        Ok(())
    }

    async fn materialise_context(&self, settings: &DeviceDnsFilterSettings) -> DeviceFilterContext {
        if !settings.enabled {
            return DeviceFilterContext {
                enabled: false,
                profiles: Vec::new(),
            };
        }

        let map = self.profiles.read().await;
        let profiles: Vec<_> = if settings.profile_ids.is_empty() {
            // Fall through to default — handled at hot-path lookup by
            // returning an empty `profiles` vec here. The hot path
            // substitutes the default-profile context.
            Vec::new()
        } else {
            settings
                .profile_ids
                .iter()
                .filter_map(|id| map.get(id).cloned())
                .collect()
        };

        DeviceFilterContext {
            enabled: true,
            profiles,
        }
    }

    async fn rebuild_default_context_inner(&self) -> Result<(), AppError> {
        let cfg = self.config.load_full();
        let new_ctx = if cfg.default_profile_ids.is_empty() {
            DeviceFilterContext::unfiltered()
        } else {
            let map = self.profiles.read().await;
            let profiles: Vec<_> = cfg
                .default_profile_ids
                .iter()
                .filter_map(|id| {
                    let resolved = map.get(id).cloned();
                    if resolved.is_none() {
                        // Survives FK CASCADE so should be unreachable in
                        // practice; surface it if a race ever lets it through.
                        tracing::warn!(profile_id = %id, "default profile not found in cache");
                    }
                    resolved
                })
                .collect();
            // Converge the empty-after-resolve case with the explicit-empty
            // case — "filtering on with zero rules" is observationally
            // identical to unfiltered but reports differently.
            if profiles.is_empty() {
                DeviceFilterContext::unfiltered()
            } else {
                DeviceFilterContext {
                    enabled: true,
                    profiles,
                }
            }
        };
        self.default_context.store(Arc::new(new_ctx));
        self.publish_rebuilt();
        Ok(())
    }

    async fn refresh_config(&self) -> Result<(), AppError> {
        let cfg = self
            .repo
            .get_dns_filter_config()
            .await
            .map_err(AppError::Internal)?;
        self.config.store(Arc::new(cfg));
        Ok(())
    }

    /// Shared tail of the hot-path checks: apply the resolved context (or
    /// the default one) and fold in the global / per-device kill switches.
    /// Computes would-have-blocked once — the kill-switch path is rare.
    fn outcome_with_context(
        &self,
        context: Option<Arc<DeviceFilterContext>>,
        domain: &str,
        qtype: RecordType,
        client: IpAddr,
    ) -> CheckOutcome {
        let cfg = self.config.load_full();
        let device_enabled = context.as_deref().is_none_or(|c| c.enabled);
        let global_enabled = cfg.enabled;

        // Build the effective context (explicit profiles or default).
        let effective_action = {
            let ctx_for_check: Arc<DeviceFilterContext> = match context {
                Some(c) if !c.profiles.is_empty() => c,
                _ => self.default_context.load_full(),
            };
            ctx_for_check.check(domain, qtype, client)
        };

        if !global_enabled || !device_enabled {
            return CheckOutcome {
                action: FilterAction::Pass,
                would_have_blocked: matches!(
                    effective_action,
                    FilterAction::Block | FilterAction::Rewrite { .. }
                ),
            };
        }

        CheckOutcome {
            action: effective_action,
            would_have_blocked: false,
        }
    }
}

#[async_trait]
impl DnsFilterService for DnsFilterServiceImpl {
    // NOTE (auth): `check` / `check_for_device` deliberately carry no
    // `require_admin`/`require_authenticated` guard. They are the DNS resolve
    // hot path, called per query from the DNS server (UDP `:53` and the DoT
    // `:853` listener) — not from an authenticated HTTP handler — and must
    // not allocate or take heavy locks. They expose no admin data and mutate
    // nothing; the filter *management* methods below are all guarded. This is
    // the same unguarded shape the pre-existing `check` has always had; the
    // guard-or-documented-exception rule in `.agents/auth.md` is satisfied by
    // this note rather than by fitting one of the three named categories.
    async fn check(&self, domain: &str, qtype: RecordType, client: IpAddr) -> CheckOutcome {
        let context = {
            let map = self.contexts.read().await;
            map.get(&client).cloned()
        };
        self.outcome_with_context(context, domain, qtype, client)
    }

    async fn check_for_device(
        &self,
        domain: &str,
        qtype: RecordType,
        device_id: Uuid,
        client: IpAddr,
    ) -> CheckOutcome {
        // Device-keyed context first; a device the map doesn't cover yet
        // falls back to the per-IP path (and from there to the default),
        // matching what the same query over UDP would resolve through.
        let context = {
            let by_device = self.device_contexts.read().await;
            by_device.get(&device_id).cloned()
        };
        let context = match context {
            Some(ctx) => Some(ctx),
            None => self.contexts.read().await.get(&client).cloned(),
        };
        self.outcome_with_context(context, domain, qtype, client)
    }

    async fn rebuild_all(&self) -> Result<(), AppError> {
        auth_context::require_admin()?;
        self.refresh_config().await?;

        // Wipe everything; rebuild from scratch.
        self.blocklist_filters.write().await.clear();
        self.profile_aux_filters.write().await.clear();
        self.profiles.write().await.clear();
        self.contexts.write().await.clear();
        self.device_contexts.write().await.clear();

        let profiles = self
            .repo
            .list_profiles()
            .await
            .map_err(AppError::Internal)?;
        for p in &profiles {
            // Build all blocklist filters first so the profile composition
            // can pick them up.
            let blocklists = self
                .repo
                .list_blocklists(p.id)
                .await
                .map_err(AppError::Internal)?;
            for bl in &blocklists {
                self.rebuild_blocklist_inner(bl.id).await?;
            }
            self.rebuild_profile_inner(p.id).await?;
        }

        // Per-device contexts.
        let device_settings = self
            .repo
            .list_device_settings_with_ips(false)
            .await
            .map_err(AppError::Internal)?;
        for entry in device_settings {
            let ctx = Arc::new(self.materialise_context(&entry.settings).await);
            if let Some(ip) = entry.ip.and_then(|s| s.parse::<IpAddr>().ok()) {
                self.contexts.write().await.insert(ip, Arc::clone(&ctx));
            }
            self.device_contexts
                .write()
                .await
                .insert(entry.settings.device_id, ctx);
        }

        self.rebuild_default_context_inner().await?;

        // The compiled cache is now live. Anything the DNS server answered
        // before this point went through an empty filter set.
        self.bootstrapped.store(true, Ordering::Release);
        Ok(())
    }

    async fn list_profiles(&self) -> Result<ListProfilesResponse, AppError> {
        auth_context::require_admin()?;
        let profiles = self
            .repo
            .list_profiles()
            .await
            .map_err(AppError::Internal)?;
        Ok(ListProfilesResponse { profiles })
    }

    async fn get_profile(&self, id: Uuid) -> Result<GetProfileResponse, AppError> {
        auth_context::require_admin()?;
        let profile = self.ensure_profile_exists(id).await?;
        Ok(GetProfileResponse { profile })
    }

    async fn create_profile(
        &self,
        req: CreateProfileRequest,
    ) -> Result<CreateProfileResponse, AppError> {
        auth_context::require_admin()?;
        if req.name.trim().is_empty() {
            return Err(AppError::BadRequest("name must not be empty".to_owned()));
        }
        if let Some(desc) = req.description.as_deref()
            && desc.len() > 200
        {
            return Err(AppError::BadRequest(
                "description must not exceed 200 characters".to_owned(),
            ));
        }
        let id = Uuid::new_v4();
        let profile = self
            .repo
            .create_profile(id, &req.name, req.description.as_deref())
            .await
            .map_err(AppError::Internal)?;
        self.publish(DnsFilterChange::ProfileMembership { profile_id: id });
        Ok(CreateProfileResponse {
            profile,
            message: "profile created".to_owned(),
        })
    }

    async fn update_profile(
        &self,
        id: Uuid,
        req: UpdateProfileRequest,
    ) -> Result<UpdateProfileResponse, AppError> {
        auth_context::require_admin()?;
        let existing = self.ensure_profile_exists(id).await?;
        if existing.builtin {
            return Err(AppError::Conflict(
                "builtin profiles cannot be modified".to_owned(),
            ));
        }
        if let Some(name) = req.name.as_deref()
            && name.trim().is_empty()
        {
            return Err(AppError::BadRequest("name must not be empty".to_owned()));
        }
        if let Some(Some(desc)) = req.description.as_ref()
            && desc.len() > 200
        {
            return Err(AppError::BadRequest(
                "description must not exceed 200 characters".to_owned(),
            ));
        }
        self.repo
            .update_profile_fields(
                id,
                req.name.as_deref(),
                req.description.as_ref().map(|d| d.as_deref()),
            )
            .await
            .map_err(AppError::Internal)?;
        let profile = self.ensure_profile_exists(id).await.unwrap_or(existing);
        self.publish(DnsFilterChange::ProfileMembership { profile_id: id });
        Ok(UpdateProfileResponse {
            profile,
            message: "profile updated".to_owned(),
        })
    }

    async fn delete_profile(&self, id: Uuid) -> Result<DeleteProfileResponse, AppError> {
        auth_context::require_admin()?;
        let existing = self.ensure_profile_exists(id).await?;
        if existing.builtin {
            return Err(AppError::Conflict(
                "builtin profiles cannot be deleted".to_owned(),
            ));
        }
        let deleted = self
            .repo
            .delete_profile(id)
            .await
            .map_err(AppError::Internal)?;
        if !deleted {
            return Err(AppError::NotFound(format!("profile {id} not found")));
        }
        self.publish(DnsFilterChange::ProfileMembership { profile_id: id });
        Ok(DeleteProfileResponse {
            message: format!("profile {id} deleted"),
        })
    }

    async fn list_blocklists(&self, profile_id: Uuid) -> Result<ListBlocklistsResponse, AppError> {
        auth_context::require_admin()?;
        self.ensure_profile_exists(profile_id).await?;
        let blocklists = self
            .repo
            .list_blocklists(profile_id)
            .await
            .map_err(AppError::Internal)?;
        Ok(ListBlocklistsResponse { blocklists })
    }

    async fn create_blocklist(
        &self,
        profile_id: Uuid,
        req: CreateBlocklistRequest,
    ) -> Result<CreateBlocklistResponse, AppError> {
        auth_context::require_admin()?;
        self.ensure_profile_exists(profile_id).await?;
        Self::validate_url(&req.url)?;
        Self::validate_cron(&req.cron_schedule)?;

        let id = Uuid::new_v4();
        let row = BlocklistRow {
            id: id.to_string(),
            profile_id: profile_id.to_string(),
            name: req.name,
            url: req.url,
            enabled: req.enabled,
            cron_schedule: req.cron_schedule,
        };
        self.repo
            .create_blocklist(&row)
            .await
            .map_err(AppError::Internal)?;
        let blocklist = self
            .repo
            .get_blocklist(id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| {
                AppError::Internal(anyhow::anyhow!("blocklist not found after insert"))
            })?;

        self.publish(DnsFilterChange::ProfileContent { profile_id });
        Ok(CreateBlocklistResponse {
            blocklist,
            message: "blocklist created".to_owned(),
        })
    }

    async fn update_blocklist(
        &self,
        profile_id: Uuid,
        id: Uuid,
        req: UpdateBlocklistRequest,
    ) -> Result<UpdateBlocklistResponse, AppError> {
        auth_context::require_admin()?;
        self.ensure_blocklist_in_profile(profile_id, id).await?;

        if let Some(ref url) = req.url {
            Self::validate_url(url)?;
        }
        if let Some(ref cron) = req.cron_schedule {
            Self::validate_cron(cron)?;
        }

        let update = BlocklistUpdate {
            name: req.name,
            url: req.url,
            enabled: req.enabled,
            cron_schedule: req.cron_schedule,
        };
        self.repo
            .update_blocklist(id, &update)
            .await
            .map_err(AppError::Internal)?;
        let blocklist = self
            .repo
            .get_blocklist(id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| {
                AppError::Internal(anyhow::anyhow!("blocklist not found after update"))
            })?;
        self.publish(DnsFilterChange::ProfileContent { profile_id });
        Ok(UpdateBlocklistResponse {
            blocklist,
            message: "blocklist updated".to_owned(),
        })
    }

    async fn delete_blocklist(
        &self,
        profile_id: Uuid,
        id: Uuid,
    ) -> Result<DeleteBlocklistResponse, AppError> {
        auth_context::require_admin()?;
        self.ensure_blocklist_in_profile(profile_id, id).await?;
        let deleted = self
            .repo
            .delete_blocklist(id)
            .await
            .map_err(AppError::Internal)?;
        if !deleted {
            return Err(AppError::NotFound(format!("blocklist {id} not found")));
        }
        self.publish(DnsFilterChange::ProfileContent { profile_id });
        Ok(DeleteBlocklistResponse {
            message: format!("blocklist {id} deleted"),
        })
    }

    async fn refresh_blocklist(
        &self,
        profile_id: Uuid,
        id: Uuid,
    ) -> Result<JobDispatchedResponse, AppError> {
        auth_context::require_admin()?;
        let bl = self.ensure_blocklist_in_profile(profile_id, id).await?;

        let repo = self.repo.clone();
        let fetcher = self.fetcher.clone();
        let events = self.events.clone();
        let refresh_lock = self.refresh_lock.clone();

        let job_id = self
            .jobs
            .dispatch(JobKind::BlocklistRefresh, move |reporter| async move {
                // Enabling a profile dispatches one job per blocklist, so all
                // but the first wait here — for minutes, if a multi-million
                // domain feed is ahead of them. Publish progress *before*
                // blocking: otherwise a queued job sits at a flat 0% for the
                // whole wait, which reads to the user as the same frozen bar
                // this whole change exists to eliminate.
                reporter.set_progress(QUEUED_PROGRESS).await;
                let _permit = refresh_lock.acquire().await?;
                blocklist_downloader::refresh_blocklist(
                    &bl,
                    repo.as_ref(),
                    fetcher.as_ref(),
                    events.as_ref(),
                    Some(&reporter),
                )
                .await
                .map(|_| ())
            })
            .await;
        Ok(JobDispatchedResponse { job_id })
    }

    async fn list_allowlist(&self, profile_id: Uuid) -> Result<ListAllowlistResponse, AppError> {
        auth_context::require_admin()?;
        self.ensure_profile_exists(profile_id).await?;
        let entries = self
            .repo
            .list_allowlist(profile_id)
            .await
            .map_err(AppError::Internal)?;
        Ok(ListAllowlistResponse { entries })
    }

    async fn create_allowlist_entry(
        &self,
        profile_id: Uuid,
        req: CreateAllowlistRequest,
    ) -> Result<CreateAllowlistResponse, AppError> {
        auth_context::require_admin()?;
        self.ensure_profile_exists(profile_id).await?;
        Self::validate_domain(&req.domain)?;

        let id = Uuid::new_v4();
        let row = AllowlistRow {
            id: id.to_string(),
            profile_id: profile_id.to_string(),
            domain: req.domain,
            reason: req.reason,
        };
        self.repo
            .create_allowlist_entry(&row)
            .await
            .map_err(AppError::Internal)?;
        let entries = self
            .repo
            .list_allowlist(profile_id)
            .await
            .map_err(AppError::Internal)?;
        let entry = entries.into_iter().find(|e| e.id == id).ok_or_else(|| {
            AppError::Internal(anyhow::anyhow!("allowlist entry not found after insert"))
        })?;
        self.publish(DnsFilterChange::ProfileContent { profile_id });
        Ok(CreateAllowlistResponse {
            entry,
            message: "allowlist entry created".to_owned(),
        })
    }

    async fn delete_allowlist_entry(
        &self,
        profile_id: Uuid,
        id: Uuid,
    ) -> Result<DeleteAllowlistResponse, AppError> {
        auth_context::require_admin()?;
        self.ensure_profile_exists(profile_id).await?;
        let deleted = self
            .repo
            .delete_allowlist_entry(id)
            .await
            .map_err(AppError::Internal)?;
        if !deleted {
            return Err(AppError::NotFound(format!(
                "allowlist entry {id} not found"
            )));
        }
        self.publish(DnsFilterChange::ProfileContent { profile_id });
        Ok(DeleteAllowlistResponse {
            message: format!("allowlist entry {id} deleted"),
        })
    }

    async fn list_custom_rules(
        &self,
        profile_id: Uuid,
    ) -> Result<ListFilterRulesResponse, AppError> {
        auth_context::require_admin()?;
        self.ensure_profile_exists(profile_id).await?;
        let rules = self
            .repo
            .list_custom_rules(profile_id)
            .await
            .map_err(AppError::Internal)?;
        Ok(ListFilterRulesResponse { rules })
    }

    async fn create_custom_rule(
        &self,
        profile_id: Uuid,
        req: CreateFilterRuleRequest,
    ) -> Result<CreateFilterRuleResponse, AppError> {
        auth_context::require_admin()?;
        self.ensure_profile_exists(profile_id).await?;
        Self::validate_rule_text(&req.rule_text)?;

        let id = Uuid::new_v4();
        let row = CustomRuleRow {
            id: id.to_string(),
            profile_id: profile_id.to_string(),
            rule_text: req.rule_text,
            enabled: req.enabled,
            comment: req.comment,
        };
        self.repo
            .create_custom_rule(&row)
            .await
            .map_err(AppError::Internal)?;
        let rule = self
            .repo
            .get_custom_rule(id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| {
                AppError::Internal(anyhow::anyhow!("custom rule not found after insert"))
            })?;
        self.publish(DnsFilterChange::ProfileContent { profile_id });
        Ok(CreateFilterRuleResponse {
            rule,
            message: "filter rule created".to_owned(),
        })
    }

    async fn update_custom_rule(
        &self,
        profile_id: Uuid,
        id: Uuid,
        req: UpdateFilterRuleRequest,
    ) -> Result<UpdateFilterRuleResponse, AppError> {
        auth_context::require_admin()?;
        self.ensure_rule_in_profile(profile_id, id).await?;
        if let Some(ref rule_text) = req.rule_text {
            Self::validate_rule_text(rule_text)?;
        }
        let update = CustomRuleUpdate {
            rule_text: req.rule_text,
            enabled: req.enabled,
            comment: req.comment,
        };
        self.repo
            .update_custom_rule(id, &update)
            .await
            .map_err(AppError::Internal)?;
        let rule = self
            .repo
            .get_custom_rule(id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| {
                AppError::Internal(anyhow::anyhow!("custom rule not found after update"))
            })?;
        self.publish(DnsFilterChange::ProfileContent { profile_id });
        Ok(UpdateFilterRuleResponse {
            rule,
            message: "filter rule updated".to_owned(),
        })
    }

    async fn delete_custom_rule(
        &self,
        profile_id: Uuid,
        id: Uuid,
    ) -> Result<DeleteFilterRuleResponse, AppError> {
        auth_context::require_admin()?;
        self.ensure_rule_in_profile(profile_id, id).await?;
        let deleted = self
            .repo
            .delete_custom_rule(id)
            .await
            .map_err(AppError::Internal)?;
        if !deleted {
            return Err(AppError::NotFound(format!("rule {id} not found")));
        }
        self.publish(DnsFilterChange::ProfileContent { profile_id });
        Ok(DeleteFilterRuleResponse {
            message: format!("filter rule {id} deleted"),
        })
    }

    async fn list_device_settings(
        &self,
        params: ListDeviceFilterSettingsParams,
    ) -> Result<ListDeviceFilterSettingsResponse, AppError> {
        auth_context::require_admin()?;
        let only_active = matches!(params.enabled, Some(true));
        let entries = self
            .repo
            .list_device_settings_with_ips(only_active)
            .await
            .map_err(AppError::Internal)?;
        let devices = entries
            .into_iter()
            .filter(|e| match params.enabled {
                Some(want) => e.settings.enabled == want,
                None => true,
            })
            .map(|e| e.settings)
            .collect();
        Ok(ListDeviceFilterSettingsResponse { devices })
    }

    async fn get_device_settings(
        &self,
        device_id: Uuid,
    ) -> Result<GetDeviceFilterSettingsResponse, AppError> {
        auth_context::require_admin()?;
        let settings = self
            .repo
            .find_device_settings(device_id)
            .await
            .map_err(AppError::Internal)?
            .unwrap_or_else(|| DeviceDnsFilterSettings::default_for(device_id));
        Ok(GetDeviceFilterSettingsResponse { settings })
    }

    async fn update_device_settings(
        &self,
        device_id: Uuid,
        req: UpdateDeviceFilterSettingsRequest,
    ) -> Result<UpdateDeviceFilterSettingsResponse, AppError> {
        auth_context::require_admin()?;
        for pid in &req.profile_ids {
            self.ensure_profile_exists(*pid).await?;
        }

        self.repo
            .upsert_device_settings(&DeviceSettingsRow {
                device_id: device_id.to_string(),
                enabled: req.enabled,
            })
            .await
            .map_err(AppError::Internal)?;
        self.repo
            .set_device_profiles(device_id, &req.profile_ids)
            .await
            .map_err(AppError::Internal)?;

        let settings = self
            .repo
            .find_device_settings(device_id)
            .await
            .map_err(AppError::Internal)?
            .unwrap_or_else(|| DeviceDnsFilterSettings::default_for(device_id));

        self.publish(DnsFilterChange::DeviceAssignment { device_id });
        Ok(UpdateDeviceFilterSettingsResponse {
            settings,
            message: "device filter settings updated".to_owned(),
        })
    }

    async fn get_filter_config(&self) -> Result<DnsFilterConfigResponse, AppError> {
        auth_context::require_admin()?;
        let config = self
            .repo
            .get_dns_filter_config()
            .await
            .map_err(AppError::Internal)?;
        self.config_response(config).await
    }

    async fn update_filter_config(
        &self,
        req: UpdateDnsFilterConfigRequest,
    ) -> Result<DnsFilterConfigResponse, AppError> {
        auth_context::require_admin()?;
        let mut current = self
            .repo
            .get_dns_filter_config()
            .await
            .map_err(AppError::Internal)?;
        let mut changed_default = false;
        let mut changed_global = false;
        if let Some(enabled) = req.enabled {
            changed_global = current.enabled != enabled;
            current.enabled = enabled;
        }
        if let Some(mut ids) = req.default_profile_ids {
            // Storage has profile_id as PK — duplicates would abort the
            // replace-set tx. Sort+dedup also doubles as the canonical
            // form for the change-detection compare below.
            ids.sort();
            ids.dedup();
            for pid in &ids {
                self.ensure_profile_exists(*pid).await?;
            }
            let mut current_sorted = current.default_profile_ids.clone();
            current_sorted.sort();
            changed_default = current_sorted != ids;
            current.default_profile_ids = ids;
        }
        self.repo
            .set_dns_filter_config(&current)
            .await
            .map_err(AppError::Internal)?;
        // Refresh the in-memory Arc immediately so synchronous readers
        // (e.g. the kill-switch short-circuit in `check()`) don't run
        // against the stale config until the runner picks up the event.
        self.config.store(Arc::new(current.clone()));
        if changed_default {
            self.publish(DnsFilterChange::DefaultProfile);
        }
        if changed_global {
            self.publish(DnsFilterChange::GlobalToggle);
        }
        self.config_response(current).await
    }

    async fn rebuild_blocklist_filter(&self, blocklist_id: Uuid) -> Result<(), AppError> {
        auth_context::require_admin()?;
        self.rebuild_blocklist_inner(blocklist_id).await?;
        // After a blocklist swap, the profile's filter list still references
        // the same Arc, so no profile rebuild needed; but the source filter
        // may have flipped enabled/disabled, so re-stitch is cheaper than
        // tracking that.
        if let Some(bl) = self
            .repo
            .get_blocklist(blocklist_id)
            .await
            .map_err(AppError::Internal)?
        {
            self.rebuild_profile_inner(bl.profile_id).await?;
        }
        Ok(())
    }

    async fn rebuild_profile(&self, profile_id: Uuid) -> Result<(), AppError> {
        auth_context::require_admin()?;
        // Re-fetch this profile's blocklists to make sure all per-source
        // filters exist before we re-stitch.
        let blocklists = self
            .repo
            .list_blocklists(profile_id)
            .await
            .map_err(AppError::Internal)?;
        for bl in &blocklists {
            self.rebuild_blocklist_inner(bl.id).await?;
        }
        self.rebuild_profile_inner(profile_id).await?;

        // Default-context might point at this profile; refresh it.
        let cfg = self.config.load_full();
        if cfg.default_profile_ids.contains(&profile_id) {
            self.rebuild_default_context_inner().await?;
        }

        // Any device with this profile in its assignments should be rebuilt.
        let device_settings = self
            .repo
            .list_device_settings_with_ips(false)
            .await
            .map_err(AppError::Internal)?;
        for entry in device_settings {
            if entry.settings.profile_ids.contains(&profile_id) {
                self.rebuild_device_inner(entry.settings.device_id).await?;
            }
        }
        Ok(())
    }

    async fn rebuild_device(&self, device_id: Uuid) -> Result<(), AppError> {
        auth_context::require_admin()?;
        self.rebuild_device_inner(device_id).await
    }

    async fn rebuild_default_context(&self) -> Result<(), AppError> {
        auth_context::require_admin()?;
        self.refresh_config().await?;
        self.rebuild_default_context_inner().await
    }

    async fn handle_device_ip_changed(
        &self,
        device_id: Uuid,
        old_ip: &str,
        new_ip: &str,
    ) -> Result<(), AppError> {
        auth_context::require_admin()?;
        let mut by_ip = self.contexts.write().await;
        let entry = old_ip
            .parse::<IpAddr>()
            .ok()
            .and_then(|ip| by_ip.remove(&ip));
        if let (Ok(new), Some(ctx)) = (new_ip.parse::<IpAddr>(), entry) {
            by_ip.insert(new, ctx);
        } else {
            drop(by_ip);
            // Fall back to a full per-device rebuild.
            self.rebuild_device_inner(device_id).await?;
        }
        Ok(())
    }
}

/// Helper used by the wiring code to know which profile the migration seeded
/// as the initial default.
#[must_use]
pub fn builtin_ad_blocking_profile_id() -> Uuid {
    BUILTIN_AD_BLOCKING
        .parse()
        .expect("hardcoded uuid is valid")
}
