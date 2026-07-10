//! Tests for the inbound (multi-peer) `WireGuard` admin API
//! (`/api/inbound-wg/*`) — issue #811. Drives the real handlers through an
//! axum router with a canned `InboundWgService`, asserting status codes,
//! response envelopes, and the auth guard.

use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{delete, get, put};
use tower::ServiceExt;
use uuid::Uuid;
use wardnet_common::api::{
    AddInboundWgPeerResponse, InboundWgConfigResponse, InboundWgPeerSummary,
};

use crate::state::AppState;
use crate::tests::stubs::{
    AlwaysAdminAuth, StubBackupService, StubDdnsService, StubDeviceService, StubDhcpServer,
    StubDhcpService, StubDiscoveryService, StubDnsFilterService, StubDnsLocalService,
    StubDnsServer, StubDnsService, StubEventPublisher, StubLogService, StubNetworkZoneService,
    StubProviderService, StubRoutingService, StubRuleRequestService, StubStatsService,
    StubSystemService, StubTlsService, StubTunnelService, StubUpdateService,
    StubZoneExceptionService,
};
use wardnetd_services::LogService;
use wardnetd_services::error::AppError;
use wardnetd_services::inbound_wg::service::{InboundWgMonitorPeer, InboundWgService};

// ── Mock ────────────────────────────────────────────────────────────────────

/// Which outcome `add_peer` should return, so a single mock can drive every
/// documented response path (201 / 404 / 409) without a per-test struct.
enum AddPeerOutcome {
    Success,
    DeviceNotFound,
    AlreadyGranted,
}

struct MockInboundWg {
    add_peer_outcome: AddPeerOutcome,
}

#[async_trait]
impl InboundWgService for MockInboundWg {
    async fn set_config(
        &self,
        enabled: bool,
        listen_port: u16,
    ) -> Result<InboundWgConfigResponse, AppError> {
        Ok(InboundWgConfigResponse {
            enabled,
            listen_port,
            server_public_key: Some("c2VydmVyLXB1YmtleQ==".to_owned()),
        })
    }

    async fn add_peer(&self, device_id: Uuid) -> Result<AddInboundWgPeerResponse, AppError> {
        match self.add_peer_outcome {
            AddPeerOutcome::Success => Ok(AddInboundWgPeerResponse {
                id: Uuid::nil(),
                name: "kitchen-tv".to_owned(),
                public_key: "cHVia2V5".to_owned(),
                private_key: "cHJpdmtleQ==".to_owned(),
                allowed_ip: "10.100.64.2/32".to_owned(),
            }),
            AddPeerOutcome::DeviceNotFound => {
                Err(AppError::NotFound(format!("device {device_id} not found")))
            }
            AddPeerOutcome::AlreadyGranted => Err(AppError::Conflict(
                "device already has an inbound WireGuard credential".to_owned(),
            )),
        }
    }

    async fn remove_peer(&self, _id: Uuid) -> Result<(), AppError> {
        Ok(())
    }

    async fn list_peers(&self) -> Result<Vec<InboundWgPeerSummary>, AppError> {
        Ok(vec![InboundWgPeerSummary {
            id: Uuid::nil(),
            name: "kitchen-tv".to_owned(),
            public_key: "cHVia2V5".to_owned(),
            allowed_ip: "10.100.64.2/32".to_owned(),
            enabled: true,
            created_at: chrono::Utc::now(),
        }])
    }

    async fn list_peers_for_monitor(&self) -> Result<Vec<InboundWgMonitorPeer>, AppError> {
        unimplemented!()
    }

    async fn reconcile(&self) -> Result<(), AppError> {
        unimplemented!()
    }
}

// ── Harness ─────────────────────────────────────────────────────────────────

fn make_state(inbound_wg: Arc<dyn InboundWgService>) -> AppState {
    AppState::new(
        Arc::new(AlwaysAdminAuth),
        Arc::new(StubBackupService),
        Arc::new(StubDeviceService),
        Arc::new(StubDhcpService),
        Arc::new(StubDnsService),
        Arc::new(StubDnsFilterService),
        Arc::new(StubDnsLocalService),
        Arc::new(StubDdnsService),
        Arc::new(StubTlsService),
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
    .with_inbound_wg_service(inbound_wg)
}

fn app(state: AppState) -> Router {
    Router::new()
        .route(
            "/api/inbound-wg/config",
            put(crate::api::inbound_wg::set_config),
        )
        .route(
            "/api/inbound-wg/peers",
            get(crate::api::inbound_wg::list_peers).post(crate::api::inbound_wg::add_peer),
        )
        .route(
            "/api/inbound-wg/peers/{id}",
            delete(crate::api::inbound_wg::remove_peer),
        )
        .with_state(state)
}

fn admin_request(method: &str, uri: &str, body: Option<serde_json::Value>) -> Request<Body> {
    request(method, uri, body, true)
}

fn request(
    method: &str,
    uri: &str,
    body: Option<serde_json::Value>,
    authenticated: bool,
) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    if authenticated {
        builder = builder.header("Cookie", "wardnet_session=test");
    }
    let body = match body {
        Some(value) => {
            builder = builder.header("Content-Type", "application/json");
            Body::from(serde_json::to_vec(&value).unwrap())
        }
        None => Body::empty(),
    };
    builder.body(body).unwrap()
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn set_config_enables_and_returns_server_pubkey() {
    let state = make_state(Arc::new(MockInboundWg {
        add_peer_outcome: AddPeerOutcome::Success,
    }));
    let req = admin_request(
        "PUT",
        "/api/inbound-wg/config",
        Some(serde_json::json!({ "enabled": true, "listen_port": 51900 })),
    );
    let resp = app(state).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: InboundWgConfigResponse = serde_json::from_slice(&body).unwrap();
    assert!(json.enabled);
    assert_eq!(json.listen_port, 51900);
    assert!(json.server_public_key.is_some());
}

#[tokio::test]
async fn list_peers_returns_configured_peers() {
    let state = make_state(Arc::new(MockInboundWg {
        add_peer_outcome: AddPeerOutcome::Success,
    }));
    let req = admin_request("GET", "/api/inbound-wg/peers", None);
    let resp = app(state).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let peers = value["peers"].as_array().unwrap();
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0]["name"], "kitchen-tv");
    // Pin the exact field set so a future handler change that swaps in a
    // type carrying `private_key` (e.g. `AddInboundWgPeerResponse`) fails
    // this test instead of silently leaking it — `InboundWgPeerSummary`
    // itself has no such field, so this can't be caught by the type system
    // alone once erased through `serde_json::Value`.
    let mut keys: Vec<&str> = peers[0]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "allowed_ip",
            "created_at",
            "enabled",
            "id",
            "name",
            "public_key"
        ]
    );
}

#[tokio::test]
async fn add_peer_success_returns_private_key_once() {
    let state = make_state(Arc::new(MockInboundWg {
        add_peer_outcome: AddPeerOutcome::Success,
    }));
    let req = admin_request(
        "POST",
        "/api/inbound-wg/peers",
        Some(serde_json::json!({ "device_id": Uuid::new_v4() })),
    );
    let resp = app(state).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
    let json: AddInboundWgPeerResponse = serde_json::from_slice(&body).unwrap();
    assert!(!json.private_key.is_empty());
}

#[tokio::test]
async fn add_peer_device_not_found_returns_404() {
    let state = make_state(Arc::new(MockInboundWg {
        add_peer_outcome: AddPeerOutcome::DeviceNotFound,
    }));
    let req = admin_request(
        "POST",
        "/api/inbound-wg/peers",
        Some(serde_json::json!({ "device_id": Uuid::new_v4() })),
    );
    let resp = app(state).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn add_peer_already_granted_returns_409() {
    let state = make_state(Arc::new(MockInboundWg {
        add_peer_outcome: AddPeerOutcome::AlreadyGranted,
    }));
    let req = admin_request(
        "POST",
        "/api/inbound-wg/peers",
        Some(serde_json::json!({ "device_id": Uuid::new_v4() })),
    );
    let resp = app(state).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn remove_peer_returns_no_content() {
    let state = make_state(Arc::new(MockInboundWg {
        add_peer_outcome: AddPeerOutcome::Success,
    }));
    let req = admin_request(
        "DELETE",
        &format!("/api/inbound-wg/peers/{}", Uuid::new_v4()),
        None,
    );
    let resp = app(state).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn inbound_wg_endpoints_reject_unauthenticated() {
    let routes: [(&str, &str, Option<serde_json::Value>); 4] = [
        (
            "PUT",
            "/api/inbound-wg/config",
            Some(serde_json::json!({ "enabled": true, "listen_port": 51900 })),
        ),
        ("GET", "/api/inbound-wg/peers", None),
        (
            "POST",
            "/api/inbound-wg/peers",
            Some(serde_json::json!({ "device_id": Uuid::new_v4() })),
        ),
        (
            "DELETE",
            &format!("/api/inbound-wg/peers/{}", Uuid::new_v4()),
            None,
        ),
    ];

    for (method, uri, body) in routes {
        let state = make_state(Arc::new(MockInboundWg {
            add_peer_outcome: AddPeerOutcome::Success,
        }));
        let req = request(method, uri, body, false);
        let resp = app(state).oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {uri} should reject an unauthenticated caller"
        );
    }
}
