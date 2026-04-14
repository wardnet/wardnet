//! Shared stub service implementations for tests.
//!
//! These stubs satisfy the trait bounds required by [`AppState::new`] but
//! panic with `unimplemented!()` if actually called. Use them when a test
//! only exercises one or two services and the rest are unused fillers.
//!
//! For tests that need real behaviour, define a local `Mock*` struct with
//! custom logic and swap only the service(s) under test.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use tokio::sync::broadcast;
use uuid::Uuid;
use wardnet_types::api::{
    CreateTunnelRequest, CreateTunnelResponse, DeleteTunnelResponse, DeviceMeResponse,
    ListCountriesResponse, ListProvidersResponse, ListServersRequest, ListServersResponse,
    ListTunnelsResponse, SetMyRuleResponse, SetupProviderRequest, SetupProviderResponse,
    SystemStatusResponse, ValidateCredentialsRequest, ValidateCredentialsResponse,
};
use wardnet_types::device::{Device, DeviceType};
use wardnet_types::event::WardnetEvent;
use wardnet_types::routing::RoutingTarget;
use wardnet_types::tunnel::Tunnel;

use crate::config::Config;
use crate::error::AppError;
use crate::event::EventPublisher;
use crate::packet_capture::ObservedDevice;
use crate::service::auth::LoginResult;
use crate::service::{
    AuthService, DeviceDiscoveryService, DeviceService, DhcpService, ObservationResult,
    ProviderService, RoutingService, SystemService, TunnelService,
};
use crate::state::AppState;
use wardnet_types::api::{
    CreateDhcpReservationRequest, CreateDhcpReservationResponse, DeleteDhcpReservationResponse,
    DhcpConfigResponse, DhcpStatusResponse, ListDhcpLeasesResponse, ListDhcpReservationsResponse,
    RevokeDhcpLeaseResponse, ToggleDhcpRequest, UpdateDhcpConfigRequest,
};
use wardnet_types::dhcp::{DhcpConfig, DhcpLease};

// ---------------------------------------------------------------------------
// StubAuthService
// ---------------------------------------------------------------------------

/// Stub auth service that does nothing useful.
///
/// `validate_session` and `validate_api_key` return `None` (unauthenticated).
/// All other methods panic with `unimplemented!()`.
pub struct StubAuthService;

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
    async fn status(&self) -> Result<SystemStatusResponse, AppError> {
        unimplemented!()
    }
}

// ---------------------------------------------------------------------------
// StubProviderService
// ---------------------------------------------------------------------------

/// Stub provider service — all methods panic with `unimplemented!()`.
pub struct StubProviderService;

#[async_trait]
impl ProviderService for StubProviderService {
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
        // Return empty — several API handlers enrich their responses with the
        // tunnel list, and tests that don't care about tunnels would otherwise
        // panic. Tests that need specific tunnels plug in their own mock.
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
    async fn devices_using_tunnel(&self, _tunnel_id: Uuid) -> Result<Vec<Uuid>, AppError> {
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
        Arc::new(StubDeviceService),
        Arc::new(StubDhcpService),
        Arc::new(StubDiscoveryService),
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(StubSystemService),
        Arc::new(StubTunnelService),
        Arc::new(crate::dhcp::server::NoopDhcpServer),
        Arc::new(StubEventPublisher),
        crate::log_broadcast::LogBroadcaster::new(16),
        crate::recent_errors::RecentErrors::new(),
        Config::default(),
        Instant::now(),
    )
}
