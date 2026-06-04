//! Minimal stub implementations of all service traits for use in tests.
//!
//! Every method panics with `unimplemented!()` — tests that need real
//! behaviour should define their own mock types and only stub the
//! services they don't exercise.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::broadcast;
use uuid::Uuid;

use wardnet_common::api::*;
use wardnet_common::device::{Device, DeviceType};
use wardnet_common::routing::RoutingTarget;
use wardnet_common::tunnel::Tunnel;

use wardnetd_services::auth::service::{LoginResult, WizardState};
use wardnetd_services::device::ObservationResult;
use wardnetd_services::dhcp::server::DhcpServer;
use wardnetd_services::dns::server::DnsServer;
use wardnetd_services::error::AppError;
use wardnetd_services::event::EventPublisher;
use wardnetd_services::jobs::{BoxedJobTask, JobService};
use wardnetd_services::logging::component::BoxedLayer;
use wardnetd_services::logging::error_notifier::ErrorEntry;
use wardnetd_services::logging::service::{LogFileInfo, LogService};
use wardnetd_services::logging::stream::LogEntry;
use wardnetd_services::{
    AuthService, DeviceDiscoveryService, DeviceService, DhcpService, DnsService, RoutingService,
    StatsService, SystemService, TunnelService, VpnProviderService,
};

use crate::state::AppState;

// ---------------------------------------------------------------------------
// Stub services
// ---------------------------------------------------------------------------

pub struct StubAuthService;
#[async_trait]
impl AuthService for StubAuthService {
    async fn login(&self, _u: &str, _p: &str, _remember_me: bool) -> Result<LoginResult, AppError> {
        unimplemented!()
    }
    async fn validate_session(&self, _token: &str) -> Result<Option<Uuid>, AppError> {
        Ok(None)
    }
    async fn validate_api_key(&self, _key: &str) -> Result<Option<Uuid>, AppError> {
        Ok(None)
    }
    async fn setup_admin(&self, _u: &str, _p: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn is_setup_completed(&self) -> Result<bool, AppError> {
        Ok(true)
    }
    async fn wizard_state(&self) -> Result<WizardState, AppError> {
        Ok(WizardState {
            step: WizardStep::Completed,
            mode: None,
        })
    }
    async fn advance_wizard(
        &self,
        _to_step: WizardStep,
        _mode: Option<WizardMode>,
    ) -> Result<WizardState, AppError> {
        unimplemented!()
    }
    async fn refresh_session(&self, _token: &str) -> Result<LoginResult, AppError> {
        unimplemented!()
    }
}

pub struct StubDeviceService;
#[async_trait]
impl DeviceService for StubDeviceService {
    async fn get_device_for_ip(&self, _ip: &str) -> Result<DeviceMeResponse, AppError> {
        unimplemented!()
    }
    async fn set_rule_for_ip(
        &self,
        _ip: &str,
        _t: RoutingTarget,
    ) -> Result<SetMyRuleResponse, AppError> {
        unimplemented!()
    }
    async fn set_rule(&self, _id: &str, _t: RoutingTarget) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn current_rules(
        &self,
    ) -> Result<std::collections::HashMap<Uuid, RoutingTarget>, AppError> {
        unimplemented!()
    }
    async fn update_admin_locked(&self, _id: &str, _locked: bool) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn get_dns_capture_settings(
        &self,
        _id: &str,
    ) -> Result<wardnet_common::api::DnsCaptureSettingsResponse, AppError> {
        unimplemented!()
    }
    async fn update_dns_capture_settings(
        &self,
        _id: &str,
        _enabled: Option<bool>,
        _cap_count: Option<i64>,
        _cap_days: Option<i64>,
    ) -> Result<(), AppError> {
        unimplemented!()
    }
}

pub struct StubDhcpService;
#[async_trait]
impl DhcpService for StubDhcpService {
    async fn get_config(&self) -> Result<DhcpConfigResponse, AppError> {
        unimplemented!()
    }
    async fn update_config(
        &self,
        _r: UpdateDhcpConfigRequest,
    ) -> Result<DhcpConfigResponse, AppError> {
        unimplemented!()
    }
    async fn toggle(&self, _r: ToggleDhcpRequest) -> Result<DhcpConfigResponse, AppError> {
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
        _r: CreateDhcpReservationRequest,
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
    async fn get_dhcp_config(&self) -> Result<wardnet_common::dhcp::DhcpConfig, AppError> {
        unimplemented!()
    }
}

pub struct StubDnsService;
#[async_trait]
impl DnsService for StubDnsService {
    async fn get_config(&self) -> Result<DnsConfigResponse, AppError> {
        unimplemented!()
    }
    async fn update_config(
        &self,
        _r: UpdateDnsConfigRequest,
    ) -> Result<DnsConfigResponse, AppError> {
        unimplemented!()
    }
    async fn toggle(&self, _r: ToggleDnsRequest) -> Result<DnsConfigResponse, AppError> {
        unimplemented!()
    }
    async fn status(&self) -> Result<DnsStatusResponse, AppError> {
        unimplemented!()
    }
    async fn flush_cache(&self) -> Result<DnsCacheFlushResponse, AppError> {
        unimplemented!()
    }
    async fn get_dns_config(&self) -> Result<wardnet_common::dns::DnsConfig, AppError> {
        unimplemented!()
    }
    async fn list_query_log(
        &self,
        _params: wardnet_common::api::ListQueryLogParams,
    ) -> Result<wardnet_common::api::ListQueryLogResponse, AppError> {
        unimplemented!()
    }
    fn subscribe_query_stream(
        &self,
    ) -> Result<tokio::sync::broadcast::Receiver<wardnet_common::api::QueryLogEvent>, AppError>
    {
        unimplemented!()
    }
    async fn flush_query_log(&self) -> Result<u64, AppError> {
        unimplemented!()
    }
}

pub struct StubDnsFilterService;
#[async_trait]
impl wardnetd_services::DnsFilterService for StubDnsFilterService {
    async fn check(
        &self,
        _domain: &str,
        _qtype: hickory_proto::rr::RecordType,
        _client: std::net::IpAddr,
    ) -> wardnetd_services::dns_filter::service::CheckOutcome {
        wardnetd_services::dns_filter::service::CheckOutcome {
            action: wardnet_common::dns::FilterAction::Pass,
            would_have_blocked: false,
        }
    }
    async fn rebuild_all(&self) -> Result<(), AppError> {
        Ok(())
    }
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
    ) -> Result<wardnet_common::jobs::JobDispatchedResponse, AppError> {
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
    async fn rebuild_blocklist_filter(&self, _id: Uuid) -> Result<(), AppError> {
        Ok(())
    }
    async fn rebuild_profile(&self, _id: Uuid) -> Result<(), AppError> {
        Ok(())
    }
    async fn rebuild_device(&self, _id: Uuid) -> Result<(), AppError> {
        Ok(())
    }
    async fn rebuild_default_context(&self) -> Result<(), AppError> {
        Ok(())
    }
    async fn handle_device_ip_changed(
        &self,
        _device_id: Uuid,
        _old_ip: &str,
        _new_ip: &str,
    ) -> Result<(), AppError> {
        Ok(())
    }
}

pub struct StubDnsLocalService;
#[async_trait]
impl wardnetd_services::DnsLocalService for StubDnsLocalService {
    async fn list_zones(&self) -> Result<ListZonesResponse, AppError> {
        unimplemented!()
    }
    async fn get_zone(&self, _id: Uuid) -> Result<GetZoneResponse, AppError> {
        unimplemented!()
    }
    async fn create_zone(&self, _r: CreateZoneRequest) -> Result<CreateZoneResponse, AppError> {
        unimplemented!()
    }
    async fn update_zone(
        &self,
        _id: Uuid,
        _r: UpdateZoneRequest,
    ) -> Result<UpdateZoneResponse, AppError> {
        unimplemented!()
    }
    async fn delete_zone(&self, _id: Uuid) -> Result<DeleteZoneResponse, AppError> {
        unimplemented!()
    }
    async fn list_zone_records(&self, _zone_id: Uuid) -> Result<ListRecordsResponse, AppError> {
        unimplemented!()
    }
    async fn list_records(&self) -> Result<ListRecordsResponse, AppError> {
        unimplemented!()
    }
    async fn get_record(&self, _id: Uuid) -> Result<GetRecordResponse, AppError> {
        unimplemented!()
    }
    async fn create_record(
        &self,
        _r: CreateRecordRequest,
    ) -> Result<CreateRecordResponse, AppError> {
        unimplemented!()
    }
    async fn update_record(
        &self,
        _id: Uuid,
        _r: UpdateRecordRequest,
    ) -> Result<UpdateRecordResponse, AppError> {
        unimplemented!()
    }
    async fn delete_record(&self, _id: Uuid) -> Result<DeleteRecordResponse, AppError> {
        unimplemented!()
    }
    async fn list_forwarding_rules(&self) -> Result<ListForwardingRulesResponse, AppError> {
        unimplemented!()
    }
    async fn get_forwarding_rule(&self, _id: Uuid) -> Result<GetForwardingRuleResponse, AppError> {
        unimplemented!()
    }
    async fn create_forwarding_rule(
        &self,
        _r: CreateForwardingRuleRequest,
    ) -> Result<CreateForwardingRuleResponse, AppError> {
        unimplemented!()
    }
    async fn update_forwarding_rule(
        &self,
        _id: Uuid,
        _r: UpdateForwardingRuleRequest,
    ) -> Result<UpdateForwardingRuleResponse, AppError> {
        unimplemented!()
    }
    async fn delete_forwarding_rule(
        &self,
        _id: Uuid,
    ) -> Result<DeleteForwardingRuleResponse, AppError> {
        unimplemented!()
    }
}

pub struct StubDiscoveryService;
#[async_trait]
impl DeviceDiscoveryService for StubDiscoveryService {
    async fn restore_devices(&self) -> Result<(), AppError> {
        Ok(())
    }
    async fn process_observation(
        &self,
        _obs: &wardnetd_services::device::packet_capture::ObservedDevice,
    ) -> Result<ObservationResult, AppError> {
        unimplemented!()
    }
    async fn flush_last_seen(&self) -> Result<u64, AppError> {
        Ok(0)
    }
    async fn scan_departures(&self, _timeout_secs: u64) -> Result<Vec<Uuid>, AppError> {
        Ok(vec![])
    }
    async fn resolve_hostname(&self, _mac: &str, _ip: &str) -> Result<(), AppError> {
        Ok(())
    }
    async fn get_all_devices(&self) -> Result<Vec<Device>, AppError> {
        Ok(vec![])
    }
    async fn get_device_by_id(&self, id: Uuid) -> Result<Device, AppError> {
        Err(AppError::NotFound(format!("device {id} not found")))
    }
    async fn update_device(
        &self,
        _id: Uuid,
        _name: Option<&str>,
        _device_type: Option<DeviceType>,
    ) -> Result<Device, AppError> {
        unimplemented!()
    }
}

pub struct StubProviderService;
#[async_trait]
impl VpnProviderService for StubProviderService {
    async fn list_providers(&self) -> Result<ListProvidersResponse, AppError> {
        unimplemented!()
    }
    async fn list_countries(&self, _provider_id: &str) -> Result<ListCountriesResponse, AppError> {
        unimplemented!()
    }
    async fn validate_credentials(
        &self,
        _provider_id: &str,
        _r: ValidateCredentialsRequest,
    ) -> Result<ValidateCredentialsResponse, AppError> {
        unimplemented!()
    }
    async fn list_servers(
        &self,
        _provider_id: &str,
        _r: ListServersRequest,
    ) -> Result<ListServersResponse, AppError> {
        unimplemented!()
    }
    async fn setup_tunnel(
        &self,
        _provider_id: &str,
        _r: SetupProviderRequest,
    ) -> Result<SetupProviderResponse, AppError> {
        unimplemented!()
    }
}

pub struct StubRoutingService;
#[async_trait]
impl RoutingService for StubRoutingService {
    async fn apply_rule(
        &self,
        _device_id: Uuid,
        _device_ip: &str,
        _target: &RoutingTarget,
    ) -> Result<(), AppError> {
        Ok(())
    }
    async fn remove_device_routes(
        &self,
        _device_id: Uuid,
        _device_ip: &str,
    ) -> Result<(), AppError> {
        Ok(())
    }
    async fn handle_ip_change(
        &self,
        _device_id: Uuid,
        _old_ip: &str,
        _new_ip: &str,
    ) -> Result<(), AppError> {
        Ok(())
    }
    async fn handle_tunnel_down(&self, _tunnel_id: Uuid) -> Result<(), AppError> {
        Ok(())
    }
    async fn handle_tunnel_up(&self, _tunnel_id: Uuid) -> Result<(), AppError> {
        Ok(())
    }
    async fn reconcile(&self) -> Result<(), AppError> {
        Ok(())
    }
    async fn handle_route_table_lost(&self, _table: u32) -> Result<(), AppError> {
        Ok(())
    }
    async fn devices_using_tunnel(&self, _tunnel_id: Uuid) -> Result<Vec<Uuid>, AppError> {
        Ok(vec![])
    }
    async fn apply_rule_for_device(
        &self,
        _device_id: Uuid,
        _target: &RoutingTarget,
    ) -> Result<(), AppError> {
        Ok(())
    }
    async fn apply_rule_for_discovered_device(
        &self,
        _device_id: Uuid,
        _ip: &str,
    ) -> Result<(), AppError> {
        Ok(())
    }
    async fn set_default_policy(&self, _policy: &str) -> Result<(), AppError> {
        Ok(())
    }
    async fn default_policy(&self) -> Result<String, AppError> {
        Ok("direct".to_owned())
    }
    fn dns_upstream_snapshot(
        &self,
    ) -> std::sync::Arc<
        arc_swap::ArcSwap<
            std::collections::HashMap<std::net::IpAddr, wardnet_common::dns::UpstreamId>,
        >,
    > {
        std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(
            std::collections::HashMap::new(),
        ))
    }
    async fn rebuild_dns_upstream_snapshot(&self) -> Result<(), AppError> {
        Ok(())
    }
}

pub struct StubSystemService;
#[async_trait]
impl SystemService for StubSystemService {
    fn version(&self) -> &'static str {
        "0.1.0-test"
    }
    fn uptime(&self) -> std::time::Duration {
        std::time::Duration::from_secs(42)
    }
    async fn status(&self) -> Result<SystemStatusResponse, AppError> {
        Ok(SystemStatusResponse {
            version: self.version().to_owned(),
            release_version: "0.0.0-test".to_owned(),
            uptime_seconds: 42,
            device_count: 0,
            tunnel_count: 0,
            tunnel_active_count: 0,
            db_size_bytes: 0,
            cpu_usage_percent: 0.0,
            memory_used_bytes: 0,
            memory_total_bytes: 0,
            disk_free_bytes: 0,
            disk_total_bytes: 0,
            last_shutdown: LastShutdownStatus {
                state: LastShutdownState::Unknown,
                at: None,
                acknowledged_at: None,
            },
        })
    }
    async fn request_restart(&self) -> Result<(), AppError> {
        Ok(())
    }
    async fn request_reboot(&self) -> Result<(), AppError> {
        Ok(())
    }
    async fn request_shutdown(&self) -> Result<(), AppError> {
        Ok(())
    }
    async fn network_status(&self) -> Result<wardnet_common::api::NetworkStatusResponse, AppError> {
        unimplemented!()
    }
    async fn discover_gateway_mac(
        &self,
        _request: wardnet_common::api::DiscoverGatewayMacRequest,
    ) -> Result<wardnet_common::api::DiscoverGatewayMacResponse, AppError> {
        unimplemented!()
    }
    async fn dhcp_self_probe(
        &self,
    ) -> Result<wardnet_common::api::DhcpSelfProbeResponse, AppError> {
        unimplemented!()
    }
    async fn record_heartbeat(&self) -> Result<(), AppError> {
        Ok(())
    }
    async fn record_graceful_shutdown(&self) -> Result<(), AppError> {
        Ok(())
    }
    async fn acknowledge_last_shutdown(&self) -> Result<(), AppError> {
        Ok(())
    }
}

pub struct StubTunnelService;
#[async_trait]
impl TunnelService for StubTunnelService {
    async fn import_tunnel(
        &self,
        _r: CreateTunnelRequest,
    ) -> Result<CreateTunnelResponse, AppError> {
        unimplemented!()
    }
    async fn list_tunnels(&self) -> Result<ListTunnelsResponse, AppError> {
        Ok(ListTunnelsResponse { tunnels: vec![] })
    }
    async fn get_tunnel(&self, _id: Uuid) -> Result<Tunnel, AppError> {
        unimplemented!()
    }
    async fn test_tunnel(
        &self,
        _id: Uuid,
    ) -> Result<wardnet_common::api::TunnelTestResult, AppError> {
        unimplemented!()
    }
    async fn list_tunnel_devices(
        &self,
        _id: Uuid,
    ) -> Result<wardnet_common::api::TunnelDevicesResponse, AppError> {
        unimplemented!()
    }
    async fn set_dns_override(
        &self,
        _id: Uuid,
        _value: bool,
    ) -> Result<wardnet_common::tunnel::Tunnel, AppError> {
        unimplemented!()
    }
    async fn rebuild(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn bring_up(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn tear_down(&self, _id: Uuid, _reason: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn bring_up_internal(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn tear_down_internal(&self, _id: Uuid, _reason: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn delete_tunnel(&self, _id: Uuid) -> Result<DeleteTunnelResponse, AppError> {
        unimplemented!()
    }
    async fn restore_tunnels(&self) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn collect_stats(&self) -> Result<(), AppError> {
        Ok(())
    }
    async fn run_health_check(&self) -> Result<(), AppError> {
        Ok(())
    }
    async fn probe_latencies(&self) -> Result<(), AppError> {
        Ok(())
    }
}

pub struct StubLogService;
#[async_trait]
impl LogService for StubLogService {
    fn subscribe(&self) -> broadcast::Receiver<LogEntry> {
        let (tx, rx) = broadcast::channel(1);
        drop(tx);
        rx
    }
    fn get_recent_errors(&self) -> Vec<ErrorEntry> {
        Vec::new()
    }
    async fn list_log_files(&self) -> Result<Vec<LogFileInfo>, AppError> {
        Ok(Vec::new())
    }
    async fn download_log_file(&self, _name: Option<&str>) -> Result<String, AppError> {
        Ok(String::new())
    }
    fn tracing_layers(&self) -> Vec<BoxedLayer> {
        Vec::new()
    }
    fn start_all(&self) {}
    fn stop_all(&self) {}
}

pub struct StubDhcpServer;
#[async_trait]
impl DhcpServer for StubDhcpServer {
    async fn start(&self) -> Result<(), AppError> {
        Ok(())
    }
    async fn stop(&self) -> Result<(), AppError> {
        Ok(())
    }
    fn is_running(&self) -> bool {
        false
    }
}

pub struct StubDnsServer;
#[async_trait]
impl DnsServer for StubDnsServer {
    async fn start(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn stop(&self) -> anyhow::Result<()> {
        Ok(())
    }
    fn is_running(&self) -> bool {
        false
    }
    async fn flush_cache(&self) -> u64 {
        0
    }
    async fn cache_size(&self) -> u64 {
        0
    }
    async fn cache_hit_rate(&self) -> f64 {
        0.0
    }
    async fn update_config(&self, _config: wardnet_common::dns::DnsConfig) {}
}

pub struct StubJobService;
impl StubJobService {
    pub fn new_arc() -> Arc<dyn JobService> {
        Arc::new(StubJobService)
    }
}
#[async_trait]
impl JobService for StubJobService {
    async fn dispatch_boxed(
        &self,
        _kind: wardnet_common::jobs::JobKind,
        _task: BoxedJobTask,
    ) -> Uuid {
        unimplemented!()
    }
    async fn get(&self, _id: Uuid) -> Option<wardnet_common::jobs::Job> {
        None
    }
}

pub struct StubEventPublisher;
impl EventPublisher for StubEventPublisher {
    fn publish(&self, _event: wardnet_common::event::WardnetEvent) {}
    fn subscribe(&self) -> broadcast::Receiver<wardnet_common::event::WardnetEvent> {
        let (tx, rx) = broadcast::channel(1);
        drop(tx);
        rx
    }
}

pub struct StubBackupService;
#[async_trait]
impl wardnetd_services::BackupService for StubBackupService {
    async fn status(&self) -> Result<wardnet_common::api::BackupStatusResponse, AppError> {
        unimplemented!()
    }
    async fn export(
        &self,
        _req: wardnet_common::api::ExportBackupRequest,
    ) -> Result<Vec<u8>, AppError> {
        unimplemented!()
    }
    async fn preview_import(
        &self,
        _bundle: Vec<u8>,
        _passphrase: String,
    ) -> Result<wardnet_common::api::RestorePreviewResponse, AppError> {
        unimplemented!()
    }
    async fn apply_import(
        &self,
        _req: wardnet_common::api::ApplyImportRequest,
    ) -> Result<wardnet_common::api::ApplyImportResponse, AppError> {
        unimplemented!()
    }
    async fn list_snapshots(&self) -> Result<wardnet_common::api::ListSnapshotsResponse, AppError> {
        unimplemented!()
    }
    async fn cleanup_old_snapshots(&self, _retain: std::time::Duration) -> Result<u32, AppError> {
        Ok(0)
    }
}

pub struct StubUpdateService;
#[async_trait]
impl wardnetd_services::UpdateService for StubUpdateService {
    async fn status(&self) -> Result<wardnet_common::api::UpdateStatusResponse, AppError> {
        unimplemented!()
    }
    async fn check(&self) -> Result<wardnet_common::api::UpdateCheckResponse, AppError> {
        unimplemented!()
    }
    async fn install(
        &self,
        _req: wardnet_common::api::InstallUpdateRequest,
    ) -> Result<wardnet_common::api::InstallUpdateResponse, AppError> {
        unimplemented!()
    }
    async fn rollback(&self) -> Result<wardnet_common::api::RollbackResponse, AppError> {
        unimplemented!()
    }
    async fn update_config(
        &self,
        _req: wardnet_common::api::UpdateConfigRequest,
    ) -> Result<wardnet_common::api::UpdateConfigResponse, AppError> {
        unimplemented!()
    }
    async fn history(
        &self,
        _limit: u32,
    ) -> Result<wardnet_common::api::UpdateHistoryResponse, AppError> {
        unimplemented!()
    }
    async fn auto_install_if_due(
        &self,
    ) -> Result<Option<wardnet_common::update::InstallHandle>, AppError> {
        Ok(None)
    }
}

// ---------------------------------------------------------------------------
// StubStatsService
// ---------------------------------------------------------------------------

pub struct StubStatsService;

#[async_trait]
impl StatsService for StubStatsService {
    async fn query(
        &self,
        _q: wardnet_common::stats::StatsQuery,
    ) -> Result<wardnet_common::stats::StatsQueryResponse, AppError> {
        unimplemented!()
    }
    async fn top(
        &self,
        _q: wardnet_common::stats::StatsTopQuery,
    ) -> Result<wardnet_common::stats::StatsTopResponse, AppError> {
        unimplemented!()
    }
    async fn run_flush(
        &self,
        _rows: Vec<wardnetd_data::repository::IntradayStatRow>,
    ) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn run_maintenance(&self) -> anyhow::Result<()> {
        unimplemented!()
    }
}

// ---------------------------------------------------------------------------
// Convenience constructor
// ---------------------------------------------------------------------------

/// Create an [`AppState`] with all stub services — useful for tests that only
/// need the router to be constructable (e.g. route reachability tests).
pub fn test_app_state() -> AppState {
    AppState::new(
        Arc::new(StubAuthService),
        Arc::new(StubBackupService),
        Arc::new(StubDeviceService),
        Arc::new(StubDhcpService),
        Arc::new(StubDnsService),
        Arc::new(StubDnsFilterService),
        Arc::new(StubDnsLocalService),
        Arc::new(StubDiscoveryService),
        Arc::new(StubLogService) as Arc<dyn LogService>,
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(StubSystemService),
        Arc::new(StubTunnelService),
        Arc::new(StubUpdateService),
        Arc::new(StubDhcpServer),
        Arc::new(StubDnsServer),
        Arc::new(StubEventPublisher),
        StubJobService::new_arc(),
        Arc::new(StubStatsService),
    )
}
