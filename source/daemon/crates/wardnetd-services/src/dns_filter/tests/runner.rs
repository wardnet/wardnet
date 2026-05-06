//! Tests for [`DnsFilterRunner`].
//!
//! The runner subscribes to the event bus and dispatches each event to the
//! right rebuild method on `DnsFilterService`. We verify that contract by
//! using a recording mock service: publish an event, give the runner a
//! tick to consume it, then assert which method got called.

use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use hickory_proto::rr::RecordType;
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
use wardnet_common::dns::{AllowlistEntry, Blocklist, CustomFilterRule, FilterAction};
use wardnet_common::dns_filter::{DeviceDnsFilterSettings, DnsFilterConfig, DnsFilterProfile};
use wardnet_common::event::{DnsFilterChange, WardnetEvent};
use wardnet_common::jobs::JobDispatchedResponse;
use wardnetd_data::repository::{
    AllowlistRow, BlocklistRow, BlocklistUpdate, CustomRuleRow, CustomRuleUpdate,
    DeviceSettingsRow, DeviceSettingsWithIp, DnsFilterRepository, ProfileFilterInputs,
};

use crate::dns_filter::blocklist_downloader::BlocklistFetcher;
use crate::dns_filter::runner::DnsFilterRunner;
use crate::dns_filter::service::{CheckOutcome, DnsFilterService};
use crate::error::AppError;
use crate::event::{BroadcastEventBus, EventPublisher};

// ---------------------------------------------------------------------------
// RecordingService — captures which rebuild method the runner invoked.
// ---------------------------------------------------------------------------

#[derive(Default, Clone)]
struct Calls {
    rebuild_all: u32,
    rebuild_blocklist: Vec<Uuid>,
    rebuild_profile: Vec<Uuid>,
    rebuild_device: Vec<Uuid>,
    rebuild_default: u32,
    handle_ip_changed: Vec<(Uuid, String, String)>,
}

#[derive(Default)]
struct RecordingService {
    calls: StdMutex<Calls>,
}

impl RecordingService {
    fn snapshot(&self) -> Calls {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl DnsFilterService for RecordingService {
    async fn check(
        &self,
        _domain: &str,
        _qtype: RecordType,
        _client: std::net::IpAddr,
    ) -> CheckOutcome {
        CheckOutcome {
            action: FilterAction::Pass,
            would_have_blocked: false,
        }
    }
    async fn rebuild_all(&self) -> Result<(), AppError> {
        self.calls.lock().unwrap().rebuild_all += 1;
        Ok(())
    }
    async fn rebuild_blocklist_filter(&self, id: Uuid) -> Result<(), AppError> {
        self.calls.lock().unwrap().rebuild_blocklist.push(id);
        Ok(())
    }
    async fn rebuild_profile(&self, id: Uuid) -> Result<(), AppError> {
        self.calls.lock().unwrap().rebuild_profile.push(id);
        Ok(())
    }
    async fn rebuild_device(&self, id: Uuid) -> Result<(), AppError> {
        self.calls.lock().unwrap().rebuild_device.push(id);
        Ok(())
    }
    async fn rebuild_default_context(&self) -> Result<(), AppError> {
        self.calls.lock().unwrap().rebuild_default += 1;
        Ok(())
    }
    async fn handle_device_ip_changed(
        &self,
        device_id: Uuid,
        old_ip: &str,
        new_ip: &str,
    ) -> Result<(), AppError> {
        self.calls.lock().unwrap().handle_ip_changed.push((
            device_id,
            old_ip.to_owned(),
            new_ip.to_owned(),
        ));
        Ok(())
    }

    // CRUD methods are not exercised by the runner; panic if reached so a
    // future change that starts using one shows up loudly.
    async fn list_profiles(&self) -> Result<ListProfilesResponse, AppError> {
        unimplemented!()
    }
    async fn get_profile(&self, _id: Uuid) -> Result<GetProfileResponse, AppError> {
        unimplemented!()
    }
    async fn create_profile(
        &self,
        _r: CreateProfileRequest,
    ) -> Result<CreateProfileResponse, AppError> {
        unimplemented!()
    }
    async fn update_profile(
        &self,
        _id: Uuid,
        _r: UpdateProfileRequest,
    ) -> Result<UpdateProfileResponse, AppError> {
        unimplemented!()
    }
    async fn delete_profile(&self, _id: Uuid) -> Result<DeleteProfileResponse, AppError> {
        unimplemented!()
    }
    async fn list_blocklists(&self, _profile_id: Uuid) -> Result<ListBlocklistsResponse, AppError> {
        unimplemented!()
    }
    async fn create_blocklist(
        &self,
        _profile_id: Uuid,
        _r: CreateBlocklistRequest,
    ) -> Result<CreateBlocklistResponse, AppError> {
        unimplemented!()
    }
    async fn update_blocklist(
        &self,
        _profile_id: Uuid,
        _id: Uuid,
        _r: UpdateBlocklistRequest,
    ) -> Result<UpdateBlocklistResponse, AppError> {
        unimplemented!()
    }
    async fn delete_blocklist(
        &self,
        _profile_id: Uuid,
        _id: Uuid,
    ) -> Result<DeleteBlocklistResponse, AppError> {
        unimplemented!()
    }
    async fn refresh_blocklist(
        &self,
        _profile_id: Uuid,
        _id: Uuid,
    ) -> Result<JobDispatchedResponse, AppError> {
        unimplemented!()
    }
    async fn list_allowlist(&self, _profile_id: Uuid) -> Result<ListAllowlistResponse, AppError> {
        unimplemented!()
    }
    async fn create_allowlist_entry(
        &self,
        _profile_id: Uuid,
        _r: CreateAllowlistRequest,
    ) -> Result<CreateAllowlistResponse, AppError> {
        unimplemented!()
    }
    async fn delete_allowlist_entry(
        &self,
        _profile_id: Uuid,
        _id: Uuid,
    ) -> Result<DeleteAllowlistResponse, AppError> {
        unimplemented!()
    }
    async fn list_custom_rules(
        &self,
        _profile_id: Uuid,
    ) -> Result<ListFilterRulesResponse, AppError> {
        unimplemented!()
    }
    async fn create_custom_rule(
        &self,
        _profile_id: Uuid,
        _r: CreateFilterRuleRequest,
    ) -> Result<CreateFilterRuleResponse, AppError> {
        unimplemented!()
    }
    async fn update_custom_rule(
        &self,
        _profile_id: Uuid,
        _id: Uuid,
        _r: UpdateFilterRuleRequest,
    ) -> Result<UpdateFilterRuleResponse, AppError> {
        unimplemented!()
    }
    async fn delete_custom_rule(
        &self,
        _profile_id: Uuid,
        _id: Uuid,
    ) -> Result<DeleteFilterRuleResponse, AppError> {
        unimplemented!()
    }
    async fn list_device_settings(
        &self,
        _params: ListDeviceFilterSettingsParams,
    ) -> Result<ListDeviceFilterSettingsResponse, AppError> {
        unimplemented!()
    }
    async fn get_device_settings(
        &self,
        _device_id: Uuid,
    ) -> Result<GetDeviceFilterSettingsResponse, AppError> {
        unimplemented!()
    }
    async fn update_device_settings(
        &self,
        _device_id: Uuid,
        _r: UpdateDeviceFilterSettingsRequest,
    ) -> Result<UpdateDeviceFilterSettingsResponse, AppError> {
        unimplemented!()
    }
    async fn get_filter_config(&self) -> Result<DnsFilterConfigResponse, AppError> {
        unimplemented!()
    }
    async fn update_filter_config(
        &self,
        _r: UpdateDnsFilterConfigRequest,
    ) -> Result<DnsFilterConfigResponse, AppError> {
        unimplemented!()
    }
}

// ---------------------------------------------------------------------------
// Empty repo + fetcher — the runner's bootstrap and cron-tick paths call
// these but the tests don't assert on their content.
// ---------------------------------------------------------------------------

struct EmptyRepo;

#[async_trait]
impl DnsFilterRepository for EmptyRepo {
    async fn list_profiles(&self) -> anyhow::Result<Vec<DnsFilterProfile>> {
        Ok(vec![])
    }
    async fn get_profile(&self, _id: Uuid) -> anyhow::Result<Option<DnsFilterProfile>> {
        Ok(None)
    }
    async fn create_profile(&self, _id: Uuid, _name: &str) -> anyhow::Result<DnsFilterProfile> {
        unimplemented!()
    }
    async fn rename_profile(&self, _id: Uuid, _name: &str) -> anyhow::Result<bool> {
        unimplemented!()
    }
    async fn delete_profile(&self, _id: Uuid) -> anyhow::Result<bool> {
        unimplemented!()
    }
    async fn list_blocklists(&self, _profile_id: Uuid) -> anyhow::Result<Vec<Blocklist>> {
        Ok(vec![])
    }
    async fn get_blocklist(&self, _id: Uuid) -> anyhow::Result<Option<Blocklist>> {
        Ok(None)
    }
    async fn create_blocklist(&self, _row: &BlocklistRow) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn update_blocklist(&self, _id: Uuid, _row: &BlocklistUpdate) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn delete_blocklist(&self, _id: Uuid) -> anyhow::Result<bool> {
        unimplemented!()
    }
    async fn replace_blocklist_domains(
        &self,
        _id: Uuid,
        _domains: &[String],
    ) -> anyhow::Result<u64> {
        unimplemented!()
    }
    async fn set_blocklist_error(&self, _id: Uuid, _error: Option<&str>) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn list_allowlist(&self, _profile_id: Uuid) -> anyhow::Result<Vec<AllowlistEntry>> {
        unimplemented!()
    }
    async fn create_allowlist_entry(&self, _row: &AllowlistRow) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn delete_allowlist_entry(&self, _id: Uuid) -> anyhow::Result<bool> {
        unimplemented!()
    }
    async fn list_custom_rules(&self, _profile_id: Uuid) -> anyhow::Result<Vec<CustomFilterRule>> {
        unimplemented!()
    }
    async fn get_custom_rule(&self, _id: Uuid) -> anyhow::Result<Option<CustomFilterRule>> {
        unimplemented!()
    }
    async fn create_custom_rule(&self, _row: &CustomRuleRow) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn update_custom_rule(&self, _id: Uuid, _row: &CustomRuleUpdate) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn delete_custom_rule(&self, _id: Uuid) -> anyhow::Result<bool> {
        unimplemented!()
    }
    async fn load_filter_inputs_for_profile(
        &self,
        _profile_id: Uuid,
    ) -> anyhow::Result<ProfileFilterInputs> {
        unimplemented!()
    }
    async fn load_blocklist_domains(&self, _blocklist_id: Uuid) -> anyhow::Result<Vec<String>> {
        unimplemented!()
    }
    async fn find_device_settings(
        &self,
        _device_id: Uuid,
    ) -> anyhow::Result<Option<DeviceDnsFilterSettings>> {
        unimplemented!()
    }
    async fn upsert_device_settings(&self, _row: &DeviceSettingsRow) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn set_device_profiles(
        &self,
        _device_id: Uuid,
        _profile_ids: &[Uuid],
    ) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn list_device_settings_with_ips(
        &self,
        _only_filtering_active: bool,
    ) -> anyhow::Result<Vec<DeviceSettingsWithIp>> {
        Ok(vec![])
    }
    async fn get_dns_filter_config(&self) -> anyhow::Result<DnsFilterConfig> {
        Ok(DnsFilterConfig::default())
    }
    async fn set_dns_filter_config(&self, _config: &DnsFilterConfig) -> anyhow::Result<()> {
        unimplemented!()
    }
}

struct StubFetcher;
#[async_trait]
impl BlocklistFetcher for StubFetcher {
    async fn fetch(&self, _url: &str) -> anyhow::Result<String> {
        Ok(String::new())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Wait for `predicate` to become true, polling every ~5 ms. Bounds the
/// flaky case where the runner hasn't yet picked up an event the publisher
/// just shoved into the broadcast channel.
async fn wait_until<F: Fn() -> bool>(deadline_ms: u64, predicate: F) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_millis(deadline_ms);
    while std::time::Instant::now() < deadline {
        if predicate() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    predicate()
}

fn build() -> (
    Arc<RecordingService>,
    Arc<dyn EventPublisher>,
    DnsFilterRunner,
) {
    let service = Arc::new(RecordingService::default());
    let svc_dyn: Arc<dyn DnsFilterService> = service.clone();
    let repo: Arc<dyn DnsFilterRepository> = Arc::new(EmptyRepo);
    let fetcher: Arc<dyn BlocklistFetcher> = Arc::new(StubFetcher);
    let events: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(64));
    // Cron interval long enough that no tick fires during the test —
    // we're only exercising the event-driven branches.
    let runner = DnsFilterRunner::start(
        svc_dyn,
        repo,
        fetcher,
        events.clone(),
        &tracing::Span::none(),
        Duration::from_hours(1),
    );
    (service, events, runner)
}

#[tokio::test]
async fn bootstrap_calls_rebuild_all() {
    let (service, _events, runner) = build();
    let hit = wait_until(500, || service.snapshot().rebuild_all >= 1).await;
    assert!(hit, "runner should bootstrap by calling rebuild_all once");
    runner.shutdown().await;
}

#[tokio::test]
async fn dns_filter_blocklist_updated_rebuilds_one_filter() {
    let (service, events, runner) = build();
    let _ = wait_until(500, || service.snapshot().rebuild_all >= 1).await;

    let blocklist_id = Uuid::new_v4();
    events.publish(WardnetEvent::DnsFilterBlocklistUpdated {
        blocklist_id,
        entry_count: 5,
        timestamp: Utc::now(),
    });

    let hit = wait_until(500, || {
        service.snapshot().rebuild_blocklist.contains(&blocklist_id)
    })
    .await;
    assert!(hit, "expected rebuild_blocklist_filter for {blocklist_id}");
    runner.shutdown().await;
}

#[tokio::test]
async fn profile_content_change_rebuilds_profile() {
    let (service, events, runner) = build();
    let _ = wait_until(500, || service.snapshot().rebuild_all >= 1).await;

    let profile_id = Uuid::new_v4();
    events.publish(WardnetEvent::DnsFilterChanged {
        change: DnsFilterChange::ProfileContent { profile_id },
        timestamp: Utc::now(),
    });

    let hit = wait_until(500, || {
        service.snapshot().rebuild_profile.contains(&profile_id)
    })
    .await;
    assert!(hit, "expected rebuild_profile for {profile_id}");
    runner.shutdown().await;
}

#[tokio::test]
async fn profile_membership_change_rebuilds_profile() {
    let (service, events, runner) = build();
    let _ = wait_until(500, || service.snapshot().rebuild_all >= 1).await;

    let profile_id = Uuid::new_v4();
    events.publish(WardnetEvent::DnsFilterChanged {
        change: DnsFilterChange::ProfileMembership { profile_id },
        timestamp: Utc::now(),
    });

    let hit = wait_until(500, || {
        service.snapshot().rebuild_profile.contains(&profile_id)
    })
    .await;
    assert!(hit);
    runner.shutdown().await;
}

#[tokio::test]
async fn device_assignment_change_rebuilds_device() {
    let (service, events, runner) = build();
    let _ = wait_until(500, || service.snapshot().rebuild_all >= 1).await;

    let device_id = Uuid::new_v4();
    events.publish(WardnetEvent::DnsFilterChanged {
        change: DnsFilterChange::DeviceAssignment { device_id },
        timestamp: Utc::now(),
    });

    let hit = wait_until(500, || {
        service.snapshot().rebuild_device.contains(&device_id)
    })
    .await;
    assert!(hit);
    runner.shutdown().await;
}

#[tokio::test]
async fn default_profile_change_rebuilds_default_context() {
    let (service, events, runner) = build();
    let _ = wait_until(500, || service.snapshot().rebuild_all >= 1).await;

    events.publish(WardnetEvent::DnsFilterChanged {
        change: DnsFilterChange::DefaultProfile,
        timestamp: Utc::now(),
    });

    let hit = wait_until(500, || service.snapshot().rebuild_default >= 1).await;
    assert!(hit);
    runner.shutdown().await;
}

#[tokio::test]
async fn global_toggle_change_rebuilds_default_context() {
    let (service, events, runner) = build();
    let _ = wait_until(500, || service.snapshot().rebuild_all >= 1).await;

    events.publish(WardnetEvent::DnsFilterChanged {
        change: DnsFilterChange::GlobalToggle,
        timestamp: Utc::now(),
    });

    let hit = wait_until(500, || service.snapshot().rebuild_default >= 1).await;
    assert!(hit);
    runner.shutdown().await;
}

#[tokio::test]
async fn device_ip_changed_propagates_to_service() {
    let (service, events, runner) = build();
    let _ = wait_until(500, || service.snapshot().rebuild_all >= 1).await;

    let device_id = Uuid::new_v4();
    events.publish(WardnetEvent::DeviceIpChanged {
        device_id,
        mac: "aa:bb:cc:dd:ee:ff".into(),
        old_ip: "10.0.0.1".into(),
        new_ip: "10.0.0.2".into(),
        timestamp: Utc::now(),
    });

    let hit = wait_until(500, || {
        service
            .snapshot()
            .handle_ip_changed
            .iter()
            .any(|(id, old, new)| *id == device_id && old == "10.0.0.1" && new == "10.0.0.2")
    })
    .await;
    assert!(hit, "expected handle_device_ip_changed call");
    runner.shutdown().await;
}

#[tokio::test]
async fn unrelated_events_are_ignored() {
    let (service, events, runner) = build();
    let _ = wait_until(500, || service.snapshot().rebuild_all >= 1).await;
    let baseline = service.snapshot();

    events.publish(WardnetEvent::DnsConfigChanged {
        timestamp: Utc::now(),
    });

    // Give the runner a moment to consume.
    tokio::time::sleep(Duration::from_millis(50)).await;

    let after = service.snapshot();
    assert_eq!(
        baseline.rebuild_blocklist.len(),
        after.rebuild_blocklist.len()
    );
    assert_eq!(baseline.rebuild_profile.len(), after.rebuild_profile.len());
    assert_eq!(baseline.rebuild_device.len(), after.rebuild_device.len());
    assert_eq!(baseline.rebuild_default, after.rebuild_default);
    assert_eq!(
        baseline.handle_ip_changed.len(),
        after.handle_ip_changed.len()
    );
    runner.shutdown().await;
}
