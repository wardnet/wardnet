//! Shared doubles for the anomaly tests.
//!
//! `DnsFilterService` and `TunnelService` are broad traits and the detectors
//! touch a handful of methods each. Following the house pattern, everything
//! unused is `unimplemented!()` so a change that starts depending on one shows
//! up loudly instead of silently returning a default.

use std::net::IpAddr;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use hickory_proto::rr::RecordType;
use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use uuid::Uuid;
use wardnet_common::anomaly::{Anomaly, AnomalyReport, AnomalyStatus, AnomalyType};
use wardnet_common::api::*;
use wardnet_common::auth::AuthContext;
use wardnet_common::dns::{Blocklist, FilterAction};
use wardnet_common::dns_filter::{DnsFilterConfig, DnsFilterProfile};
use wardnet_common::jobs::JobDispatchedResponse;
use wardnet_common::speed_test::TunnelSpeedTestHistoryResponse;
use wardnet_common::tunnel::{Tunnel, TunnelStatus};

use crate::anomaly::detector::AnomalyDetector;
use crate::auth_context;
use crate::dns_filter::service::{CheckOutcome, DnsFilterService};
use crate::error::AppError;
use crate::push::PushService;
use crate::tunnel::TunnelService;

/// In-memory pool with the workspace migrations applied. The macro path is
/// relative to this crate, so it resolves up into the data crate.
pub async fn test_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../wardnetd-data/migrations")
        .run(&pool)
        .await
        .unwrap();
    pool
}

/// Run `fut` under the nil-admin context every background caller uses.
pub async fn as_admin<T>(fut: impl Future<Output = T>) -> T {
    auth_context::with_context(AuthContext::system(), fut).await
}

use std::future::Future;

pub fn blocklist(id: Uuid, profile_id: Uuid, name: &str, failures: u32) -> Blocklist {
    Blocklist {
        id,
        profile_id,
        name: name.to_owned(),
        url: "https://example.test/list.txt".to_owned(),
        enabled: true,
        entry_count: 0,
        last_updated: None,
        cron_schedule: "0 3 * * *".to_owned(),
        last_error: Some("download failed".to_owned()),
        last_error_at: Some(Utc::now()),
        consecutive_failures: failures,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

pub fn tunnel(id: Uuid, status: TunnelStatus) -> Tunnel {
    Tunnel {
        id,
        label: "US-1".to_owned(),
        country_code: "us".to_owned(),
        provider: None,
        interface_name: "wg_ward0".to_owned(),
        endpoint: "example.test:51820".to_owned(),
        status,
        last_handshake: None,
        bytes_tx: 0,
        bytes_rx: 0,
        created_at: Utc::now(),
        override_default_dns: false,
        server_selector: None,
        resolved_server_name: None,
        endpoint_resolved_at: None,
    }
}

// ---------------------------------------------------------------------------
// FakeDnsFilter
// ---------------------------------------------------------------------------

/// Serves a fixed profile/blocklist set and threshold to the blocklist
/// detector.
pub struct FakeDnsFilter {
    pub threshold: u32,
    pub profiles: Vec<DnsFilterProfileStub>,
}

/// One profile and the blocklists under it.
pub struct DnsFilterProfileStub {
    pub id: Uuid,
    pub blocklists: Vec<Blocklist>,
}

impl FakeDnsFilter {
    pub fn new(threshold: u32, profiles: Vec<DnsFilterProfileStub>) -> Arc<Self> {
        Arc::new(Self {
            threshold,
            profiles,
        })
    }
}

#[async_trait]
impl DnsFilterService for FakeDnsFilter {
    async fn check(&self, _d: &str, _q: RecordType, _c: IpAddr) -> CheckOutcome {
        CheckOutcome {
            action: FilterAction::Pass,
            would_have_blocked: false,
        }
    }
    async fn rebuild_all(&self) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn list_profiles(&self) -> Result<ListProfilesResponse, AppError> {
        Ok(ListProfilesResponse {
            profiles: self
                .profiles
                .iter()
                .map(|p| DnsFilterProfile {
                    id: p.id,
                    name: "Ad blocking".to_owned(),
                    description: None,
                    builtin: false,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                })
                .collect(),
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
    async fn list_blocklists(&self, profile_id: Uuid) -> Result<ListBlocklistsResponse, AppError> {
        self.profiles
            .iter()
            .find(|p| p.id == profile_id)
            .map(|p| ListBlocklistsResponse {
                blocklists: p.blocklists.clone(),
            })
            .ok_or_else(|| AppError::NotFound("profile not found".to_owned()))
    }
    async fn create_blocklist(
        &self,
        _p: Uuid,
        _r: CreateBlocklistRequest,
    ) -> Result<CreateBlocklistResponse, AppError> {
        unimplemented!()
    }
    async fn update_blocklist(
        &self,
        _p: Uuid,
        _id: Uuid,
        _r: UpdateBlocklistRequest,
    ) -> Result<UpdateBlocklistResponse, AppError> {
        unimplemented!()
    }
    async fn delete_blocklist(
        &self,
        _p: Uuid,
        _id: Uuid,
    ) -> Result<DeleteBlocklistResponse, AppError> {
        unimplemented!()
    }
    async fn refresh_blocklist(
        &self,
        _p: Uuid,
        _id: Uuid,
    ) -> Result<JobDispatchedResponse, AppError> {
        unimplemented!()
    }
    async fn list_allowlist(&self, _p: Uuid) -> Result<ListAllowlistResponse, AppError> {
        unimplemented!()
    }
    async fn create_allowlist_entry(
        &self,
        _p: Uuid,
        _r: CreateAllowlistRequest,
    ) -> Result<CreateAllowlistResponse, AppError> {
        unimplemented!()
    }
    async fn delete_allowlist_entry(
        &self,
        _p: Uuid,
        _id: Uuid,
    ) -> Result<DeleteAllowlistResponse, AppError> {
        unimplemented!()
    }
    async fn list_custom_rules(&self, _p: Uuid) -> Result<ListFilterRulesResponse, AppError> {
        unimplemented!()
    }
    async fn create_custom_rule(
        &self,
        _p: Uuid,
        _r: CreateFilterRuleRequest,
    ) -> Result<CreateFilterRuleResponse, AppError> {
        unimplemented!()
    }
    async fn update_custom_rule(
        &self,
        _p: Uuid,
        _id: Uuid,
        _r: UpdateFilterRuleRequest,
    ) -> Result<UpdateFilterRuleResponse, AppError> {
        unimplemented!()
    }
    async fn delete_custom_rule(
        &self,
        _p: Uuid,
        _id: Uuid,
    ) -> Result<DeleteFilterRuleResponse, AppError> {
        unimplemented!()
    }
    async fn list_device_settings(
        &self,
        _p: ListDeviceFilterSettingsParams,
    ) -> Result<ListDeviceFilterSettingsResponse, AppError> {
        unimplemented!()
    }
    async fn get_device_settings(
        &self,
        _d: Uuid,
    ) -> Result<GetDeviceFilterSettingsResponse, AppError> {
        unimplemented!()
    }
    async fn update_device_settings(
        &self,
        _d: Uuid,
        _r: UpdateDeviceFilterSettingsRequest,
    ) -> Result<UpdateDeviceFilterSettingsResponse, AppError> {
        unimplemented!()
    }
    async fn get_filter_config(&self) -> Result<DnsFilterConfigResponse, AppError> {
        Ok(DnsFilterConfigResponse {
            config: DnsFilterConfig {
                enabled: true,
                default_profile_ids: Vec::new(),
                blocklist_failure_alert_threshold: self.threshold,
            },
            filters_ready: true,
            pending_blocklists: 0,
        })
    }
    async fn update_filter_config(
        &self,
        _r: UpdateDnsFilterConfigRequest,
    ) -> Result<DnsFilterConfigResponse, AppError> {
        unimplemented!()
    }
    async fn rebuild_blocklist_filter(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn rebuild_profile(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn rebuild_device(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn rebuild_default_context(&self) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn handle_device_ip_changed(&self, _d: Uuid, _o: &str, _n: &str) -> Result<(), AppError> {
        unimplemented!()
    }
}

// ---------------------------------------------------------------------------
// FakeTunnels
// ---------------------------------------------------------------------------

/// Answers `get_tunnel` from a fixed list; anything absent is a 404, which is
/// how the tunnel detectors see a deleted tunnel.
pub struct FakeTunnels {
    pub tunnels: Vec<Tunnel>,
}

impl FakeTunnels {
    pub fn new(tunnels: Vec<Tunnel>) -> Arc<Self> {
        Arc::new(Self { tunnels })
    }
}

#[async_trait]
impl TunnelService for FakeTunnels {
    async fn import_tunnel(
        &self,
        _r: CreateTunnelRequest,
    ) -> Result<CreateTunnelResponse, AppError> {
        unimplemented!()
    }
    async fn list_tunnels(&self) -> Result<ListTunnelsResponse, AppError> {
        unimplemented!()
    }
    async fn get_tunnel(&self, id: Uuid) -> Result<Tunnel, AppError> {
        self.tunnels
            .iter()
            .find(|t| t.id == id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("tunnel '{id}' not found")))
    }
    async fn test_tunnel(&self, _id: Uuid) -> Result<TunnelTestResult, AppError> {
        unimplemented!()
    }
    async fn list_tunnel_devices(&self, _id: Uuid) -> Result<TunnelDevicesResponse, AppError> {
        unimplemented!()
    }
    async fn set_dns_override(&self, _id: Uuid, _v: bool) -> Result<Tunnel, AppError> {
        unimplemented!()
    }
    async fn rebuild(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn bring_up(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn tear_down(&self, _id: Uuid, _r: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn delete_tunnel(&self, _id: Uuid) -> Result<DeleteTunnelResponse, AppError> {
        unimplemented!()
    }
    async fn bring_up_internal(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn tear_down_internal(&self, _id: Uuid, _r: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn restore_tunnels(&self) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn collect_stats(&self) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn run_health_check(&self) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn probe_latencies(&self) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn start_speed_test(
        self: Arc<Self>,
        _id: Uuid,
    ) -> Result<JobDispatchedResponse, AppError> {
        unimplemented!()
    }
    async fn list_speed_tests(
        &self,
        _id: Uuid,
    ) -> Result<TunnelSpeedTestHistoryResponse, AppError> {
        unimplemented!()
    }
}

// ---------------------------------------------------------------------------
// RecordingPush
// ---------------------------------------------------------------------------

/// Records which anomaly notifications were delivered, so tests can assert on
/// exactly-once alerting.
#[derive(Default)]
pub struct RecordingPush {
    pub opened: StdMutex<Vec<Uuid>>,
    pub resolved: StdMutex<Vec<Uuid>>,
    /// When set, delivery fails — used to prove `notified_at` is only stamped
    /// on success, so the recovery notice stays gated.
    pub fail: bool,
}

impl RecordingPush {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn failing() -> Arc<Self> {
        Arc::new(Self {
            fail: true,
            ..Self::default()
        })
    }

    pub fn opened_ids(&self) -> Vec<Uuid> {
        self.opened.lock().unwrap().clone()
    }

    pub fn resolved_ids(&self) -> Vec<Uuid> {
        self.resolved.lock().unwrap().clone()
    }
}

#[async_trait]
impl PushService for RecordingPush {
    async fn vapid_public_key(&self) -> Result<String, AppError> {
        unimplemented!()
    }
    async fn subscribe(&self, _s: WebPushSubscription) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn unsubscribe(&self, _e: Option<String>) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn handle_event(&self, _e: &wardnet_common::event::WardnetEvent) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn recent_notifications(
        &self,
        _l: u32,
    ) -> Result<Vec<wardnetd_data::repository::StoredNotification>, AppError> {
        unimplemented!()
    }
    async fn clear_notifications(&self) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn notify_anomaly_opened(&self, anomaly: &Anomaly) -> Result<(), AppError> {
        if self.fail {
            return Err(AppError::Internal(anyhow::anyhow!("push is down")));
        }
        self.opened.lock().unwrap().push(anomaly.id);
        Ok(())
    }
    async fn notify_anomaly_resolved(&self, anomaly: &Anomaly) -> Result<(), AppError> {
        if self.fail {
            return Err(AppError::Internal(anyhow::anyhow!("push is down")));
        }
        self.resolved.lock().unwrap().push(anomaly.id);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// FakeDetector
// ---------------------------------------------------------------------------

/// A scriptable detector: fixed verdict, fixed reports, optional hang.
pub struct FakeDetector {
    pub anomaly_type: AnomalyType,
    pub interval: Option<Duration>,
    pub stale_after: Option<Duration>,
    pub verdict: AnomalyStatus,
    pub reports: Vec<AnomalyReport>,
    /// Makes both `detect` and `reevaluate` hang, to exercise the timeout.
    pub hang: bool,
    /// Makes both calls fail, to exercise the error path.
    pub fail: bool,
    pub detect_calls: StdMutex<u32>,
}

impl FakeDetector {
    pub fn new(anomaly_type: AnomalyType) -> Self {
        Self {
            anomaly_type,
            interval: None,
            stale_after: None,
            verdict: AnomalyStatus::Open,
            reports: Vec::new(),
            hang: false,
            fail: false,
            detect_calls: StdMutex::new(0),
        }
    }

    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = Some(interval);
        self
    }

    pub fn with_stale_after(mut self, stale_after: Duration) -> Self {
        self.stale_after = Some(stale_after);
        self
    }

    pub fn with_verdict(mut self, verdict: AnomalyStatus) -> Self {
        self.verdict = verdict;
        self
    }

    pub fn with_reports(mut self, reports: Vec<AnomalyReport>) -> Self {
        self.reports = reports;
        self
    }

    pub fn hanging(mut self) -> Self {
        self.hang = true;
        self
    }

    pub fn failing(mut self) -> Self {
        self.fail = true;
        self
    }

    pub fn detect_count(&self) -> u32 {
        *self.detect_calls.lock().unwrap()
    }
}

#[async_trait]
impl AnomalyDetector for FakeDetector {
    fn anomaly_type(&self) -> AnomalyType {
        self.anomaly_type
    }

    fn interval(&self) -> Option<Duration> {
        self.interval
    }

    fn stale_after(&self) -> Option<Duration> {
        self.stale_after
    }

    async fn detect(&self) -> anyhow::Result<Vec<AnomalyReport>> {
        *self.detect_calls.lock().unwrap() += 1;
        if self.fail {
            anyhow::bail!("detector exploded");
        }
        if self.hang {
            std::future::pending::<()>().await;
        }
        Ok(self.reports.clone())
    }

    async fn reevaluate(&self, _anomaly: &Anomaly) -> anyhow::Result<AnomalyStatus> {
        if self.fail {
            anyhow::bail!("detector exploded");
        }
        if self.hang {
            std::future::pending::<()>().await;
        }
        Ok(self.verdict)
    }
}

/// `DhcpService` double for the renewal-storm detector.
///
/// Only the three methods that detector touches are real; everything else is
/// `unimplemented!()` so a change that starts depending on one shows up loudly.
pub struct FakeDhcpService {
    lease_duration_secs: u32,
    counts: Vec<(String, i64)>,
}

impl FakeDhcpService {
    #[must_use]
    pub fn new(lease_duration_secs: u32, counts: &[(&str, i64)]) -> Self {
        Self {
            lease_duration_secs,
            counts: counts
                .iter()
                .map(|(mac, n)| ((*mac).to_owned(), *n))
                .collect(),
        }
    }
}

#[async_trait]
impl crate::dhcp::DhcpService for FakeDhcpService {
    async fn lease_logs_for_mac_between(
        &self,
        _mac: &str,
        _from: chrono::DateTime<Utc>,
        _to: chrono::DateTime<Utc>,
    ) -> Result<Vec<wardnet_common::dhcp::DhcpLeaseLog>, AppError> {
        unimplemented!()
    }

    async fn get_dhcp_config(&self) -> Result<wardnet_common::dhcp::DhcpConfig, AppError> {
        Ok(wardnet_common::dhcp::DhcpConfig {
            enabled: true,
            gateway_ip: "192.168.100.1".parse().unwrap(),
            pool_start: "192.168.100.100".parse().unwrap(),
            pool_end: "192.168.100.200".parse().unwrap(),
            subnet_mask: "255.255.255.0".parse().unwrap(),
            upstream_dns: vec![],
            lease_duration_secs: self.lease_duration_secs,
            router_ip: None,
        })
    }

    async fn renewal_counts_since(
        &self,
        _since: chrono::DateTime<Utc>,
    ) -> Result<Vec<(String, i64)>, AppError> {
        Ok(self.counts.clone())
    }

    async fn renewal_count_for_mac_since(
        &self,
        mac: &str,
        _since: chrono::DateTime<Utc>,
    ) -> Result<i64, AppError> {
        Ok(self
            .counts
            .iter()
            .find(|(m, _)| m == mac)
            .map_or(0, |(_, n)| *n))
    }

    async fn get_config(&self) -> Result<DhcpConfigResponse, AppError> {
        unimplemented!()
    }
    async fn update_config(
        &self,
        _req: UpdateDhcpConfigRequest,
    ) -> Result<DhcpConfigResponse, AppError> {
        unimplemented!()
    }
    async fn preview_config(
        &self,
        _req: PreviewDhcpConfigRequest,
    ) -> Result<PreviewDhcpConfigResponse, AppError> {
        unimplemented!()
    }
    async fn toggle(&self, _req: ToggleDhcpRequest) -> Result<DhcpConfigResponse, AppError> {
        unimplemented!()
    }
    async fn list_leases(&self) -> Result<ListDhcpLeasesResponse, AppError> {
        unimplemented!()
    }
    async fn revoke_lease(&self, _id: Uuid) -> Result<RevokeDhcpLeaseResponse, AppError> {
        unimplemented!()
    }
    async fn list_reservations(&self) -> Result<ListDhcpReservationsResponse, AppError> {
        unimplemented!()
    }
    async fn create_reservation(
        &self,
        _req: CreateDhcpReservationRequest,
    ) -> Result<CreateDhcpReservationResponse, AppError> {
        unimplemented!()
    }
    async fn delete_reservation(
        &self,
        _id: Uuid,
    ) -> Result<DeleteDhcpReservationResponse, AppError> {
        unimplemented!()
    }
    async fn status(&self) -> Result<DhcpStatusResponse, AppError> {
        unimplemented!()
    }
    async fn assign_lease(
        &self,
        _mac: &str,
        _hostname: Option<&str>,
    ) -> Result<wardnet_common::dhcp::DhcpLease, AppError> {
        unimplemented!()
    }
    async fn renew_lease(
        &self,
        _mac: &str,
        _hostname: Option<&str>,
    ) -> Result<wardnet_common::dhcp::DhcpLease, AppError> {
        unimplemented!()
    }
    async fn release_lease(&self, _mac: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn cleanup_expired(&self) -> Result<u64, AppError> {
        unimplemented!()
    }
    async fn scope_for_mac(&self, _mac: &str) -> Result<wardnet_common::dhcp::DhcpScope, AppError> {
        unimplemented!()
    }
}

/// `DeviceEventService` double for the address-churn detector.
pub struct FakeDeviceEvents {
    counts: Vec<(String, i64)>,
}

impl FakeDeviceEvents {
    #[must_use]
    pub fn new(counts: &[(&str, i64)]) -> Arc<Self> {
        Arc::new(Self {
            counts: counts
                .iter()
                .map(|(mac, n)| ((*mac).to_owned(), *n))
                .collect(),
        })
    }
}

#[async_trait]
impl crate::device_event::DeviceEventService for FakeDeviceEvents {
    async fn count_by_mac_since(
        &self,
        _kind: wardnet_common::device_event::DeviceEventKind,
        _since: chrono::DateTime<Utc>,
    ) -> Result<Vec<(String, i64)>, AppError> {
        Ok(self.counts.clone())
    }

    async fn count_for_mac_since(
        &self,
        mac: &str,
        _kind: wardnet_common::device_event::DeviceEventKind,
        _since: chrono::DateTime<Utc>,
    ) -> Result<i64, AppError> {
        Ok(self
            .counts
            .iter()
            .find(|(m, _)| m == mac)
            .map_or(0, |(_, n)| *n))
    }

    async fn record(
        &self,
        _intent: crate::device_event::DeviceEventIntent,
    ) -> Result<(), AppError> {
        unimplemented!()
    }

    async fn list_for_device(
        &self,
        _device_id: Uuid,
        _from: chrono::DateTime<Utc>,
        _to: chrono::DateTime<Utc>,
    ) -> Result<Vec<wardnet_common::device_event::DeviceEvent>, AppError> {
        unimplemented!()
    }

    async fn prune(&self) -> Result<u64, AppError> {
        unimplemented!()
    }
}
