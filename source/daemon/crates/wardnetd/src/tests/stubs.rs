//! Shared stub service implementations for tests.
//!
//! These stubs satisfy the trait bounds required by [`AppState::new`] but
//! panic with `unimplemented!()` if actually called. Use them when a test
//! only exercises one or two services and the rest are unused fillers.
//!
//! For tests that need real behaviour, define a local `Mock*` struct with
//! custom logic and swap only the service(s) under test.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::broadcast;
use uuid::Uuid;
use wardnet_common::api::{
    CreateTunnelRequest, CreateTunnelResponse, DeleteTunnelResponse, DeviceMeResponse,
    ListCountriesResponse, ListProvidersResponse, ListServersRequest, ListServersResponse,
    ListTunnelsResponse, SetMyRuleResponse, SetupProviderRequest, SetupProviderResponse,
    SystemStatusResponse, ValidateCredentialsRequest, ValidateCredentialsResponse,
};
use wardnet_common::device::{Device, DeviceType};
use wardnet_common::event::WardnetEvent;
use wardnet_common::routing::RoutingTarget;
use wardnet_common::tunnel::Tunnel;

use wardnet_common::api::{
    CreateDhcpReservationRequest, CreateDhcpReservationResponse, DeleteDhcpReservationResponse,
    DhcpConfigResponse, DhcpStatusResponse, ListDhcpLeasesResponse, ListDhcpReservationsResponse,
    RevokeDhcpLeaseResponse, ToggleDhcpRequest, UpdateDhcpConfigRequest,
};
use wardnet_common::dhcp::{DhcpConfig, DhcpLease};
use wardnetd_services::auth::service::LoginResult;
use wardnetd_services::device::packet_capture::ObservedDevice;
use wardnetd_services::error::AppError;
use wardnetd_services::event::EventPublisher;
use wardnetd_services::{
    AuthService, BackupService, DeviceDiscoveryService, DeviceService, DhcpService,
    ObservationResult, RoutingService, SystemService, TunnelService, VpnProviderService,
};

use wardnetd_api::state::AppState;

// ---------------------------------------------------------------------------
// StubAuthService
// ---------------------------------------------------------------------------

/// Stub auth service that does nothing useful.
///
/// `validate_session` and `validate_api_key` return `None` (unauthenticated).
/// All other methods panic with `unimplemented!()`.
pub struct StubAuthService;

pub struct StubBackupService;

#[async_trait]
impl BackupService for StubBackupService {
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

#[async_trait]
impl AuthService for StubAuthService {
    async fn login(&self, _u: &str, _p: &str) -> Result<LoginResult, AppError> {
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
}

// ---------------------------------------------------------------------------
// StubDeviceService
// ---------------------------------------------------------------------------

/// Stub device service — all methods panic with `unimplemented!()`.
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
    async fn update_admin_locked(&self, _id: &str, _locked: bool) -> Result<(), AppError> {
        unimplemented!()
    }
}

// ---------------------------------------------------------------------------
// StubDiscoveryService
// ---------------------------------------------------------------------------

/// Stub discovery service — all methods panic with `unimplemented!()`.
pub struct StubDiscoveryService;

#[async_trait]
impl DeviceDiscoveryService for StubDiscoveryService {
    async fn restore_devices(&self) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn process_observation(
        &self,
        _obs: &ObservedDevice,
    ) -> Result<ObservationResult, AppError> {
        unimplemented!()
    }
    async fn flush_last_seen(&self) -> Result<u64, AppError> {
        unimplemented!()
    }
    async fn scan_departures(&self, _t: u64) -> Result<Vec<Uuid>, AppError> {
        unimplemented!()
    }
    async fn resolve_hostname(&self, _id: Uuid, _ip: String) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn get_all_devices(&self) -> Result<Vec<Device>, AppError> {
        unimplemented!()
    }
    async fn get_device_by_id(&self, _id: Uuid) -> Result<Device, AppError> {
        unimplemented!()
    }
    async fn update_device(
        &self,
        _id: Uuid,
        _n: Option<&str>,
        _dt: Option<DeviceType>,
    ) -> Result<Device, AppError> {
        unimplemented!()
    }
}

// ---------------------------------------------------------------------------
// StubSystemService
// ---------------------------------------------------------------------------

/// Stub system service — all methods panic with `unimplemented!()`.
pub struct StubSystemService;

#[async_trait]
impl SystemService for StubSystemService {
    fn version(&self) -> &'static str {
        "0.0.0-stub"
    }
    fn uptime(&self) -> std::time::Duration {
        std::time::Duration::from_secs(0)
    }
    async fn status(&self) -> Result<SystemStatusResponse, AppError> {
        unimplemented!()
    }
    async fn request_restart(&self) -> Result<(), AppError> {
        unimplemented!()
    }
}

// ---------------------------------------------------------------------------
// StubProviderService
// ---------------------------------------------------------------------------

/// Stub provider service — all methods panic with `unimplemented!()`.
pub struct StubProviderService;

#[async_trait]
impl VpnProviderService for StubProviderService {
    async fn list_providers(&self) -> Result<ListProvidersResponse, AppError> {
        unimplemented!()
    }
    async fn list_countries(&self, _id: &str) -> Result<ListCountriesResponse, AppError> {
        Ok(ListCountriesResponse { countries: vec![] })
    }
    async fn validate_credentials(
        &self,
        _id: &str,
        _req: ValidateCredentialsRequest,
    ) -> Result<ValidateCredentialsResponse, AppError> {
        unimplemented!()
    }
    async fn list_servers(
        &self,
        _id: &str,
        _req: ListServersRequest,
    ) -> Result<ListServersResponse, AppError> {
        unimplemented!()
    }
    async fn setup_tunnel(
        &self,
        _id: &str,
        _req: SetupProviderRequest,
    ) -> Result<SetupProviderResponse, AppError> {
        unimplemented!()
    }
}

// ---------------------------------------------------------------------------
// StubTunnelService
// ---------------------------------------------------------------------------

/// Stub tunnel service — all methods panic with `unimplemented!()`.
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
    async fn bring_up(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn tear_down(&self, _id: Uuid, _r: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn bring_up_internal(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn tear_down_internal(&self, _id: Uuid, _r: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn delete_tunnel(&self, _id: Uuid) -> Result<DeleteTunnelResponse, AppError> {
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
}

// ---------------------------------------------------------------------------
// StubRoutingService
// ---------------------------------------------------------------------------

/// Stub routing service — all methods panic with `unimplemented!()`.
pub struct StubRoutingService;

#[async_trait]
impl RoutingService for StubRoutingService {
    async fn apply_rule(
        &self,
        _device_id: Uuid,
        _device_ip: &str,
        _target: &RoutingTarget,
    ) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn remove_device_routes(
        &self,
        _device_id: Uuid,
        _device_ip: &str,
    ) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn handle_ip_change(
        &self,
        _device_id: Uuid,
        _old_ip: &str,
        _new_ip: &str,
    ) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn handle_tunnel_down(&self, _tunnel_id: Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn handle_tunnel_up(&self, _tunnel_id: Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn reconcile(&self) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn handle_route_table_lost(&self, _table: u32) -> Result<(), AppError> {
        Ok(())
    }
    async fn devices_using_tunnel(&self, _tunnel_id: Uuid) -> Result<Vec<Uuid>, AppError> {
        unimplemented!()
    }
    async fn apply_rule_for_device(
        &self,
        _device_id: Uuid,
        _target: &RoutingTarget,
    ) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn apply_rule_for_discovered_device(
        &self,
        _device_id: Uuid,
        _ip: &str,
    ) -> Result<(), AppError> {
        unimplemented!()
    }
}

// ---------------------------------------------------------------------------
// StubDhcpService
// ---------------------------------------------------------------------------

/// Stub DHCP service — all methods panic with `unimplemented!()`.
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
    ) -> Result<DhcpLease, AppError> {
        unimplemented!()
    }
    async fn renew_lease(&self, _mac: &str) -> Result<DhcpLease, AppError> {
        unimplemented!()
    }
    async fn release_lease(&self, _mac: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn cleanup_expired(&self) -> Result<u64, AppError> {
        unimplemented!()
    }
    async fn get_dhcp_config(&self) -> Result<DhcpConfig, AppError> {
        unimplemented!()
    }
}

// ---------------------------------------------------------------------------
// StubDnsService
// ---------------------------------------------------------------------------

/// Stub DNS service — all methods panic with `unimplemented!()`.
pub struct StubDnsService;

#[async_trait]
impl wardnetd_services::dns::DnsService for StubDnsService {
    async fn get_config(&self) -> Result<wardnet_common::api::DnsConfigResponse, AppError> {
        unimplemented!()
    }
    async fn update_config(
        &self,
        _req: wardnet_common::api::UpdateDnsConfigRequest,
    ) -> Result<wardnet_common::api::DnsConfigResponse, AppError> {
        unimplemented!()
    }
    async fn toggle(
        &self,
        _req: wardnet_common::api::ToggleDnsRequest,
    ) -> Result<wardnet_common::api::DnsConfigResponse, AppError> {
        unimplemented!()
    }
    async fn status(&self) -> Result<wardnet_common::api::DnsStatusResponse, AppError> {
        unimplemented!()
    }
    async fn flush_cache(&self) -> Result<wardnet_common::api::DnsCacheFlushResponse, AppError> {
        unimplemented!()
    }
    async fn get_dns_config(&self) -> Result<wardnet_common::dns::DnsConfig, AppError> {
        unimplemented!()
    }
    async fn list_blocklists(
        &self,
    ) -> Result<wardnet_common::api::ListBlocklistsResponse, AppError> {
        unimplemented!()
    }
    async fn create_blocklist(
        &self,
        _req: wardnet_common::api::CreateBlocklistRequest,
    ) -> Result<wardnet_common::api::CreateBlocklistResponse, AppError> {
        unimplemented!()
    }
    async fn update_blocklist(
        &self,
        _id: uuid::Uuid,
        _req: wardnet_common::api::UpdateBlocklistRequest,
    ) -> Result<wardnet_common::api::UpdateBlocklistResponse, AppError> {
        unimplemented!()
    }
    async fn delete_blocklist(
        &self,
        _id: uuid::Uuid,
    ) -> Result<wardnet_common::api::DeleteBlocklistResponse, AppError> {
        unimplemented!()
    }
    async fn update_blocklist_now(
        &self,
        _id: uuid::Uuid,
    ) -> Result<wardnet_common::jobs::JobDispatchedResponse, AppError> {
        unimplemented!()
    }
    async fn list_allowlist(&self) -> Result<wardnet_common::api::ListAllowlistResponse, AppError> {
        unimplemented!()
    }
    async fn create_allowlist_entry(
        &self,
        _req: wardnet_common::api::CreateAllowlistRequest,
    ) -> Result<wardnet_common::api::CreateAllowlistResponse, AppError> {
        unimplemented!()
    }
    async fn delete_allowlist_entry(
        &self,
        _id: uuid::Uuid,
    ) -> Result<wardnet_common::api::DeleteAllowlistResponse, AppError> {
        unimplemented!()
    }
    async fn list_filter_rules(
        &self,
    ) -> Result<wardnet_common::api::ListFilterRulesResponse, AppError> {
        unimplemented!()
    }
    async fn create_filter_rule(
        &self,
        _req: wardnet_common::api::CreateFilterRuleRequest,
    ) -> Result<wardnet_common::api::CreateFilterRuleResponse, AppError> {
        unimplemented!()
    }
    async fn update_filter_rule(
        &self,
        _id: uuid::Uuid,
        _req: wardnet_common::api::UpdateFilterRuleRequest,
    ) -> Result<wardnet_common::api::UpdateFilterRuleResponse, AppError> {
        unimplemented!()
    }
    async fn delete_filter_rule(
        &self,
        _id: uuid::Uuid,
    ) -> Result<wardnet_common::api::DeleteFilterRuleResponse, AppError> {
        unimplemented!()
    }
    async fn load_filter_inputs(
        &self,
    ) -> Result<wardnetd_services::dns::filter::FilterInputs, AppError> {
        unimplemented!()
    }
}

// ---------------------------------------------------------------------------
// StubLogService
// ---------------------------------------------------------------------------

/// Stub log service — satisfies trait bounds without real behaviour.
pub struct StubLogService;

#[async_trait]
impl wardnetd_services::logging::LogService for StubLogService {
    fn subscribe(&self) -> broadcast::Receiver<wardnetd_services::logging::stream::LogEntry> {
        let (tx, rx) = broadcast::channel(1);
        drop(tx);
        rx
    }
    fn get_recent_errors(&self) -> Vec<wardnetd_services::logging::error_notifier::ErrorEntry> {
        Vec::new()
    }
    async fn list_log_files(
        &self,
    ) -> Result<Vec<wardnetd_services::logging::service::LogFileInfo>, AppError> {
        Ok(Vec::new())
    }
    async fn download_log_file(&self, _name: Option<&str>) -> Result<String, AppError> {
        Ok(String::new())
    }
    fn tracing_layers(&self) -> Vec<wardnetd_services::logging::BoxedLayer> {
        Vec::new()
    }
    fn start_all(&self) {}
    fn stop_all(&self) {}
}

// ---------------------------------------------------------------------------
// StubEventPublisher
// ---------------------------------------------------------------------------

/// Stub event publisher — `publish` is a no-op, `subscribe` returns a dead receiver.
pub struct StubEventPublisher;

impl EventPublisher for StubEventPublisher {
    fn publish(&self, _event: WardnetEvent) {}
    fn subscribe(&self) -> broadcast::Receiver<WardnetEvent> {
        let (tx, rx) = broadcast::channel(1);
        drop(tx);
        rx
    }
}

// ---------------------------------------------------------------------------
// StubUpdateService
// ---------------------------------------------------------------------------

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
// StubDhcpServer
// ---------------------------------------------------------------------------

/// Stub DHCP server — all methods return Ok/false.
pub struct StubDhcpServer;

#[async_trait]
impl wardnetd_services::dhcp::server::DhcpServer for StubDhcpServer {
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

// ---------------------------------------------------------------------------
// StubDnsServer
// ---------------------------------------------------------------------------

/// Stub DNS server — all methods return Ok/false/0.
pub struct StubDnsServer;

#[async_trait]
impl wardnetd_services::dns::server::DnsServer for StubDnsServer {
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

// ---------------------------------------------------------------------------
// Helper: test_app_state
// ---------------------------------------------------------------------------

/// Create an [`AppState`] with all stub services and default configuration.
///
/// Useful as a starting point for tests that only need one or two real
/// services. Tests requiring custom services should construct `AppState::new()`
/// directly, replacing the stub(s) they care about.
#[allow(dead_code)]
pub fn test_app_state() -> AppState {
    AppState::new(
        Arc::new(StubAuthService),
        Arc::new(StubBackupService),
        Arc::new(StubDeviceService),
        Arc::new(StubDhcpService),
        Arc::new(StubDnsService),
        Arc::new(StubDiscoveryService),
        Arc::new(StubLogService),
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(StubSystemService),
        Arc::new(StubTunnelService),
        Arc::new(StubUpdateService),
        Arc::new(StubDhcpServer),
        Arc::new(StubDnsServer),
        Arc::new(StubEventPublisher),
        wardnetd_services::jobs::JobServiceImpl::new(),
    )
}
