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
use wardnet_common::dns::{Blocklist, FilterAction};
use wardnet_common::dns_filter::DnsFilterProfile;
use wardnet_common::event::{DnsFilterChange, WardnetEvent};
use wardnet_common::jobs::JobDispatchedResponse;

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
    /// `(profile_id, blocklist_id)` pairs the cron path dispatched a
    /// refresh for.
    refresh_blocklist: Vec<(Uuid, Uuid)>,
}

#[derive(Default)]
struct RecordingService {
    calls: StdMutex<Calls>,
    /// Profiles returned by `list_profiles` — drives the cron path. Empty by
    /// default so event-driven tests never trip the cron branch.
    cron_profiles: Vec<DnsFilterProfile>,
    /// Blocklists returned by `list_blocklists` for any profile.
    cron_blocklists: Vec<Blocklist>,
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

    // `list_profiles` / `list_blocklists` / `refresh_blocklist` drive the cron
    // path; the rest are not exercised by the runner and panic if reached so a
    // future change that starts using one shows up loudly.
    async fn list_profiles(&self) -> Result<ListProfilesResponse, AppError> {
        Ok(ListProfilesResponse {
            profiles: self.cron_profiles.clone(),
        })
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
        Ok(ListBlocklistsResponse {
            blocklists: self.cron_blocklists.clone(),
        })
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
        profile_id: Uuid,
        id: Uuid,
    ) -> Result<JobDispatchedResponse, AppError> {
        self.calls
            .lock()
            .unwrap()
            .refresh_blocklist
            .push((profile_id, id));
        Ok(JobDispatchedResponse {
            job_id: Uuid::new_v4(),
        })
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
    let events: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(64));
    // Cron interval long enough that no tick fires during the test —
    // we're only exercising the event-driven branches.
    let runner = DnsFilterRunner::start(
        svc_dyn,
        events.as_ref(),
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

// ---------------------------------------------------------------------------
// Cron-tick coverage — exercises `check_blocklist_cron`.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cron_tick_refreshes_due_blocklists() {
    // A profile with one due-for-refresh blocklist (cron `* * * * *`, never
    // refreshed) — the cron path should dispatch a refresh for it via the
    // service.
    let profile_id = Uuid::nil();
    let blocklist_id = Uuid::new_v4();
    let service = Arc::new(RecordingService {
        cron_profiles: vec![DnsFilterProfile {
            id: profile_id,
            name: "Ad Blocking".into(),
            description: None,
            builtin: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }],
        cron_blocklists: vec![Blocklist {
            id: blocklist_id,
            profile_id,
            name: "cron-list".into(),
            url: "http://example.test/c.txt".into(),
            enabled: true,
            entry_count: 0,
            last_updated: None,
            cron_schedule: "* * * * *".into(),
            last_error: None,
            last_error_at: None,
            consecutive_failures: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }],
        ..RecordingService::default()
    });
    let svc_dyn: Arc<dyn DnsFilterService> = service.clone();
    let events: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(64));

    let runner = DnsFilterRunner::start(
        svc_dyn,
        events.as_ref(),
        &tracing::Span::none(),
        // Tight cron interval — first tick fires almost immediately
        // after the runner skips the initial tick.
        Duration::from_millis(50),
    );

    // Wait for `check_blocklist_cron` to dispatch a refresh for the only
    // due-for-refresh blocklist the service returns.
    let hit = wait_until(2_000, || {
        service
            .snapshot()
            .refresh_blocklist
            .contains(&(profile_id, blocklist_id))
    })
    .await;
    assert!(
        hit,
        "cron should have dispatched a refresh for the due blocklist"
    );
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

// ── refresh backoff after failures ──────────────────────────────────────────

/// Build a blocklist with `failures` consecutive failures, the most recent
/// one `mins_ago` minutes in the past.
fn failing_blocklist(failures: u32, mins_ago: i64) -> Blocklist {
    Blocklist {
        id: Uuid::new_v4(),
        profile_id: Uuid::new_v4(),
        name: "failing".into(),
        url: "http://example.test/f.txt".into(),
        enabled: true,
        entry_count: 0,
        last_updated: None,
        cron_schedule: "* * * * *".into(),
        last_error: Some("download failed".into()),
        last_error_at: Some(Utc::now() - chrono::Duration::minutes(mins_ago)),
        consecutive_failures: failures,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

/// A healthy blocklist has nothing to back off from.
#[test]
fn retry_not_before_is_none_without_failures() {
    let mut bl = failing_blocklist(0, 0);
    bl.last_error = None;
    bl.last_error_at = None;
    assert!(crate::dns_filter::runner::retry_not_before(&bl).is_none());
}

/// A failure count with no recorded timestamp has no anchor to back off from,
/// so it must not block the retry indefinitely.
#[test]
fn retry_not_before_is_none_without_a_timestamp() {
    let mut bl = failing_blocklist(3, 0);
    bl.last_error_at = None;
    assert!(crate::dns_filter::runner::retry_not_before(&bl).is_none());
}

/// The interval doubles per consecutive failure and then holds at the cap.
#[test]
fn retry_backoff_doubles_then_caps() {
    // (failures, expected minutes after last_error_at)
    for (failures, expect_mins) in [
        (1_u32, 5_i64),
        (2, 10),
        (3, 20),
        (4, 40),
        (5, 80),
        (6, 160),
        (7, 320),
        (8, 360),  // capped
        (50, 360), // far past the cap, still capped
        (u32::MAX, 360),
    ] {
        let bl = failing_blocklist(failures, 0);
        let anchor = bl.last_error_at.unwrap();
        let got = crate::dns_filter::runner::retry_not_before(&bl).expect("should back off");
        assert_eq!(
            (got - anchor).num_minutes(),
            expect_mins,
            "failures={failures} should wait {expect_mins}m"
        );
    }
}

/// Backoff is measured from `last_error_at`, so a long-past failure is
/// retryable again immediately — the wait does not restart on daemon restart.
#[test]
fn retry_allowed_once_the_backoff_window_has_passed() {
    // 3 failures ⇒ 20m window, last failure 60m ago.
    let bl = failing_blocklist(3, 60);
    let retry_at = crate::dns_filter::runner::retry_not_before(&bl).expect("should back off");
    assert!(
        retry_at < Utc::now(),
        "a failure 60m ago with a 20m window should be retryable now"
    );

    // Same failure count, but it just happened — still held off.
    let fresh = failing_blocklist(3, 0);
    let retry_at = crate::dns_filter::runner::retry_not_before(&fresh).expect("should back off");
    assert!(
        retry_at > Utc::now(),
        "a failure just now with a 20m window must not retry yet"
    );
}
