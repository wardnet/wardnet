//! Tests for the device API endpoints (GET /api/devices/me, PUT /api/devices/me/rule,
//! GET /api/devices, GET /api/devices/:id, PUT /api/devices/:id).

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::routing::{get, put};
use tower::ServiceExt;
use wardnet_common::api::{DeviceMeResponse, SetMyRuleResponse};
use wardnet_common::device::{Device, DeviceSignal, DeviceSignalKind, DeviceType};
use wardnet_common::routing::RoutingTarget;

use crate::state::AppState;
use crate::tests::stubs::{
    StubDhcpServer, StubDhcpService, StubDnsFilterService, StubDnsLocalService, StubDnsServer,
    StubDnsService, StubEventPublisher, StubLogService, StubNetworkZoneService,
    StubProviderService, StubRoutingService, StubSystemService, StubTunnelService,
};
use uuid::Uuid;
use wardnet_common::auth::{AuthenticatedUser, UserRole};
use wardnet_test_support::principal;
use wardnetd_services::LogService;
use wardnetd_services::auth::service::LoginResult;
use wardnetd_services::auth::{CurrentUser, LoginAttempt};
use wardnetd_services::device::identification::{DeviceIdentificationService, ProbeOutcome};
use wardnetd_services::error::AppError;
use wardnetd_services::{
    AuthService, DeviceDiscoveryService, DeviceService, DhcpService, TunnelService,
};

// ---------------------------------------------------------------------------
// Mock services
// ---------------------------------------------------------------------------

/// Mock `AuthService` that always validates sessions.
struct MockAuthService;

#[async_trait]
impl AuthService for MockAuthService {
    async fn issue_verified_session(
        &self,
        _user_id: uuid::Uuid,
        _remember_me: bool,
        _user_agent: Option<&str>,
    ) -> Result<LoginResult, AppError> {
        unimplemented!()
    }
    async fn current_user(&self) -> Result<CurrentUser, AppError> {
        Ok(CurrentUser {
            user_id: Uuid::nil(),
            display_name: "admin".to_owned(),
            email: None,
            role: UserRole::Admin,
        })
    }
    async fn login(&self, _attempt: LoginAttempt<'_>) -> Result<LoginResult, AppError> {
        Ok(LoginResult {
            token: "t".to_owned(),
            max_age_seconds: 3600,
        })
    }
    async fn validate_session(&self, _token: &str) -> Result<Option<AuthenticatedUser>, AppError> {
        Ok(Some(principal::admin(
            Uuid::parse_str("00000000-0000-0000-0000-000000000099").unwrap(),
        )))
    }
    async fn validate_api_key(&self, _key: &str) -> Result<Option<AuthenticatedUser>, AppError> {
        Ok(None)
    }
    async fn setup_admin(&self, _u: &str, _p: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn is_setup_completed(&self) -> Result<bool, AppError> {
        unimplemented!()
    }
    async fn wizard_state(
        &self,
    ) -> Result<wardnetd_services::auth::service::WizardState, AppError> {
        unimplemented!()
    }
    async fn advance_wizard(
        &self,
        _to_step: wardnet_common::api::WizardStep,
        _mode: Option<wardnet_common::api::WizardMode>,
    ) -> Result<wardnetd_services::auth::service::WizardState, AppError> {
        unimplemented!()
    }
    async fn logout_session(&self, _token: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn refresh_session(&self, _token: &str) -> Result<LoginResult, AppError> {
        unimplemented!()
    }
    async fn cleanup_expired_sessions(&self) -> Result<u64, AppError> {
        unimplemented!()
    }
}

/// Mock `DeviceService` returning configurable responses.
/// Ordered record of the service calls a request made, shared between the
/// device and discovery mocks so a test can assert *sequence*, not just
/// occurrence. The release handler's correctness is an ordering property —
/// `clear_managed` must be last — so occurrence alone would not catch the bug
/// that matters (issue #1181).
type AuditLog = Arc<std::sync::Mutex<Vec<&'static str>>>;

struct MockDeviceService {
    device: Option<Device>,
    rule: Option<RoutingTarget>,
    admin_locked: bool,
    set_rule_error: Option<String>,
    /// Batched routing rules returned by `current_rules` (used by the list path).
    current_rules: std::collections::HashMap<Uuid, RoutingTarget>,
    audit: AuditLog,
    /// When set, `update_admin_locked` fails — standing in for any mid-sequence
    /// step of the release blowing up.
    fail_admin_locked: bool,
}

impl MockDeviceService {
    fn found(device: Device, rule: Option<RoutingTarget>) -> Self {
        let admin_locked = device.admin_locked;
        Self {
            device: Some(device),
            rule,
            admin_locked,
            set_rule_error: None,
            current_rules: std::collections::HashMap::new(),
            audit: AuditLog::default(),
            fail_admin_locked: false,
        }
    }

    fn not_found() -> Self {
        Self {
            device: None,
            rule: None,
            admin_locked: false,
            set_rule_error: Some("not_found".to_owned()),
            current_rules: std::collections::HashMap::new(),
            audit: AuditLog::default(),
            fail_admin_locked: false,
        }
    }

    fn forbidden(device: Device) -> Self {
        Self {
            device: Some(device),
            rule: None,
            admin_locked: true,
            set_rule_error: Some("forbidden".to_owned()),
            current_rules: std::collections::HashMap::new(),
            audit: AuditLog::default(),
            fail_admin_locked: false,
        }
    }

    /// Seed the batched `current_rules` map returned to the list handler.
    fn with_current_rules(mut self, rules: std::collections::HashMap<Uuid, RoutingTarget>) -> Self {
        self.current_rules = rules;
        self
    }
}

/// Mock `TunnelService` that returns a fixed list of tunnels for `list_tunnels`.
struct MockTunnelService {
    tunnels: Vec<wardnet_common::tunnel::Tunnel>,
}

#[async_trait]
impl TunnelService for MockTunnelService {
    async fn import_tunnel(
        &self,
        _r: wardnet_common::api::CreateTunnelRequest,
    ) -> Result<wardnet_common::api::CreateTunnelResponse, AppError> {
        unimplemented!()
    }
    async fn list_tunnels(&self) -> Result<wardnet_common::api::ListTunnelsResponse, AppError> {
        Ok(wardnet_common::api::ListTunnelsResponse {
            tunnels: self.tunnels.clone(),
        })
    }
    async fn get_tunnel(&self, _id: Uuid) -> Result<wardnet_common::tunnel::Tunnel, AppError> {
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
    async fn delete_tunnel(
        &self,
        _id: Uuid,
    ) -> Result<wardnet_common::api::DeleteTunnelResponse, AppError> {
        unimplemented!()
    }
    async fn bring_up_internal(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn tear_down_internal(&self, _id: Uuid, _reason: &str) -> Result<(), AppError> {
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
        _id: uuid::Uuid,
    ) -> Result<wardnet_common::jobs::JobDispatchedResponse, AppError> {
        unimplemented!()
    }
    async fn list_speed_tests(
        &self,
        _id: uuid::Uuid,
    ) -> Result<wardnet_common::speed_test::TunnelSpeedTestHistoryResponse, AppError> {
        unimplemented!()
    }
}

#[async_trait]
impl DeviceService for MockDeviceService {
    async fn clear_rule(&self, _device_id: &str) -> Result<(), AppError> {
        self.audit.lock().unwrap().push("clear_rule");
        Ok(())
    }

    async fn mark_managed(&self, _device_id: &str) -> Result<(), AppError> {
        self.audit.lock().unwrap().push("mark_managed");
        Ok(())
    }

    async fn clear_managed(&self, _device_id: &str) -> Result<(), AppError> {
        self.audit.lock().unwrap().push("clear_managed");
        Ok(())
    }

    async fn set_device_owner(
        &self,
        _device_id: &str,
        _owner_user_id: Option<uuid::Uuid>,
    ) -> Result<(), AppError> {
        unimplemented!()
    }

    async fn get_device(
        &self,
        _device_id: &str,
    ) -> Result<Option<wardnet_common::device::Device>, AppError> {
        Ok(self.device.clone())
    }
    async fn clear_remote_connection_mode(&self, _device_id: &str) -> Result<(), AppError> {
        Ok(())
    }
    async fn get_device_for_ip(&self, _ip: &str) -> Result<DeviceMeResponse, AppError> {
        Ok(DeviceMeResponse {
            device: self.device.clone(),
            current_rule: self.rule.clone(),
            admin_locked: self.admin_locked,
            available_tunnels: vec![],
            zone: None,
            routing_profiles: vec![],
        })
    }

    async fn set_rule_for_ip(
        &self,
        _ip: &str,
        target: RoutingTarget,
    ) -> Result<SetMyRuleResponse, AppError> {
        match self.set_rule_error.as_deref() {
            Some("not_found") => Err(AppError::NotFound(
                "device not found for this IP".to_owned(),
            )),
            Some("forbidden") => Err(AppError::Forbidden("routing is locked by admin".to_owned())),
            _ => Ok(SetMyRuleResponse {
                message: "routing rule updated".to_owned(),
                target,
            }),
        }
    }

    async fn set_rule(&self, _id: &str, _t: RoutingTarget) -> Result<(), AppError> {
        Ok(())
    }

    async fn current_rules(
        &self,
    ) -> Result<std::collections::HashMap<Uuid, RoutingTarget>, AppError> {
        Ok(self.current_rules.clone())
    }

    async fn get_rule_for_device(
        &self,
        _device_id: &str,
    ) -> Result<Option<RoutingTarget>, AppError> {
        Ok(self.rule.clone())
    }

    async fn update_admin_locked(&self, _id: &str, _locked: bool) -> Result<(), AppError> {
        self.audit.lock().unwrap().push("update_admin_locked");
        if self.fail_admin_locked {
            return Err(AppError::Internal(anyhow::anyhow!(
                "synthetic lock failure"
            )));
        }
        Ok(())
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
        self.audit
            .lock()
            .unwrap()
            .push("update_dns_capture_settings");
        Ok(())
    }
    async fn set_my_capture_enabled(
        &self,
        _ip: &str,
        _enabled: bool,
    ) -> Result<wardnet_common::api::DnsCaptureSettingsResponse, AppError> {
        unimplemented!()
    }
    async fn fetch_pending_dns_events(
        &self,
        _device_id: &str,
        _after_id: i64,
        _limit: i64,
    ) -> Result<Vec<wardnet_common::api::DnsEventItem>, AppError> {
        unimplemented!()
    }
    async fn ack_dns_events(&self, _device_id: &str, _up_to_id: i64) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn list_capture_enabled_device_ids(&self) -> Result<Vec<String>, AppError> {
        unimplemented!()
    }
    async fn get_device_capture_settings(
        &self,
        _device_id: &str,
    ) -> Result<Option<(bool, i64, i64)>, AppError> {
        unimplemented!()
    }
}

/// Mock `DhcpService` that returns configurable leases and reservations.
///
/// All mutating methods delegate to `StubDhcpService` (i.e. panic).
/// Only the read methods used by the device list enrichment are implemented.
struct MockDhcpService {
    leases: Vec<wardnet_common::dhcp::DhcpLease>,
    reservations: Vec<wardnet_common::dhcp::DhcpReservation>,
    audit: AuditLog,
}

impl MockDhcpService {
    fn empty() -> Self {
        Self {
            leases: vec![],
            reservations: vec![],
            audit: AuditLog::default(),
        }
    }

    /// One reservation pinned to `mac`, so the release's MAC-keyed lookup has
    /// something to find (#1181).
    fn with_reservation_for(mac: &str, audit: AuditLog) -> Self {
        let now = chrono::Utc::now();
        Self {
            leases: vec![],
            reservations: vec![wardnet_common::dhcp::DhcpReservation {
                id: Uuid::parse_str("00000000-0000-0000-0000-0000000000d1").unwrap(),
                // Deliberately uppercase: the handler matches case-insensitively
                // because reservation MACs are not guaranteed canonical.
                mac_address: mac.to_uppercase(),
                ip_address: "192.168.1.42".parse().unwrap(),
                hostname: None,
                description: None,
                created_at: now,
                updated_at: now,
            }],
            audit,
        }
    }
}

#[async_trait]
impl DhcpService for MockDhcpService {
    async fn get_config(&self) -> Result<wardnet_common::api::DhcpConfigResponse, AppError> {
        unimplemented!()
    }
    async fn update_config(
        &self,
        _r: wardnet_common::api::UpdateDhcpConfigRequest,
    ) -> Result<wardnet_common::api::DhcpConfigResponse, AppError> {
        unimplemented!()
    }
    async fn preview_config(
        &self,
        _req: wardnet_common::api::PreviewDhcpConfigRequest,
    ) -> Result<wardnet_common::api::PreviewDhcpConfigResponse, AppError> {
        Ok(wardnet_common::api::PreviewDhcpConfigResponse {
            affected: Vec::new(),
        })
    }
    async fn toggle(
        &self,
        _r: wardnet_common::api::ToggleDhcpRequest,
    ) -> Result<wardnet_common::api::DhcpConfigResponse, AppError> {
        unimplemented!()
    }
    async fn list_leases(&self) -> Result<wardnet_common::api::ListDhcpLeasesResponse, AppError> {
        Ok(wardnet_common::api::ListDhcpLeasesResponse {
            leases: self.leases.clone(),
        })
    }
    async fn revoke_lease(
        &self,
        _id: Uuid,
    ) -> Result<wardnet_common::api::RevokeDhcpLeaseResponse, AppError> {
        unimplemented!()
    }
    async fn list_reservations(
        &self,
    ) -> Result<wardnet_common::api::ListDhcpReservationsResponse, AppError> {
        Ok(wardnet_common::api::ListDhcpReservationsResponse {
            reservations: self.reservations.clone(),
        })
    }
    async fn create_reservation(
        &self,
        _r: wardnet_common::api::CreateDhcpReservationRequest,
    ) -> Result<wardnet_common::api::CreateDhcpReservationResponse, AppError> {
        unimplemented!()
    }
    async fn delete_reservation(
        &self,
        _id: Uuid,
    ) -> Result<wardnet_common::api::DeleteDhcpReservationResponse, AppError> {
        self.audit.lock().unwrap().push("delete_reservation");
        Ok(wardnet_common::api::DeleteDhcpReservationResponse {
            message: "reservation deleted".to_owned(),
        })
    }
    async fn status(&self) -> Result<wardnet_common::api::DhcpStatusResponse, AppError> {
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
    async fn scope_for_mac(&self, _mac: &str) -> Result<wardnet_common::dhcp::DhcpScope, AppError> {
        unimplemented!()
    }
}

/// Mock `DeviceDiscoveryService` for admin device endpoints.
struct MockDiscoveryService {
    devices: Vec<Device>,
    audit: AuditLog,
}

#[async_trait]
impl DeviceDiscoveryService for MockDiscoveryService {
    async fn prune_unmanaged_devices(&self) -> Result<u64, wardnetd_services::error::AppError> {
        Ok(0)
    }

    async fn process_peer_observation(
        &self,
        _device_id: uuid::Uuid,
        _ip: &str,
    ) -> Result<wardnetd_services::ObservationResult, AppError> {
        unimplemented!()
    }
    async fn touch_peer_presence(&self, _device_id: uuid::Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn mark_peer_gone(
        &self,
        _device_id: uuid::Uuid,
        _timeout: std::time::Duration,
    ) -> Result<Option<uuid::Uuid>, AppError> {
        unimplemented!()
    }
    async fn restore_devices(&self) -> Result<(), AppError> {
        Ok(())
    }
    async fn rebuild_trusted_subnets(&self) -> Result<(), AppError> {
        Ok(())
    }
    async fn process_observation(
        &self,
        _obs: &wardnetd_services::device::packet_capture::ObservedDevice,
    ) -> Result<wardnetd_services::device::ObservationResult, AppError> {
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
        Ok(self.devices.clone())
    }
    async fn get_device_by_id(&self, id: Uuid) -> Result<Device, AppError> {
        self.devices
            .iter()
            .find(|d| d.id == id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("device {id} not found")))
    }
    async fn update_device(
        &self,
        id: Uuid,
        name: Option<&str>,
        _device_type: Option<DeviceType>,
    ) -> Result<Device, AppError> {
        self.audit.lock().unwrap().push("update_device");
        let mut device = self.get_device_by_id(id).await?;
        // Mirrors the service: a blank name clears it to `None` rather than
        // storing an empty string (#1181).
        device.name = name
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(str::to_owned)
            .or(if name.is_some() { None } else { device.name });
        Ok(device)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn sample_device() -> Device {
    Device {
        id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
        mac: "aa:bb:cc:dd:ee:01".to_owned(),
        name: Some("My Phone".to_owned()),
        hostname: None,
        manufacturer: Some("Apple".to_owned()),
        manufacturer_source: None,
        is_randomized: false,
        device_type: DeviceType::Phone,
        first_seen: "2026-03-07T00:00:00Z".parse().unwrap(),
        last_seen: "2026-03-07T00:00:00Z".parse().unwrap(),
        last_ip: "192.168.1.10".to_owned(),
        admin_locked: false,
        zone_id: "00000000-0000-0000-0000-000000000201".parse().unwrap(),
        owner_user_id: None,
        dns_capture_enabled: false,
        dns_capture_cap_count: 1000,
        dns_capture_cap_days: 7,
        connection_mode: wardnet_common::device::DeviceConnectionMode::Lan,
        managed: false,
    }
}

fn connect_info() -> ConnectInfo<SocketAddr> {
    ConnectInfo(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10)),
        12345,
    ))
}

fn build_state(
    device_svc: impl DeviceService + 'static,
    discovery_svc: impl DeviceDiscoveryService + 'static,
) -> AppState {
    build_state_with_dhcp(device_svc, discovery_svc, StubDhcpService)
}

fn build_state_with_dhcp(
    device_svc: impl DeviceService + 'static,
    discovery_svc: impl DeviceDiscoveryService + 'static,
    dhcp_svc: impl DhcpService + 'static,
) -> AppState {
    AppState::new(
        Arc::new(MockAuthService),
        Arc::new(crate::tests::stubs::StubBackupService),
        Arc::new(device_svc),
        Arc::new(dhcp_svc),
        Arc::new(StubDnsService),
        Arc::new(StubDnsFilterService),
        Arc::new(StubDnsLocalService),
        Arc::new(crate::tests::stubs::StubDdnsService),
        Arc::new(crate::tests::stubs::StubTlsService),
        Arc::new(discovery_svc),
        Arc::new(StubLogService) as Arc<dyn LogService>,
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(StubNetworkZoneService),
        Arc::new(StubSystemService),
        Arc::new(StubTunnelService),
        Arc::new(crate::tests::stubs::StubUpdateService),
        Arc::new(StubDhcpServer),
        Arc::new(StubDnsServer),
        Arc::new(StubEventPublisher),
        crate::tests::stubs::StubJobService::new_arc(),
        Arc::new(crate::tests::stubs::StubStatsService),
        Arc::new(crate::tests::stubs::StubAccessRequestService),
        Arc::new(crate::tests::stubs::StubZoneExceptionService),
    )
}

fn build_state_with_tunnel_svc(
    device_svc: impl DeviceService + 'static,
    discovery_svc: impl DeviceDiscoveryService + 'static,
    tunnel_svc: impl TunnelService + 'static,
) -> AppState {
    AppState::new(
        Arc::new(MockAuthService),
        Arc::new(crate::tests::stubs::StubBackupService),
        Arc::new(device_svc),
        Arc::new(StubDhcpService),
        Arc::new(StubDnsService),
        Arc::new(StubDnsFilterService),
        Arc::new(StubDnsLocalService),
        Arc::new(crate::tests::stubs::StubDdnsService),
        Arc::new(crate::tests::stubs::StubTlsService),
        Arc::new(discovery_svc),
        Arc::new(StubLogService) as Arc<dyn LogService>,
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(StubNetworkZoneService),
        Arc::new(StubSystemService),
        Arc::new(tunnel_svc),
        Arc::new(crate::tests::stubs::StubUpdateService),
        Arc::new(StubDhcpServer),
        Arc::new(StubDnsServer),
        Arc::new(StubEventPublisher),
        crate::tests::stubs::StubJobService::new_arc(),
        Arc::new(crate::tests::stubs::StubStatsService),
        Arc::new(crate::tests::stubs::StubAccessRequestService),
        Arc::new(crate::tests::stubs::StubZoneExceptionService),
    )
}

/// Mock [`DeviceIdentificationService`] whose `signals_for` returns whatever
/// the test configured — a fixed list, or an error standing in for a database
/// failure on the signals table.
struct MockIdentificationService {
    signals_for: Result<Vec<DeviceSignal>, &'static str>,
    /// What `probe_device` returns. `Err` stands in for the daemon's
    /// online-only refusal (issue #1116).
    probe: Result<ProbeOutcome, &'static str>,
}

impl MockIdentificationService {
    fn returning(signals: Vec<DeviceSignal>) -> Self {
        Self {
            signals_for: Ok(signals),
            probe: Ok(ProbeOutcome {
                ports_probed: vec![4003, 6668],
                answering_ports: vec![6668],
            }),
        }
    }

    fn failing() -> Self {
        Self {
            signals_for: Err("signals table is gone"),
            probe: Ok(ProbeOutcome {
                ports_probed: Vec::new(),
                answering_ports: Vec::new(),
            }),
        }
    }

    /// A service that refuses every probe, as the daemon does for a device
    /// that has left the network.
    fn refusing_probe() -> Self {
        Self {
            signals_for: Ok(Vec::new()),
            probe: Err("device is not currently on the network"),
        }
    }
}

#[async_trait]
impl DeviceIdentificationService for MockIdentificationService {
    async fn record_signal(
        &self,
        _device_id: &str,
        _kind: DeviceSignalKind,
        _value: &str,
    ) -> Result<(), AppError> {
        Ok(())
    }
    async fn record_signal_for_mac(
        &self,
        _mac: &str,
        _kind: DeviceSignalKind,
        _value: &str,
    ) -> Result<(), AppError> {
        Ok(())
    }
    async fn record_signal_for_ip(
        &self,
        _ip: std::net::IpAddr,
        _kind: DeviceSignalKind,
        _value: &str,
    ) -> Result<(), AppError> {
        Ok(())
    }
    async fn signals_for(&self, _device_id: &str) -> Result<Vec<DeviceSignal>, AppError> {
        self.signals_for
            .clone()
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))
    }
    async fn reconcile_from_catalog(&self) -> Result<usize, AppError> {
        Ok(0)
    }
    async fn probe_device(&self, _device_id: &str) -> Result<ProbeOutcome, AppError> {
        self.probe
            .clone()
            .map_err(|e| AppError::Conflict(e.to_owned()))
    }
}

/// Build a state whose device detail carries identification signals.
fn build_state_with_identification(
    device: Device,
    identification: impl DeviceIdentificationService + 'static,
) -> AppState {
    build_state_with_dhcp(
        MockDeviceService::found(device.clone(), Some(RoutingTarget::Direct)),
        MockDiscoveryService {
            devices: vec![device],
            audit: AuditLog::default(),
        },
        MockDhcpService::empty(),
    )
    .with_device_identification_service(Arc::new(identification))
}

fn device_router(state: AppState) -> Router {
    Router::new()
        .route("/api/devices/me", get(crate::api::devices::get_me))
        .route(
            "/api/devices/me/rule",
            put(crate::api::devices::set_my_rule),
        )
        .route("/api/devices", get(crate::api::devices::list_devices))
        .route(
            "/api/devices/{id}",
            get(crate::api::devices::get_device).put(crate::api::devices::update_device),
        )
        .route(
            "/api/devices/{id}/zone",
            put(crate::api::devices::assign_device_zone),
        )
        .route(
            "/api/devices/{id}/release",
            axum::routing::post(crate::api::devices::release_device),
        )
        .route(
            "/api/devices/{id}/identify",
            axum::routing::post(crate::api::devices::identify_device),
        )
        .with_state(state)
}

/// Send an authenticated GET request with `ConnectInfo` extension.
async fn get_json(app: Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let resp = app
        .oneshot(
            Request::builder()
                .uri(uri)
                .header("Cookie", "wardnet_session=valid-token")
                .extension(connect_info())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let status = resp.status();
    let body = axum::body::to_bytes(resp.into_body(), 16384).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    (status, json)
}

/// Send an authenticated PUT request with JSON body and `ConnectInfo` extension.
async fn put_json(app: Router, uri: &str, json_body: &str) -> (StatusCode, serde_json::Value) {
    let resp = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(uri)
                .header("Content-Type", "application/json")
                .header("Cookie", "wardnet_session=valid-token")
                .extension(connect_info())
                .body(Body::from(json_body.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = resp.status();
    let body = axum::body::to_bytes(resp.into_body(), 16384).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    (status, json)
}

/// Send an authenticated POST with no body.
async fn post_json(app: Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("Cookie", "wardnet_session=valid-token")
                .extension(connect_info())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let status = resp.status();
    let body = axum::body::to_bytes(resp.into_body(), 16384).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    (status, json)
}

// ---------------------------------------------------------------------------
// GET /api/devices/me
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_me_returns_device_when_found() {
    let device = sample_device();
    let state = build_state(
        MockDeviceService::found(device, Some(RoutingTarget::Direct)),
        MockDiscoveryService {
            devices: vec![],
            audit: AuditLog::default(),
        },
    );
    let app = device_router(state);

    let (status, json) = get_json(app, "/api/devices/me").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["device"]["mac"], "aa:bb:cc:dd:ee:01");
    assert_eq!(json["current_rule"]["type"], "direct");
    assert_eq!(json["admin_locked"], false);
    // The handler enriches the response with the caller's own zone (resolved
    // under an internal admin context) for the read-only user-PWA display.
    assert_eq!(json["zone"]["name"], "Trusted");
    assert_eq!(json["zone"]["is_default"], false);
}

#[tokio::test]
async fn get_me_returns_null_device_when_unknown_ip() {
    let state = build_state(
        MockDeviceService::not_found(),
        MockDiscoveryService {
            devices: vec![],
            audit: AuditLog::default(),
        },
    );
    let app = device_router(state);

    let (status, json) = get_json(app, "/api/devices/me").await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["device"].is_null());
    assert!(json["current_rule"].is_null());
}

#[tokio::test]
async fn get_me_includes_assigned_routing_profiles() {
    use wardnetd_data::repository::{
        RoutingProfileRepository, RoutingProfileRow, SqliteRoutingProfileRepository,
    };
    use wardnetd_services::routing_profile::RoutingProfileServiceImpl;

    let device = sample_device();
    // A real routing-profile service over an in-memory DB, with the caller
    // device assigned one profile, so the `get_me` enrichment resolves it.
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../wardnetd-data/migrations")
        .run(&pool)
        .await
        .unwrap();
    // The assignment FK requires the device row to exist.
    sqlx::query(
        "INSERT INTO devices (id, mac, last_ip, device_type, first_seen, last_seen, zone_id) \
         VALUES (?, 'aa:bb:cc:dd:ee:01', '10.0.0.1', 'unknown', ?, ?, \
                 '00000000-0000-0000-0000-000000000201')",
    )
    .bind(device.id.to_string())
    .bind("2026-01-01T00:00:00Z")
    .bind("2026-01-01T00:00:00Z")
    .execute(&pool)
    .await
    .unwrap();
    let repo = Arc::new(SqliteRoutingProfileRepository::new(pool));
    let profile = repo
        .create_profile(&RoutingProfileRow {
            id: Uuid::new_v4().to_string(),
            name: "Streaming".to_owned(),
        })
        .await
        .unwrap();
    repo.set_device_profiles(device.id, &[profile.id])
        .await
        .unwrap();
    let (tx, _rx) = tokio::sync::mpsc::channel(16);
    let routing_svc = Arc::new(RoutingProfileServiceImpl::new(
        repo,
        Arc::new(StubTunnelService),
        Arc::new(crate::tests::stubs::StubDeviceService),
        tx,
    ));

    let state = build_state(
        MockDeviceService::found(device, None),
        MockDiscoveryService {
            devices: vec![],
            audit: AuditLog::default(),
        },
    )
    .with_routing_profile_service(routing_svc);
    let app = device_router(state);

    let (status, json) = get_json(app, "/api/devices/me").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["routing_profiles"][0]["name"], "Streaming");
    assert_eq!(json["routing_profiles"][0]["id"], profile.id.to_string());
}

// ---------------------------------------------------------------------------
#[tokio::test]
async fn get_me_includes_tunnel_status_and_last_handshake() {
    let tunnel_id = Uuid::new_v4();
    let tunnel = wardnet_common::tunnel::Tunnel {
        id: tunnel_id,
        label: "UK Server".to_owned(),
        country_code: "GB".to_owned(),
        provider: None,
        interface_name: "wg0".to_owned(),
        endpoint: "1.2.3.4:51820".to_owned(),
        status: wardnet_common::tunnel::TunnelStatus::Up,
        last_handshake: None,
        bytes_tx: 0,
        bytes_rx: 0,
        created_at: chrono::Utc::now(),
        override_default_dns: false,
        server_selector: None,
        resolved_server_name: None,
        endpoint_resolved_at: None,
    };
    let state = build_state_with_tunnel_svc(
        MockDeviceService::found(sample_device(), None),
        MockDiscoveryService {
            devices: vec![],
            audit: AuditLog::default(),
        },
        MockTunnelService {
            tunnels: vec![tunnel],
        },
    );
    let app = device_router(state);

    let (status, json) = get_json(app, "/api/devices/me").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["available_tunnels"][0]["id"], tunnel_id.to_string());
    assert_eq!(json["available_tunnels"][0]["status"], "up");
    assert!(json["available_tunnels"][0]["last_handshake"].is_null());
}

// PUT /api/devices/me/rule
// ---------------------------------------------------------------------------

#[tokio::test]
async fn set_my_rule_success() {
    let state = build_state(
        MockDeviceService::found(sample_device(), None),
        MockDiscoveryService {
            devices: vec![],
            audit: AuditLog::default(),
        },
    );
    let app = device_router(state);

    let (status, json) = put_json(
        app,
        "/api/devices/me/rule",
        r#"{"target":{"type":"direct"}}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["target"]["type"], "direct");
    assert_eq!(json["message"], "routing rule updated");
}

#[tokio::test]
async fn set_my_rule_with_tunnel_target() {
    let tunnel_id = "00000000-0000-0000-0000-000000000010";
    let state = build_state(
        MockDeviceService::found(sample_device(), None),
        MockDiscoveryService {
            devices: vec![],
            audit: AuditLog::default(),
        },
    );
    let app = device_router(state);

    let body = format!(r#"{{"target":{{"type":"tunnel","tunnel_id":"{tunnel_id}"}}}}"#);
    let (status, json) = put_json(app, "/api/devices/me/rule", &body).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["target"]["type"], "tunnel");
    assert_eq!(json["target"]["tunnel_id"], tunnel_id);
}

#[tokio::test]
async fn set_my_rule_device_not_found() {
    let state = build_state(
        MockDeviceService::not_found(),
        MockDiscoveryService {
            devices: vec![],
            audit: AuditLog::default(),
        },
    );
    let app = device_router(state);

    let (status, json) = put_json(
        app,
        "/api/devices/me/rule",
        r#"{"target":{"type":"direct"}}"#,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"], "not found");
}

#[tokio::test]
async fn set_my_rule_forbidden_when_locked() {
    let mut device = sample_device();
    device.admin_locked = true;

    let svc = MockDeviceService::forbidden(device);
    let state = build_state(
        svc,
        MockDiscoveryService {
            devices: vec![],
            audit: AuditLog::default(),
        },
    );
    let app = device_router(state);

    let (status, json) = put_json(
        app,
        "/api/devices/me/rule",
        r#"{"target":{"type":"direct"}}"#,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(json["error"], "forbidden");
}

#[tokio::test]
async fn set_my_rule_bad_json_returns_error() {
    let state = build_state(
        MockDeviceService::found(sample_device(), None),
        MockDiscoveryService {
            devices: vec![],
            audit: AuditLog::default(),
        },
    );
    let app = device_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/devices/me/rule")
                .header("Content-Type", "application/json")
                .header("Cookie", "wardnet_session=valid-token")
                .extension(connect_info())
                .body(Body::from("not json"))
                .unwrap(),
        )
        .await
        .unwrap();

    // Axum returns 400 or 422 for deserialization failures depending on version.
    let status = resp.status();
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY,
        "expected 400 or 422, got {status}"
    );
}

// ---------------------------------------------------------------------------
// GET /api/devices (admin, list all)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_devices_returns_all() {
    let device = sample_device();
    let state = build_state_with_dhcp(
        MockDeviceService::not_found(),
        MockDiscoveryService {
            devices: vec![device],
            audit: AuditLog::default(),
        },
        MockDhcpService::empty(),
    );
    let app = device_router(state);

    let (status, json) = get_json(app, "/api/devices").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["devices"].as_array().unwrap().len(), 1);
    assert_eq!(json["devices"][0]["mac"], "aa:bb:cc:dd:ee:01");
    // No lease or reservation for this device, so dhcp_status should be "external".
    assert_eq!(json["devices"][0]["dhcp_status"], "external");
    // No routing rule for this device → it follows the gateway default policy,
    // surfaced as a null current_rule.
    assert!(json["devices"][0]["current_rule"].is_null());
}

#[tokio::test]
async fn list_devices_includes_routing_target() {
    // Two devices: one tunnel-routed, one with no rule (default policy).
    let mut tunnel_device = sample_device();
    let tunnel_id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
    tunnel_device.id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    tunnel_device.mac = "aa:bb:cc:dd:ee:01".to_owned();

    let mut default_device = sample_device();
    default_device.id = Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap();
    default_device.mac = "aa:bb:cc:dd:ee:02".to_owned();

    let mut rules = std::collections::HashMap::new();
    rules.insert(tunnel_device.id, RoutingTarget::Tunnel { tunnel_id });

    let state = build_state_with_dhcp(
        MockDeviceService::not_found().with_current_rules(rules),
        MockDiscoveryService {
            devices: vec![tunnel_device, default_device],
            audit: AuditLog::default(),
        },
        MockDhcpService::empty(),
    );
    let app = device_router(state);

    let (status, json) = get_json(app, "/api/devices").await;
    assert_eq!(status, StatusCode::OK);

    let devices = json["devices"].as_array().unwrap();
    assert_eq!(devices.len(), 2);

    let tunnel = devices
        .iter()
        .find(|d| d["mac"] == "aa:bb:cc:dd:ee:01")
        .unwrap();
    assert_eq!(tunnel["current_rule"]["type"], "tunnel");
    assert_eq!(
        tunnel["current_rule"]["tunnel_id"],
        "11111111-1111-1111-1111-111111111111"
    );

    let default = devices
        .iter()
        .find(|d| d["mac"] == "aa:bb:cc:dd:ee:02")
        .unwrap();
    assert!(default["current_rule"].is_null());
}

#[tokio::test]
async fn list_devices_returns_empty() {
    let state = build_state_with_dhcp(
        MockDeviceService::not_found(),
        MockDiscoveryService {
            devices: vec![],
            audit: AuditLog::default(),
        },
        MockDhcpService::empty(),
    );
    let app = device_router(state);

    let (status, json) = get_json(app, "/api/devices").await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["devices"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn list_devices_unauthorized_without_session() {
    let state = build_state_with_dhcp(
        MockDeviceService::not_found(),
        MockDiscoveryService {
            devices: vec![],
            audit: AuditLog::default(),
        },
        MockDhcpService::empty(),
    );
    let app = device_router(state);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/devices")
                .extension(connect_info())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ---------------------------------------------------------------------------
// GET /api/devices/:id (admin, detail)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_device_by_id_success() {
    let device = sample_device();
    let state = build_state_with_dhcp(
        MockDeviceService::found(device.clone(), Some(RoutingTarget::Direct)),
        MockDiscoveryService {
            devices: vec![device],
            audit: AuditLog::default(),
        },
        MockDhcpService::empty(),
    );
    let app = device_router(state);

    let (status, json) = get_json(app, "/api/devices/00000000-0000-0000-0000-000000000001").await;
    assert_eq!(status, StatusCode::OK);
    // DeviceDetailResponse is { device, current_rule } — device fields are
    // nested, current_rule sits at the top level.
    assert_eq!(json["device"]["mac"], "aa:bb:cc:dd:ee:01");
    assert_eq!(json["current_rule"]["type"], "direct");
    assert_eq!(json["device"]["dhcp_status"], "external");
    // No identification service injected: the field is present and empty
    // rather than absent, so the client never has to special-case it.
    assert_eq!(json["signals"], serde_json::json!([]));
}

#[tokio::test]
async fn get_device_by_id_returns_identification_signals() {
    let device = sample_device();
    let state = build_state_with_identification(
        device,
        MockIdentificationService::returning(vec![DeviceSignal {
            kind: DeviceSignalKind::MdnsService,
            value: "_govee._tcp".to_owned(),
            inferred: true,
            observed_at: chrono::Utc::now(),
        }]),
    );
    let app = device_router(state);

    let (status, json) = get_json(app, "/api/devices/00000000-0000-0000-0000-000000000001").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["signals"][0]["kind"], "mdns_service");
    assert_eq!(json["signals"][0]["value"], "_govee._tcp");
    assert_eq!(json["signals"][0]["inferred"], true);
}

#[tokio::test]
async fn get_device_by_id_fails_loudly_when_signals_cannot_be_read() {
    // An empty `signals` list is not "unknown" — the UI renders it as the
    // positive claim "nothing observed yet". On the read path we therefore
    // refuse to guess: a failed read must surface as an error rather than as a
    // confident falsehood about the device.
    let device = sample_device();
    let state = build_state_with_identification(device, MockIdentificationService::failing());
    let app = device_router(state);

    let (status, _json) = get_json(app, "/api/devices/00000000-0000-0000-0000-000000000001").await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn update_device_degrades_when_signals_cannot_be_read() {
    // The mutation has already landed by the time signals are read, so failing
    // here would report an error for a change that actually succeeded. The
    // client's next GET is what surfaces the problem.
    let device = sample_device();
    let state = build_state_with_identification(device, MockIdentificationService::failing());
    let app = device_router(state);

    let (status, json) = put_json(
        app,
        "/api/devices/00000000-0000-0000-0000-000000000001",
        r#"{"name":"Renamed"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["signals"], serde_json::json!([]));
}

#[tokio::test]
async fn get_device_by_id_not_found() {
    let state = build_state_with_dhcp(
        MockDeviceService::not_found(),
        MockDiscoveryService {
            devices: vec![],
            audit: AuditLog::default(),
        },
        MockDhcpService::empty(),
    );
    let app = device_router(state);

    let (status, json) = get_json(app, "/api/devices/00000000-0000-0000-0000-000000000099").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"], "not found");
}

#[tokio::test]
async fn get_device_by_id_invalid_uuid() {
    let state = build_state_with_dhcp(
        MockDeviceService::not_found(),
        MockDiscoveryService {
            devices: vec![],
            audit: AuditLog::default(),
        },
        MockDhcpService::empty(),
    );
    let app = device_router(state);

    let (status, json) = get_json(app, "/api/devices/not-a-uuid").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["error"], "bad request");
}

// ---------------------------------------------------------------------------
// PUT /api/devices/:id (admin, update)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn assign_device_zone_success() {
    let device = sample_device();
    let state = build_state_with_dhcp(
        MockDeviceService::found(device.clone(), Some(RoutingTarget::Direct)),
        MockDiscoveryService {
            devices: vec![device],
            audit: AuditLog::default(),
        },
        MockDhcpService::empty(),
    );
    let app = device_router(state);

    // The handler resolves the device's current rule by device ID (not by its
    // last_ip, which is cleared on departure — issue #831), so the response
    // still carries the correct rule for the reassigned device.
    let (status, json) = put_json(
        app,
        "/api/devices/00000000-0000-0000-0000-000000000001/zone",
        r#"{"zone_id":"00000000-0000-0000-0000-000000000201"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["device"]["mac"], "aa:bb:cc:dd:ee:01");
    assert_eq!(json["current_rule"]["type"], "direct");
}

#[tokio::test]
async fn update_device_success() {
    let device = sample_device();
    let state = build_state_with_dhcp(
        MockDeviceService::found(device.clone(), None),
        MockDiscoveryService {
            devices: vec![device],
            audit: AuditLog::default(),
        },
        MockDhcpService::empty(),
    );
    let app = device_router(state);

    let (status, json) = put_json(
        app,
        "/api/devices/00000000-0000-0000-0000-000000000001",
        r#"{"name":"Renamed Device"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["device"]["name"], "Renamed Device");
}

#[tokio::test]
async fn update_device_with_type() {
    let device = sample_device();
    let state = build_state_with_dhcp(
        MockDeviceService::found(device.clone(), None),
        MockDiscoveryService {
            devices: vec![device],
            audit: AuditLog::default(),
        },
        MockDhcpService::empty(),
    );
    let app = device_router(state);

    let (status, json) = put_json(
        app,
        "/api/devices/00000000-0000-0000-0000-000000000001",
        r#"{"device_type":"laptop"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["device"]["mac"].is_string());
}

#[tokio::test]
async fn update_device_invalid_uuid() {
    let state = build_state_with_dhcp(
        MockDeviceService::not_found(),
        MockDiscoveryService {
            devices: vec![],
            audit: AuditLog::default(),
        },
        MockDhcpService::empty(),
    );
    let app = device_router(state);

    let (status, json) = put_json(app, "/api/devices/not-a-uuid", r#"{"name":"x"}"#).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["error"], "bad request");
}

#[tokio::test]
async fn update_device_not_found() {
    let state = build_state_with_dhcp(
        MockDeviceService::not_found(),
        MockDiscoveryService {
            devices: vec![],
            audit: AuditLog::default(),
        },
        MockDhcpService::empty(),
    );
    let app = device_router(state);

    let (status, json) = put_json(
        app,
        "/api/devices/00000000-0000-0000-0000-000000000099",
        r#"{"name":"x"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"], "not found");
}

// ---------------------------------------------------------------------------
// PUT /api/devices/:id with routing_target and admin_locked
// ---------------------------------------------------------------------------

#[tokio::test]
async fn update_device_with_routing_target() {
    let device = sample_device();
    let state = build_state_with_dhcp(
        MockDeviceService::found(device.clone(), None),
        MockDiscoveryService {
            devices: vec![device],
            audit: AuditLog::default(),
        },
        MockDhcpService::empty(),
    );
    let app = device_router(state);

    let (status, _json) = put_json(
        app,
        "/api/devices/00000000-0000-0000-0000-000000000001",
        r#"{"routing_target":{"type":"direct"}}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn update_device_with_admin_locked() {
    let device = sample_device();
    let state = build_state_with_dhcp(
        MockDeviceService::found(device.clone(), None),
        MockDiscoveryService {
            devices: vec![device],
            audit: AuditLog::default(),
        },
        MockDhcpService::empty(),
    );
    let app = device_router(state);

    let (status, _json) = put_json(
        app,
        "/api/devices/00000000-0000-0000-0000-000000000001",
        r#"{"admin_locked":true}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

// ---------------------------------------------------------------------------
// DHCP status enrichment
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_devices_with_dhcp_lease_shows_lease_status() {
    let device = sample_device();
    let lease = wardnet_common::dhcp::DhcpLease {
        id: Uuid::new_v4(),
        mac_address: "aa:bb:cc:dd:ee:01".to_owned(),
        ip_address: "192.168.1.10".parse().unwrap(),
        hostname: None,
        lease_start: "2026-04-13T00:00:00Z".parse().unwrap(),
        lease_end: "2026-04-13T01:00:00Z".parse().unwrap(),
        status: wardnet_common::dhcp::DhcpLeaseStatus::Active,
        device_id: Some(device.id),
        created_at: "2026-04-13T00:00:00Z".parse().unwrap(),
        updated_at: "2026-04-13T00:00:00Z".parse().unwrap(),
    };

    let dhcp = MockDhcpService {
        leases: vec![lease],
        reservations: vec![],
        audit: AuditLog::default(),
    };

    let state = build_state_with_dhcp(
        MockDeviceService::not_found(),
        MockDiscoveryService {
            devices: vec![device],
            audit: AuditLog::default(),
        },
        dhcp,
    );
    let app = device_router(state);

    let (status, json) = get_json(app, "/api/devices").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["devices"][0]["dhcp_status"], "lease");
}

#[tokio::test]
async fn list_devices_with_dhcp_reservation_shows_reservation_status() {
    let device = sample_device();
    let reservation = wardnet_common::dhcp::DhcpReservation {
        id: Uuid::new_v4(),
        mac_address: "aa:bb:cc:dd:ee:01".to_owned(),
        ip_address: "192.168.1.10".parse().unwrap(),
        hostname: None,
        description: None,
        created_at: "2026-04-13T00:00:00Z".parse().unwrap(),
        updated_at: "2026-04-13T00:00:00Z".parse().unwrap(),
    };

    let dhcp = MockDhcpService {
        leases: vec![],
        reservations: vec![reservation],
        audit: AuditLog::default(),
    };

    let state = build_state_with_dhcp(
        MockDeviceService::not_found(),
        MockDiscoveryService {
            devices: vec![device],
            audit: AuditLog::default(),
        },
        dhcp,
    );
    let app = device_router(state);

    let (status, json) = get_json(app, "/api/devices").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["devices"][0]["dhcp_status"], "reservation");
}

#[tokio::test]
async fn list_devices_reservation_overrides_lease() {
    let device = sample_device();
    let lease = wardnet_common::dhcp::DhcpLease {
        id: Uuid::new_v4(),
        mac_address: "aa:bb:cc:dd:ee:01".to_owned(),
        ip_address: "192.168.1.10".parse().unwrap(),
        hostname: None,
        lease_start: "2026-04-13T00:00:00Z".parse().unwrap(),
        lease_end: "2026-04-13T01:00:00Z".parse().unwrap(),
        status: wardnet_common::dhcp::DhcpLeaseStatus::Active,
        device_id: Some(device.id),
        created_at: "2026-04-13T00:00:00Z".parse().unwrap(),
        updated_at: "2026-04-13T00:00:00Z".parse().unwrap(),
    };
    let reservation = wardnet_common::dhcp::DhcpReservation {
        id: Uuid::new_v4(),
        mac_address: "aa:bb:cc:dd:ee:01".to_owned(),
        ip_address: "192.168.1.10".parse().unwrap(),
        hostname: None,
        description: None,
        created_at: "2026-04-13T00:00:00Z".parse().unwrap(),
        updated_at: "2026-04-13T00:00:00Z".parse().unwrap(),
    };

    // Both lease and reservation for the same MAC -- reservation should win.
    let dhcp = MockDhcpService {
        leases: vec![lease],
        reservations: vec![reservation],
        audit: AuditLog::default(),
    };

    let state = build_state_with_dhcp(
        MockDeviceService::not_found(),
        MockDiscoveryService {
            devices: vec![device],
            audit: AuditLog::default(),
        },
        dhcp,
    );
    let app = device_router(state);

    let (status, json) = get_json(app, "/api/devices").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        json["devices"][0]["dhcp_status"], "reservation",
        "reservation should override lease"
    );
}

#[tokio::test]
async fn get_device_by_id_includes_dhcp_status() {
    let device = sample_device();
    let lease = wardnet_common::dhcp::DhcpLease {
        id: Uuid::new_v4(),
        mac_address: "aa:bb:cc:dd:ee:01".to_owned(),
        ip_address: "192.168.1.10".parse().unwrap(),
        hostname: None,
        lease_start: "2026-04-13T00:00:00Z".parse().unwrap(),
        lease_end: "2026-04-13T01:00:00Z".parse().unwrap(),
        status: wardnet_common::dhcp::DhcpLeaseStatus::Active,
        device_id: Some(device.id),
        created_at: "2026-04-13T00:00:00Z".parse().unwrap(),
        updated_at: "2026-04-13T00:00:00Z".parse().unwrap(),
    };

    let dhcp = MockDhcpService {
        leases: vec![lease],
        reservations: vec![],
        audit: AuditLog::default(),
    };

    let state = build_state_with_dhcp(
        MockDeviceService::found(device.clone(), Some(RoutingTarget::Direct)),
        MockDiscoveryService {
            devices: vec![device],
            audit: AuditLog::default(),
        },
        dhcp,
    );
    let app = device_router(state);

    let (status, json) = get_json(app, "/api/devices/00000000-0000-0000-0000-000000000001").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["device"]["dhcp_status"], "lease");
}

// ---------------------------------------------------------------------------
// POST /api/devices/{id}/release  (issue #1181)
// ---------------------------------------------------------------------------

/// Build a state for the release path, returning the shared audit log so the
/// test can assert the *order* the handler called things in.
fn build_release_state_with(
    device_svc: MockDeviceService,
    device: Device,
    audit: AuditLog,
) -> AppState {
    build_state_with_dhcp(
        device_svc,
        MockDiscoveryService {
            devices: vec![device],
            audit,
        },
        MockDhcpService::empty(),
    )
    .with_routing_profile_service(Arc::new(crate::tests::stubs::StubRoutingProfileService))
}

fn build_release_state(managed: bool) -> (AppState, AuditLog) {
    let mut device = sample_device();
    device.managed = managed;
    device.name = Some("Living Room TV".to_owned());

    let audit = AuditLog::default();
    let mut device_svc = MockDeviceService::found(device.clone(), Some(RoutingTarget::Direct));
    device_svc.audit = audit.clone();

    let state = build_release_state_with(device_svc, device, audit.clone());
    (state, audit)
}

#[tokio::test]
async fn release_clears_managed_last() {
    // The ordering IS the safety property: `clear_managed` asserts the
    // invariant "no admin artefacts exist", so it must only run once every
    // revert has landed. If it moved earlier, a later failure would leave an
    // unmanaged device still holding a live credential — which the retention
    // prune would then delete 30 days later.
    let (state, audit) = build_release_state(true);
    let app = device_router(state);

    let (status, _json) = post_json(
        app,
        "/api/devices/00000000-0000-0000-0000-000000000001/release",
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let calls = audit.lock().unwrap().clone();
    assert!(
        calls.contains(&"clear_managed"),
        "release must clear the managed flag; calls={calls:?}"
    );
    assert_eq!(
        calls.last(),
        Some(&"clear_managed"),
        "clear_managed must be the LAST call; calls={calls:?}"
    );
    // And it really did revert things on the way — not just flip the flag.
    assert!(
        calls.contains(&"update_admin_locked") && calls.contains(&"update_device"),
        "release must revert configuration, not merely unmanage; calls={calls:?}"
    );
}

#[tokio::test]
async fn release_leaves_the_device_managed_when_a_step_fails() {
    // A partial release must be recoverable by retrying, never a half-released
    // device carrying a credential it is no longer recorded as owning.
    let mut device = sample_device();
    device.managed = true;

    let audit = AuditLog::default();
    let mut device_svc = MockDeviceService::found(device.clone(), Some(RoutingTarget::Direct));
    device_svc.audit = audit.clone();
    device_svc.fail_admin_locked = true;

    let state = build_release_state_with(device_svc, device, audit.clone());
    let app = device_router(state);

    let (status, json) = post_json(
        app,
        "/api/devices/00000000-0000-0000-0000-000000000001/release",
    )
    .await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    // The response body is deliberately generic — the step name goes to the
    // log, not to the client.
    assert_eq!(json["error"], "internal server error");

    let calls = audit.lock().unwrap().clone();
    assert!(
        !calls.contains(&"clear_managed"),
        "a failed release must leave the device managed; calls={calls:?}"
    );
}

#[tokio::test]
async fn release_is_404_for_an_unknown_device() {
    // Every step below is individually tolerant of a missing device, so without
    // an up-front lookup the whole sequence would "succeed" against nothing.
    let state = build_state_with_dhcp(
        MockDeviceService::not_found(),
        MockDiscoveryService {
            devices: vec![],
            audit: AuditLog::default(),
        },
        MockDhcpService::empty(),
    )
    .with_routing_profile_service(Arc::new(crate::tests::stubs::StubRoutingProfileService));
    let app = device_router(state);

    let (status, _json) = post_json(
        app,
        "/api/devices/00000000-0000-0000-0000-000000000099/release",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn release_rejects_a_malformed_device_id() {
    let (state, _audit) = build_release_state(true);
    let app = device_router(state);

    let (status, _json) = post_json(app, "/api/devices/not-a-uuid/release").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

// ---------------------------------------------------------------------------
// Release with artefacts actually present (issue #1181)
// ---------------------------------------------------------------------------
//
// The release tests above run against empty collections, which proves the
// ordering but never executes a single revoke. These populate every artefact
// the release is supposed to tear down, so the destructive path — the one that
// disconnects a device — is actually exercised.

const RELEASE_DEVICE_ID: &str = "00000000-0000-0000-0000-000000000001";
const RELEASE_MAC: &str = "aa:bb:cc:dd:ee:01";

/// `PrivateDnsService` holding one grant for the device under test.
struct GrantingPrivateDns {
    audit: AuditLog,
}

#[async_trait]
impl wardnetd_services::private_dns::PrivateDnsService for GrantingPrivateDns {
    async fn list_grants(
        &self,
    ) -> Result<Vec<wardnetd_services::private_dns::PrivateDnsGrant>, AppError> {
        // `device_grant` is a provided method over this one, so seeding here is
        // enough to drive the handler's lookup.
        Ok(vec![wardnetd_services::private_dns::PrivateDnsGrant {
            id: Uuid::parse_str("00000000-0000-0000-0000-0000000000a1").unwrap(),
            device_id: Uuid::parse_str(RELEASE_DEVICE_ID).unwrap(),
            token: "secrettoken".to_owned(),
            created_at: "2026-01-01T00:00:00Z".to_owned(),
        }])
    }
    /// Overridden, not inherited: the trait's default `device_grant` returns
    /// `Ok(None)` unconditionally and is meant to be replaced by the impl (as
    /// `PrivateDnsServiceImpl` does). A double that inherited it would report
    /// "no grant" and quietly skip the revoke this test exists to check.
    async fn device_grant(
        &self,
        device_id: Uuid,
    ) -> Result<Option<wardnetd_services::private_dns::PrivateDnsGrant>, AppError> {
        Ok(self
            .list_grants()
            .await?
            .into_iter()
            .find(|g| g.device_id == device_id))
    }

    async fn revoke_grant(&self, _grant_id: Uuid) -> Result<(), AppError> {
        self.audit.lock().unwrap().push("revoke_grant");
        Ok(())
    }
    async fn status(&self) -> Result<wardnetd_services::private_dns::PrivateDnsStatus, AppError> {
        unimplemented!()
    }
    async fn is_enabled(&self) -> Result<bool, AppError> {
        Ok(true)
    }
    async fn set_enabled(
        &self,
        _enabled: bool,
    ) -> Result<wardnetd_services::private_dns::PrivateDnsStatus, AppError> {
        unimplemented!()
    }
    async fn grant_device(
        &self,
        _device_id: Uuid,
    ) -> Result<wardnetd_services::private_dns::PrivateDnsGrant, AppError> {
        unimplemented!()
    }
    async fn resolve_token(
        &self,
        _token: &str,
    ) -> Result<Option<wardnetd_services::private_dns::PrivateDnsGrant>, AppError> {
        unimplemented!()
    }
    async fn reconcile(&self) -> Result<(), AppError> {
        unimplemented!()
    }
}

/// `InboundWgService` holding one peer bound to the device under test.
struct PeeredInboundWg {
    audit: AuditLog,
}

#[async_trait]
impl wardnetd_services::InboundWgService for PeeredInboundWg {
    async fn list_peers(&self) -> Result<Vec<wardnet_common::api::InboundWgPeerSummary>, AppError> {
        Ok(vec![wardnet_common::api::InboundWgPeerSummary {
            id: Uuid::parse_str("00000000-0000-0000-0000-0000000000b1").unwrap(),
            name: "alice-phone".to_owned(),
            public_key: "pk".to_owned(),
            allowed_ip: "10.100.64.2/32".to_owned(),
            enabled: true,
            created_at: chrono::Utc::now(),
            device_id: Some(Uuid::parse_str(RELEASE_DEVICE_ID).unwrap()),
        }])
    }
    async fn remove_peer(&self, _id: Uuid) -> Result<(), AppError> {
        self.audit.lock().unwrap().push("remove_peer");
        Ok(())
    }
    async fn get_config(&self) -> Result<wardnet_common::api::InboundWgConfigResponse, AppError> {
        unimplemented!()
    }
    async fn set_config(
        &self,
        _enabled: bool,
        _listen_port: u16,
    ) -> Result<wardnet_common::api::InboundWgConfigResponse, AppError> {
        unimplemented!()
    }
    async fn add_peer(
        &self,
        _device_id: Uuid,
        _endpoint: Option<String>,
    ) -> Result<wardnet_common::api::AddInboundWgPeerResponse, AppError> {
        unimplemented!()
    }
    async fn set_peer_enabled(
        &self,
        _id: Uuid,
        _enabled: bool,
    ) -> Result<wardnet_common::api::InboundWgPeerSummary, AppError> {
        unimplemented!()
    }
    async fn list_peers_for_monitor(
        &self,
    ) -> Result<Vec<wardnetd_services::inbound_wg::InboundWgMonitorPeer>, AppError> {
        unimplemented!()
    }
    async fn reconcile(&self) -> Result<(), AppError> {
        unimplemented!()
    }
}

/// `ZoneExceptionService` holding one exception naming the device.
struct ExceptioningZones {
    audit: AuditLog,
}

impl ExceptioningZones {
    fn exception() -> wardnet_common::zone_exception::ZoneException {
        use wardnet_common::zone_exception::{
            ExceptionEndpoint, ExceptionEndpointKind, ServiceSet, ServiceSpec, ZoneException,
        };
        let now = chrono::Utc::now();
        ZoneException {
            id: Uuid::parse_str("00000000-0000-0000-0000-0000000000c1").unwrap(),
            // Device on the `to` side, so the handler's both-endpoints check is
            // exercised rather than only its first arm.
            from: ExceptionEndpoint {
                kind: ExceptionEndpointKind::Zone,
                id: Uuid::parse_str("00000000-0000-0000-0000-000000000202").unwrap(),
            },
            to: ExceptionEndpoint {
                kind: ExceptionEndpointKind::Device,
                id: Uuid::parse_str(RELEASE_DEVICE_ID).unwrap(),
            },
            service: ServiceSpec::Preset {
                set: ServiceSet::Casting,
            },
            bidirectional: true,
            created_at: now,
            updated_at: now,
        }
    }
}

#[async_trait]
impl wardnetd_services::ZoneExceptionService for ExceptioningZones {
    async fn list_exceptions(
        &self,
    ) -> Result<Vec<wardnet_common::zone_exception::ZoneException>, AppError> {
        Ok(vec![Self::exception()])
    }
    async fn delete_exception(&self, _id: Uuid) -> Result<(), AppError> {
        self.audit.lock().unwrap().push("delete_exception");
        Ok(())
    }
    async fn get_exception(
        &self,
        _id: Uuid,
    ) -> Result<wardnet_common::zone_exception::ZoneException, AppError> {
        unimplemented!()
    }
    async fn create_exception(
        &self,
        _req: wardnet_common::api::CreateZoneExceptionRequest,
    ) -> Result<wardnet_common::zone_exception::ZoneException, AppError> {
        unimplemented!()
    }
    async fn update_exception(
        &self,
        _id: Uuid,
        _req: wardnet_common::api::UpdateZoneExceptionRequest,
    ) -> Result<wardnet_common::zone_exception::ZoneException, AppError> {
        unimplemented!()
    }
}

/// A zone catalogue whose default-for-new zone is NOT the device's, so the
/// release's zone-reset step actually fires.
struct RezoningNetworkZones {
    audit: AuditLog,
}

const RELEASE_DEFAULT_ZONE: &str = "00000000-0000-0000-0000-0000000000e1";

#[async_trait]
impl wardnetd_services::NetworkZoneService for RezoningNetworkZones {
    async fn list_zones(&self) -> Result<Vec<wardnet_common::api::NetworkZoneView>, AppError> {
        let mut zone = crate::tests::stubs::StubNetworkZoneService::stub_zone();
        zone.id = Uuid::parse_str(RELEASE_DEFAULT_ZONE).unwrap();
        zone.is_default_for_new = true;
        Ok(vec![wardnet_common::api::NetworkZoneView {
            zone,
            member_count: 0,
        }])
    }
    async fn assign_device(&self, _device_id: Uuid, _zone_id: Uuid) -> Result<(), AppError> {
        self.audit.lock().unwrap().push("assign_device");
        Ok(())
    }
    async fn get_zone(&self, _id: Uuid) -> Result<wardnet_common::api::NetworkZoneView, AppError> {
        unimplemented!()
    }
    async fn create_zone(
        &self,
        _req: wardnet_common::api::CreateNetworkZoneRequest,
    ) -> Result<wardnet_common::network_zone::NetworkZone, AppError> {
        unimplemented!()
    }
    async fn update_zone(
        &self,
        _id: Uuid,
        _req: wardnet_common::api::UpdateNetworkZoneRequest,
    ) -> Result<wardnet_common::network_zone::NetworkZone, AppError> {
        unimplemented!()
    }
    async fn delete_zone(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn set_default(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn set_default_for_new(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn get_quarantine_new_devices(&self) -> Result<bool, AppError> {
        Ok(false)
    }
    async fn set_quarantine_new_devices(&self, _enabled: bool) -> Result<(), AppError> {
        unimplemented!()
    }
}

#[tokio::test]
async fn release_revokes_every_artefact_the_device_actually_has() {
    // The load-bearing release test. Every prior one runs against empty
    // collections, so the four revoke paths — the ones that disconnect a
    // device — never execute. This seeds a Private-DNS grant, a Remote peer,
    // a DHCP reservation, a zone exception and a differing default-for-new
    // zone, and asserts each is torn down, in order, with `clear_managed` last.
    let mut device = sample_device();
    device.managed = true;
    device.name = Some("Living Room TV".to_owned());

    let audit = AuditLog::default();
    let mut device_svc = MockDeviceService::found(device.clone(), Some(RoutingTarget::Direct));
    device_svc.audit = audit.clone();

    // `zone_exception` and `network_zone` are constructor params, not `with_`
    // builders, so this test assembles the state directly.
    let state = AppState::new(
        Arc::new(MockAuthService),
        Arc::new(crate::tests::stubs::StubBackupService),
        Arc::new(device_svc),
        Arc::new(MockDhcpService::with_reservation_for(
            RELEASE_MAC,
            audit.clone(),
        )),
        Arc::new(StubDnsService),
        Arc::new(StubDnsFilterService),
        Arc::new(StubDnsLocalService),
        Arc::new(crate::tests::stubs::StubDdnsService),
        Arc::new(crate::tests::stubs::StubTlsService),
        Arc::new(MockDiscoveryService {
            devices: vec![device],
            audit: audit.clone(),
        }),
        Arc::new(StubLogService) as Arc<dyn LogService>,
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(RezoningNetworkZones {
            audit: audit.clone(),
        }),
        Arc::new(StubSystemService),
        Arc::new(StubTunnelService),
        Arc::new(crate::tests::stubs::StubUpdateService),
        Arc::new(StubDhcpServer),
        Arc::new(StubDnsServer),
        Arc::new(StubEventPublisher),
        crate::tests::stubs::StubJobService::new_arc(),
        Arc::new(crate::tests::stubs::StubStatsService),
        Arc::new(crate::tests::stubs::StubAccessRequestService),
        Arc::new(ExceptioningZones {
            audit: audit.clone(),
        }),
    )
    .with_routing_profile_service(Arc::new(crate::tests::stubs::StubRoutingProfileService))
    .with_private_dns_service(Arc::new(GrantingPrivateDns {
        audit: audit.clone(),
    }))
    .with_inbound_wg_service(Arc::new(PeeredInboundWg {
        audit: audit.clone(),
    }));
    let app = device_router(state);

    let (status, _json) =
        post_json(app, &format!("/api/devices/{RELEASE_DEVICE_ID}/release")).await;
    assert_eq!(status, StatusCode::OK);

    let calls = audit.lock().unwrap().clone();
    for expected in [
        "revoke_grant",
        "remove_peer",
        "delete_reservation",
        "delete_exception",
        "clear_rule",
        "update_dns_capture_settings",
        "update_admin_locked",
        "update_device",
        "assign_device",
        "clear_managed",
    ] {
        assert!(
            calls.contains(&expected),
            "release did not perform '{expected}'; calls={calls:?}"
        );
    }

    // The grant is revoked FIRST — it publishes `PrivateDnsGrantRevoked`, which
    // tears down the device's live DoT sessions.
    assert_eq!(calls.first(), Some(&"revoke_grant"), "calls={calls:?}");
    // And `clear_managed` is LAST, so the "no admin artefacts exist" invariant
    // is only asserted once every artefact above is actually gone.
    assert_eq!(calls.last(), Some(&"clear_managed"), "calls={calls:?}");
}

// ---------------------------------------------------------------------------
// POST /api/devices/{id}/identify  (issue #1116)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn identify_returns_what_was_probed_alongside_what_answered() {
    // `ports_probed` is what lets a client distinguish "we contacted these and
    // none answered" from "nothing happened".
    let device = sample_device();
    let state =
        build_state_with_identification(device, MockIdentificationService::returning(Vec::new()));
    let app = device_router(state);

    let (status, json) = post_json(
        app,
        "/api/devices/00000000-0000-0000-0000-000000000001/identify",
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["ports_probed"], serde_json::json!([4003, 6668]));
    assert_eq!(json["answering_ports"], serde_json::json!([6668]));
    // The refreshed detail rides along so a caller sees new signals without a
    // second round trip.
    assert!(json["device"]["device"].is_object());
}

#[tokio::test]
async fn identify_surfaces_the_online_only_refusal_as_409() {
    // The daemon refuses a probe for a departed device — its last known address
    // may since have been handed to someone else. The status has to be
    // distinguishable so the UI can explain rather than say "failed".
    let device = sample_device();
    let state =
        build_state_with_identification(device, MockIdentificationService::refusing_probe());
    let app = device_router(state);

    let (status, _) = post_json(
        app,
        "/api/devices/00000000-0000-0000-0000-000000000001/identify",
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn identify_rejects_a_malformed_device_id() {
    let device = sample_device();
    let state =
        build_state_with_identification(device, MockIdentificationService::returning(Vec::new()));
    let app = device_router(state);

    let (status, _) = post_json(app, "/api/devices/not-a-uuid/identify").await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
}
