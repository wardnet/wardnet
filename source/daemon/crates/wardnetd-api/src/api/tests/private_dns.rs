//! Tests for the Private DNS API (`/api/private-dns/*`) — issue #914. Drives
//! the real handlers through an axum router with a canned `PrivateDnsService`,
//! asserting status codes, the status/prerequisite envelope, the 409 enable
//! gate, and the device-keyed `me` surface (including the signed-profile
//! content type).

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post, put};
use chrono::Utc;
use tower::ServiceExt;
use uuid::Uuid;
use wardnet_common::api::{DeviceMeResponse, PrivateDnsMeResponse, PrivateDnsStatusResponse};
use wardnet_common::device::{Device, DeviceConnectionMode, DeviceType};

use crate::state::AppState;
use crate::tests::stubs::{
    AlwaysAdminAuth, StubBackupService, StubDdnsService, StubDhcpServer, StubDhcpService,
    StubDiscoveryService, StubDnsFilterService, StubDnsLocalService, StubDnsServer, StubDnsService,
    StubEventPublisher, StubLogService, StubNetworkZoneService, StubProviderService,
    StubRoutingService, StubRuleRequestService, StubStatsService, StubSystemService,
    StubTunnelService, StubUpdateService, StubZoneExceptionService,
};
use wardnetd_services::entitlement::Entitlement;
use wardnetd_services::error::AppError;
use wardnetd_services::private_dns::{PrivateDnsGrant, PrivateDnsService, PrivateDnsStatus};
use wardnetd_services::{DeviceService, LogService, TlsService, TlsStatus};

// The API layer derives the grant domain from the DDNS status, so this must
// match `StubDdnsService`'s wardnet fqdn (the single source of truth).
const DOMAIN: &str = "test.my.wardnet.services";
// The device the source IP resolves to in the device-keyed tests.
const ME_DEVICE: Uuid = Uuid::from_u128(0x1111_2222_3333_4444_5555_6666_7777_8888);

// ── Canned PrivateDnsService ─────────────────────────────────────────────────

/// Configurable to drive every documented path without a per-test struct.
#[derive(Default)]
struct MockPrivateDns {
    enabled: AtomicBool,
    /// The single grant `list_grants` / `device_grant` return (its `device_id`
    /// gates the device-keyed lookups).
    grant: Option<PrivateDnsGrant>,
    /// When set, `set_enabled(true)` rejects with 409 (a missing hard prereq).
    enable_conflicts: bool,
    /// Bytes `device_profile` returns for the granted device (`None` → 404).
    profile: Option<Vec<u8>>,
}

fn sample_grant() -> PrivateDnsGrant {
    PrivateDnsGrant {
        id: Uuid::from_u128(0xabcd),
        device_id: ME_DEVICE,
        token: "abcdefghijklmnop".to_owned(),
        created_at: "2026-07-20T00:00:00Z".to_owned(),
    }
}

#[async_trait]
impl PrivateDnsService for MockPrivateDns {
    async fn status(&self) -> Result<PrivateDnsStatus, AppError> {
        Ok(PrivateDnsStatus {
            enabled: self.enabled.load(Ordering::Relaxed),
            domain: Some(DOMAIN.to_owned()),
        })
    }
    async fn is_enabled(&self) -> Result<bool, AppError> {
        Ok(self.enabled.load(Ordering::Relaxed))
    }
    async fn set_enabled(&self, enabled: bool) -> Result<PrivateDnsStatus, AppError> {
        if enabled && self.enable_conflicts {
            return Err(AppError::Conflict(
                "Private DNS requires the wardnet DNS provider".to_owned(),
            ));
        }
        self.enabled.store(enabled, Ordering::Relaxed);
        Ok(PrivateDnsStatus {
            enabled,
            domain: Some(DOMAIN.to_owned()),
        })
    }
    async fn grant_device(&self, device_id: Uuid) -> Result<PrivateDnsGrant, AppError> {
        Ok(PrivateDnsGrant {
            device_id,
            ..sample_grant()
        })
    }
    async fn revoke_grant(&self, _grant_id: Uuid) -> Result<(), AppError> {
        Ok(())
    }
    async fn list_grants(&self) -> Result<Vec<PrivateDnsGrant>, AppError> {
        Ok(self.grant.clone().into_iter().collect())
    }
    async fn device_grant(&self, device_id: Uuid) -> Result<Option<PrivateDnsGrant>, AppError> {
        Ok(self.grant.clone().filter(|g| g.device_id == device_id))
    }
    async fn device_profile(&self, _device_id: Uuid) -> Result<Option<Vec<u8>>, AppError> {
        Ok(self.profile.clone())
    }
    async fn resolve_token(&self, _token: &str) -> Result<Option<PrivateDnsGrant>, AppError> {
        Ok(None)
    }
    async fn reconcile(&self) -> Result<(), AppError> {
        Ok(())
    }
}

// ── TLS stub whose `status` resolves (the shared one panics) ─────────────────

struct IssuedTls;

#[async_trait]
impl TlsService for IssuedTls {
    async fn ensure_certificate(&self) -> Result<TlsStatus, AppError> {
        unimplemented!()
    }
    async fn status(&self) -> Result<TlsStatus, AppError> {
        Ok(TlsStatus::UpToDate {
            domain: DOMAIN.to_owned(),
            not_after: Utc::now(),
        })
    }
    async fn mark_provisioning_started(&self) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn provisioning_status(
        &self,
    ) -> Result<wardnet_common::api::TlsStatusResponse, AppError> {
        unimplemented!()
    }
    async fn teardown(&self) -> Result<(), AppError> {
        unimplemented!()
    }
}

// ── DeviceService whose source-IP lookup resolves to `ME_DEVICE` ─────────────

struct MeDeviceService {
    /// When false, the source IP resolves to no device (unknown caller).
    known: bool,
}

fn me_device() -> Device {
    Device {
        id: ME_DEVICE,
        mac: "aa:bb:cc:dd:ee:ff".to_owned(),
        name: Some("phone".to_owned()),
        hostname: None,
        manufacturer: None,
        device_type: DeviceType::Unknown,
        first_seen: Utc::now(),
        last_seen: Utc::now(),
        last_ip: "192.168.1.10".to_owned(),
        admin_locked: false,
        zone_id: Uuid::nil(),
        dns_capture_enabled: false,
        dns_capture_cap_count: 0,
        dns_capture_cap_days: 0,
        connection_mode: DeviceConnectionMode::Lan,
    }
}

#[async_trait]
impl DeviceService for MeDeviceService {
    async fn get_device_for_ip(&self, _ip: &str) -> Result<DeviceMeResponse, AppError> {
        Ok(DeviceMeResponse {
            device: self.known.then(me_device),
            current_rule: None,
            admin_locked: false,
            available_tunnels: vec![],
            zone: None,
            routing_profiles: vec![],
        })
    }
    async fn get_device(&self, _id: &str) -> Result<Option<Device>, AppError> {
        unimplemented!()
    }
    async fn clear_remote_connection_mode(&self, _id: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn set_rule_for_ip(
        &self,
        _ip: &str,
        _t: wardnet_common::routing::RoutingTarget,
    ) -> Result<wardnet_common::api::SetMyRuleResponse, AppError> {
        unimplemented!()
    }
    async fn set_rule(
        &self,
        _id: &str,
        _t: wardnet_common::routing::RoutingTarget,
    ) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn current_rules(
        &self,
    ) -> Result<std::collections::HashMap<Uuid, wardnet_common::routing::RoutingTarget>, AppError>
    {
        unimplemented!()
    }
    async fn get_rule_for_device(
        &self,
        _id: &str,
    ) -> Result<Option<wardnet_common::routing::RoutingTarget>, AppError> {
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
    async fn set_my_capture_enabled(
        &self,
        _ip: &str,
        _enabled: bool,
    ) -> Result<wardnet_common::api::DnsCaptureSettingsResponse, AppError> {
        unimplemented!()
    }
    async fn fetch_pending_dns_events(
        &self,
        _id: &str,
        _after_id: i64,
        _limit: i64,
    ) -> Result<Vec<wardnet_common::api::DnsEventItem>, AppError> {
        unimplemented!()
    }
    async fn ack_dns_events(&self, _id: &str, _up_to_id: i64) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn list_capture_enabled_device_ids(&self) -> Result<Vec<String>, AppError> {
        unimplemented!()
    }
    async fn get_device_capture_settings(
        &self,
        _id: &str,
    ) -> Result<Option<(bool, i64, i64)>, AppError> {
        unimplemented!()
    }
}

// ── Harness ──────────────────────────────────────────────────────────────────

fn make_state(private_dns: MockPrivateDns, device: MeDeviceService) -> AppState {
    AppState::new(
        Arc::new(AlwaysAdminAuth),
        Arc::new(StubBackupService),
        Arc::new(device),
        Arc::new(StubDhcpService),
        Arc::new(StubDnsService),
        Arc::new(StubDnsFilterService),
        Arc::new(StubDnsLocalService),
        Arc::new(StubDdnsService),
        Arc::new(IssuedTls),
        Arc::new(StubDiscoveryService),
        Arc::new(StubLogService) as Arc<dyn LogService>,
        Arc::new(StubProviderService),
        Arc::new(StubRoutingService),
        Arc::new(StubNetworkZoneService),
        Arc::new(StubSystemService),
        Arc::new(StubTunnelService),
        Arc::new(StubUpdateService),
        Arc::new(StubDhcpServer),
        Arc::new(StubDnsServer),
        Arc::new(StubEventPublisher),
        crate::tests::stubs::StubJobService::new_arc(),
        Arc::new(StubStatsService),
        Arc::new(StubRuleRequestService),
        Arc::new(StubZoneExceptionService),
    )
    .with_private_dns_service(Arc::new(private_dns))
    .with_entitlement({
        // A subscribed box, so the `entitled` prerequisite reads true.
        let entitlement = Entitlement::shared();
        entitlement.set_premium(true);
        entitlement
    })
}

fn app(state: AppState) -> Router {
    Router::new()
        .route(
            "/api/private-dns/status",
            get(crate::api::private_dns::get_status),
        )
        .route(
            "/api/private-dns",
            put(crate::api::private_dns::set_enabled),
        )
        .route(
            "/api/private-dns/grants",
            post(crate::api::private_dns::create_grant),
        )
        .route(
            "/api/private-dns/grants/{device_id}",
            axum::routing::delete(crate::api::private_dns::delete_grant),
        )
        .route("/api/private-dns/me", get(crate::api::private_dns::get_me))
        .route(
            "/api/private-dns/me/profile",
            get(crate::api::private_dns::get_me_profile),
        )
        .with_state(state)
}

fn connect_info() -> ConnectInfo<SocketAddr> {
    ConnectInfo("192.168.1.10:44321".parse().unwrap())
}

fn admin_request(method: &str, uri: &str, body: Option<serde_json::Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("Cookie", "wardnet_session=test");
    let body = match body {
        Some(value) => {
            builder = builder.header("Content-Type", "application/json");
            Body::from(serde_json::to_vec(&value).unwrap())
        }
        None => Body::empty(),
    };
    let mut req = builder.body(body).unwrap();
    req.extensions_mut().insert(connect_info());
    req
}

fn anon_get(uri: &str) -> Request<Body> {
    let mut req = Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .unwrap();
    req.extensions_mut().insert(connect_info());
    req
}

// ── Admin endpoints ──────────────────────────────────────────────────────────

#[tokio::test]
async fn status_reports_enabled_prereqs_and_grants() {
    let state = make_state(
        MockPrivateDns {
            enabled: AtomicBool::new(true),
            grant: Some(sample_grant()),
            ..Default::default()
        },
        MeDeviceService { known: true },
    );
    let resp = app(state)
        .oneshot(admin_request("GET", "/api/private-dns/status", None))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
    let json: PrivateDnsStatusResponse = serde_json::from_slice(&body).unwrap();
    assert!(json.enabled);
    assert_eq!(json.domain.as_deref(), Some(DOMAIN));
    // Prereqs are derived from the stub DDNS (wardnet) + issued TLS + entitled.
    assert!(json.prerequisites.wardnet_provider);
    assert!(json.prerequisites.certificate);
    assert!(json.prerequisites.entitled);
    assert_eq!(json.grants.len(), 1);
    // The grant carries the full minted hostname, not the bare token.
    assert_eq!(
        json.grants[0].hostname,
        format!("abcdefghijklmnop.{DOMAIN}")
    );
}

#[tokio::test]
async fn set_enabled_returns_full_status() {
    let state = make_state(MockPrivateDns::default(), MeDeviceService { known: true });
    let resp = app(state)
        .oneshot(admin_request(
            "PUT",
            "/api/private-dns",
            Some(serde_json::json!({ "enabled": true })),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
    let json: PrivateDnsStatusResponse = serde_json::from_slice(&body).unwrap();
    assert!(json.enabled);
}

#[tokio::test]
async fn set_enabled_conflicts_when_prereq_missing() {
    let state = make_state(
        MockPrivateDns {
            enable_conflicts: true,
            ..Default::default()
        },
        MeDeviceService { known: true },
    );
    let resp = app(state)
        .oneshot(admin_request(
            "PUT",
            "/api/private-dns",
            Some(serde_json::json!({ "enabled": true })),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn create_grant_returns_201_with_hostname() {
    let state = make_state(
        MockPrivateDns {
            enabled: AtomicBool::new(true),
            ..Default::default()
        },
        MeDeviceService { known: true },
    );
    let resp = app(state)
        .oneshot(admin_request(
            "POST",
            "/api/private-dns/grants",
            Some(serde_json::json!({ "device_id": ME_DEVICE })),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
    let json: wardnet_common::api::PrivateDnsGrantSummary = serde_json::from_slice(&body).unwrap();
    assert_eq!(json.device_id, ME_DEVICE);
    assert!(json.hostname.ends_with(&format!(".{DOMAIN}")));
}

#[tokio::test]
async fn delete_grant_returns_204_when_present() {
    let state = make_state(
        MockPrivateDns {
            enabled: AtomicBool::new(true),
            grant: Some(sample_grant()),
            ..Default::default()
        },
        MeDeviceService { known: true },
    );
    let resp = app(state)
        .oneshot(admin_request(
            "DELETE",
            &format!("/api/private-dns/grants/{ME_DEVICE}"),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn delete_grant_returns_404_when_absent() {
    let state = make_state(
        MockPrivateDns {
            enabled: AtomicBool::new(true),
            grant: None,
            ..Default::default()
        },
        MeDeviceService { known: true },
    );
    let resp = app(state)
        .oneshot(admin_request(
            "DELETE",
            &format!("/api/private-dns/grants/{}", Uuid::new_v4()),
            None,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn admin_endpoints_reject_unauthenticated() {
    let cases: [(&str, &str, Option<serde_json::Value>); 4] = [
        ("GET", "/api/private-dns/status", None),
        (
            "PUT",
            "/api/private-dns",
            Some(serde_json::json!({ "enabled": true })),
        ),
        (
            "POST",
            "/api/private-dns/grants",
            Some(serde_json::json!({ "device_id": ME_DEVICE })),
        ),
        (
            "DELETE",
            "/api/private-dns/grants/00000000-0000-0000-0000-000000000000",
            None,
        ),
    ];
    for (method, uri, body) in cases {
        let state = make_state(MockPrivateDns::default(), MeDeviceService { known: true });
        let mut builder = Request::builder().method(method).uri(uri);
        let req_body = match body {
            Some(value) => {
                builder = builder.header("Content-Type", "application/json");
                Body::from(serde_json::to_vec(&value).unwrap())
            }
            None => Body::empty(),
        };
        let mut req = builder.body(req_body).unwrap();
        req.extensions_mut().insert(connect_info());
        let resp = app(state).oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {uri} must reject an unauthenticated caller"
        );
    }
}

// ── Device-keyed surface (unauthenticated, by source IP) ─────────────────────

#[tokio::test]
async fn me_reports_this_devices_grant() {
    let state = make_state(
        MockPrivateDns {
            enabled: AtomicBool::new(true),
            grant: Some(sample_grant()),
            ..Default::default()
        },
        MeDeviceService { known: true },
    );
    let resp = app(state)
        .oneshot(anon_get("/api/private-dns/me"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
    let json: PrivateDnsMeResponse = serde_json::from_slice(&body).unwrap();
    assert!(json.enabled);
    assert!(json.granted);
    assert_eq!(
        json.hostname.as_deref(),
        Some(&*format!("abcdefghijklmnop.{DOMAIN}"))
    );
}

#[tokio::test]
async fn me_reports_ungranted_device() {
    let state = make_state(
        MockPrivateDns {
            enabled: AtomicBool::new(true),
            grant: None,
            ..Default::default()
        },
        MeDeviceService { known: true },
    );
    let resp = app(state)
        .oneshot(anon_get("/api/private-dns/me"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
    let json: PrivateDnsMeResponse = serde_json::from_slice(&body).unwrap();
    assert!(!json.granted);
    assert!(json.hostname.is_none());
}

#[tokio::test]
async fn me_hides_hostname_while_disabled() {
    // A retained grant surfaces as `granted`, but the hostname is withheld
    // while the feature is off — it wouldn't resolve, matching /me/profile's 404.
    let state = make_state(
        MockPrivateDns {
            enabled: AtomicBool::new(false),
            grant: Some(sample_grant()),
            ..Default::default()
        },
        MeDeviceService { known: true },
    );
    let resp = app(state)
        .oneshot(anon_get("/api/private-dns/me"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
    let json: PrivateDnsMeResponse = serde_json::from_slice(&body).unwrap();
    assert!(!json.enabled);
    assert!(json.granted);
    assert!(json.hostname.is_none());
}

#[tokio::test]
async fn me_profile_serves_signed_bytes_with_apple_content_type() {
    let state = make_state(
        MockPrivateDns {
            enabled: AtomicBool::new(true),
            grant: Some(sample_grant()),
            profile: Some(b"\x30\x82fake-cms".to_vec()),
            ..Default::default()
        },
        MeDeviceService { known: true },
    );
    let resp = app(state)
        .oneshot(anon_get("/api/private-dns/me/profile"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers()
            .get(axum::http::header::CONTENT_TYPE)
            .unwrap(),
        "application/x-apple-aspen-config"
    );
    let body = axum::body::to_bytes(resp.into_body(), 8192).await.unwrap();
    assert_eq!(&body[..], b"\x30\x82fake-cms");
}

#[tokio::test]
async fn me_profile_404_when_not_granted() {
    let state = make_state(
        MockPrivateDns {
            enabled: AtomicBool::new(true),
            grant: None,
            profile: None,
            ..Default::default()
        },
        MeDeviceService { known: true },
    );
    let resp = app(state)
        .oneshot(anon_get("/api/private-dns/me/profile"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
