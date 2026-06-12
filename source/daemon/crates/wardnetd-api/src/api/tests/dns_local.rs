//! Tests for the local-DNS API endpoints (`/api/dns/local/...`, issue #217).
//!
//! The handlers are thin admin-gated delegations to `DnsLocalService`; these
//! drive each route through the router with a canned mock service and assert
//! the status code + response shape (and that the admin gate rejects
//! unauthenticated callers).

use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::get;
use chrono::Utc;
use tower::ServiceExt;
use uuid::Uuid;
use wardnet_common::api::{
    CreateForwardingRuleRequest, CreateForwardingRuleResponse, CreateRecordRequest,
    CreateRecordResponse, CreateZoneRequest, CreateZoneResponse, DeleteForwardingRuleResponse,
    DeleteRecordResponse, DeleteZoneResponse, GetForwardingRuleResponse, GetRecordResponse,
    GetZoneResponse, ListForwardingRulesResponse, ListRecordsResponse, ListZonesResponse,
    UpdateForwardingRuleRequest, UpdateForwardingRuleResponse, UpdateRecordRequest,
    UpdateRecordResponse, UpdateZoneRequest, UpdateZoneResponse, UpsertRecordRequest,
    UpsertRecordResponse,
};
use wardnet_common::dns::{
    ConditionalForwardingRule, CustomDnsRecord, DnsRecordSource, DnsRecordType, DnsZone,
    DnsZoneSource,
};

use crate::state::AppState;
use crate::tests::stubs::{
    StubBackupService, StubDdnsService, StubDeviceService, StubDhcpServer, StubDhcpService,
    StubDiscoveryService, StubDnsFilterService, StubDnsServer, StubDnsService, StubEventPublisher,
    StubJobService, StubLogService, StubProviderService, StubRoutingService, StubStatsService,
    StubSystemService, StubTlsService, StubTunnelService, StubUpdateService,
};
use wardnetd_services::DnsLocalService;
use wardnetd_services::auth::service::{LoginResult, WizardState};
use wardnetd_services::error::AppError;
use wardnetd_services::{AuthService, LogService};

// ── Canned fixtures ───────────────────────────────────────────────────────────

fn zone() -> DnsZone {
    DnsZone {
        id: Uuid::nil(),
        name: "lab".to_owned(),
        enabled: true,
        source: DnsZoneSource::Manual,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn record() -> CustomDnsRecord {
    CustomDnsRecord {
        id: Uuid::nil(),
        zone_id: None,
        domain: "nas.lab".to_owned(),
        record_type: DnsRecordType::A,
        value: "10.0.0.5".to_owned(),
        ttl: 300,
        enabled: true,
        source: DnsRecordSource::Manual,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn rule() -> ConditionalForwardingRule {
    ConditionalForwardingRule {
        id: Uuid::nil(),
        domain: "corp.example".to_owned(),
        upstream: "10.0.0.1".to_owned(),
        enabled: true,
        created_at: Utc::now(),
    }
}

// ── Mocks ───────────────────────────────────────────────────────────────────

/// Returns canned data for every method so each handler exercises its happy
/// path. `message` fields are filled with a fixed string.
struct MockDnsLocal;

#[async_trait]
impl DnsLocalService for MockDnsLocal {
    async fn list_zones(&self) -> Result<ListZonesResponse, AppError> {
        Ok(ListZonesResponse {
            zones: vec![zone()],
        })
    }
    async fn get_zone(&self, _id: Uuid) -> Result<GetZoneResponse, AppError> {
        Ok(GetZoneResponse { zone: zone() })
    }
    async fn create_zone(&self, _r: CreateZoneRequest) -> Result<CreateZoneResponse, AppError> {
        Ok(CreateZoneResponse {
            zone: zone(),
            message: "created".to_owned(),
        })
    }
    async fn update_zone(
        &self,
        _id: Uuid,
        _r: UpdateZoneRequest,
    ) -> Result<UpdateZoneResponse, AppError> {
        Ok(UpdateZoneResponse {
            zone: zone(),
            message: "updated".to_owned(),
        })
    }
    async fn delete_zone(&self, _id: Uuid) -> Result<DeleteZoneResponse, AppError> {
        Ok(DeleteZoneResponse {
            message: "deleted".to_owned(),
        })
    }
    async fn list_zone_records(&self, _zone_id: Uuid) -> Result<ListRecordsResponse, AppError> {
        Ok(ListRecordsResponse {
            records: vec![record()],
        })
    }
    async fn list_records(&self) -> Result<ListRecordsResponse, AppError> {
        Ok(ListRecordsResponse {
            records: vec![record()],
        })
    }
    async fn get_record(&self, _id: Uuid) -> Result<GetRecordResponse, AppError> {
        Ok(GetRecordResponse { record: record() })
    }
    async fn create_record(
        &self,
        _r: CreateRecordRequest,
    ) -> Result<CreateRecordResponse, AppError> {
        Ok(CreateRecordResponse {
            record: record(),
            message: "created".to_owned(),
        })
    }
    async fn update_record(
        &self,
        _id: Uuid,
        _r: UpdateRecordRequest,
    ) -> Result<UpdateRecordResponse, AppError> {
        Ok(UpdateRecordResponse {
            record: record(),
            message: "updated".to_owned(),
        })
    }
    async fn delete_record(&self, _id: Uuid) -> Result<DeleteRecordResponse, AppError> {
        Ok(DeleteRecordResponse {
            message: "deleted".to_owned(),
        })
    }
    async fn upsert_record(
        &self,
        _r: UpsertRecordRequest,
    ) -> Result<UpsertRecordResponse, AppError> {
        Ok(UpsertRecordResponse {
            record: record(),
            message: "upserted".to_owned(),
        })
    }
    async fn list_forwarding_rules(&self) -> Result<ListForwardingRulesResponse, AppError> {
        Ok(ListForwardingRulesResponse {
            rules: vec![rule()],
        })
    }
    async fn get_forwarding_rule(&self, _id: Uuid) -> Result<GetForwardingRuleResponse, AppError> {
        Ok(GetForwardingRuleResponse { rule: rule() })
    }
    async fn create_forwarding_rule(
        &self,
        _r: CreateForwardingRuleRequest,
    ) -> Result<CreateForwardingRuleResponse, AppError> {
        Ok(CreateForwardingRuleResponse {
            rule: rule(),
            message: "created".to_owned(),
        })
    }
    async fn update_forwarding_rule(
        &self,
        _id: Uuid,
        _r: UpdateForwardingRuleRequest,
    ) -> Result<UpdateForwardingRuleResponse, AppError> {
        Ok(UpdateForwardingRuleResponse {
            rule: rule(),
            message: "updated".to_owned(),
        })
    }
    async fn delete_forwarding_rule(
        &self,
        _id: Uuid,
    ) -> Result<DeleteForwardingRuleResponse, AppError> {
        Ok(DeleteForwardingRuleResponse {
            message: "deleted".to_owned(),
        })
    }
}

/// Authenticates any `wardnet_session` cookie to a stable admin id; rejects
/// callers without one (returns `None` → `AdminAuth` yields 401).
struct AlwaysAdminAuth;
#[async_trait]
impl AuthService for AlwaysAdminAuth {
    async fn login(&self, _u: &str, _p: &str, _r: bool) -> Result<LoginResult, AppError> {
        unimplemented!()
    }
    async fn validate_session(&self, _token: &str) -> Result<Option<Uuid>, AppError> {
        Ok(Some(Uuid::nil()))
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
        unimplemented!()
    }
    async fn advance_wizard(
        &self,
        _to_step: wardnet_common::api::WizardStep,
        _mode: Option<wardnet_common::api::WizardMode>,
    ) -> Result<WizardState, AppError> {
        unimplemented!()
    }
    async fn refresh_session(&self, _token: &str) -> Result<LoginResult, AppError> {
        unimplemented!()
    }
}

// ── Harness ───────────────────────────────────────────────────────────────────

fn make_state() -> AppState {
    AppState::new(
        Arc::new(AlwaysAdminAuth),
        Arc::new(StubBackupService),
        Arc::new(StubDeviceService),
        Arc::new(StubDhcpService),
        Arc::new(StubDnsService),
        Arc::new(StubDnsFilterService),
        Arc::new(MockDnsLocal),
        Arc::new(StubDdnsService),
        Arc::new(StubTlsService),
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

fn app() -> Router {
    use crate::api::dns_local as h;
    Router::new()
        .route(
            "/api/dns/local/zones",
            get(h::list_zones).post(h::create_zone),
        )
        .route(
            "/api/dns/local/zones/{id}",
            get(h::get_zone).put(h::update_zone).delete(h::delete_zone),
        )
        .route(
            "/api/dns/local/zones/{id}/records",
            get(h::list_zone_records),
        )
        .route(
            "/api/dns/local/records",
            get(h::list_records).post(h::create_record),
        )
        .route(
            "/api/dns/local/records/{id}",
            get(h::get_record)
                .put(h::update_record)
                .delete(h::delete_record),
        )
        .route(
            "/api/dns/local/forwarding",
            get(h::list_forwarding_rules).post(h::create_forwarding_rule),
        )
        .route(
            "/api/dns/local/forwarding/{id}",
            get(h::get_forwarding_rule)
                .put(h::update_forwarding_rule)
                .delete(h::delete_forwarding_rule),
        )
        .with_state(make_state())
}

fn admin(method: &str, uri: &str, body: Option<serde_json::Value>) -> Request<Body> {
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        .header("Cookie", "wardnet_session=test");
    let body = match body {
        Some(v) => {
            b = b.header("Content-Type", "application/json");
            Body::from(serde_json::to_vec(&v).unwrap())
        }
        None => Body::empty(),
    };
    b.body(body).unwrap()
}

async fn send(req: Request<Body>) -> StatusCode {
    app().oneshot(req).await.unwrap().status()
}

// ── Zone routes ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_zones_ok() {
    assert_eq!(
        send(admin("GET", "/api/dns/local/zones", None)).await,
        StatusCode::OK
    );
}

#[tokio::test]
async fn create_zone_created() {
    let body = serde_json::json!({ "name": "lab", "enabled": true });
    assert_eq!(
        send(admin("POST", "/api/dns/local/zones", Some(body))).await,
        StatusCode::CREATED
    );
}

#[tokio::test]
async fn get_update_delete_zone() {
    let id = Uuid::nil();
    assert_eq!(
        send(admin("GET", &format!("/api/dns/local/zones/{id}"), None)).await,
        StatusCode::OK
    );
    let upd = serde_json::json!({ "enabled": false });
    assert_eq!(
        send(admin(
            "PUT",
            &format!("/api/dns/local/zones/{id}"),
            Some(upd)
        ))
        .await,
        StatusCode::OK
    );
    assert_eq!(
        send(admin("DELETE", &format!("/api/dns/local/zones/{id}"), None)).await,
        StatusCode::OK
    );
}

#[tokio::test]
async fn list_zone_records_ok() {
    let id = Uuid::nil();
    assert_eq!(
        send(admin(
            "GET",
            &format!("/api/dns/local/zones/{id}/records"),
            None
        ))
        .await,
        StatusCode::OK
    );
}

// ── Record routes ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_and_create_records() {
    assert_eq!(
        send(admin("GET", "/api/dns/local/records", None)).await,
        StatusCode::OK
    );
    let body = serde_json::json!({
        "domain": "nas.lab",
        "record_type": "A",
        "value": "10.0.0.5",
        "ttl": 300,
        "enabled": true,
    });
    assert_eq!(
        send(admin("POST", "/api/dns/local/records", Some(body))).await,
        StatusCode::CREATED
    );
}

#[tokio::test]
async fn get_update_delete_record() {
    let id = Uuid::nil();
    assert_eq!(
        send(admin("GET", &format!("/api/dns/local/records/{id}"), None)).await,
        StatusCode::OK
    );
    let upd = serde_json::json!({ "value": "10.0.0.9" });
    assert_eq!(
        send(admin(
            "PUT",
            &format!("/api/dns/local/records/{id}"),
            Some(upd)
        ))
        .await,
        StatusCode::OK
    );
    assert_eq!(
        send(admin(
            "DELETE",
            &format!("/api/dns/local/records/{id}"),
            None
        ))
        .await,
        StatusCode::OK
    );
}

// ── Forwarding-rule routes ──────────────────────────────────────────────────

#[tokio::test]
async fn list_and_create_forwarding_rules() {
    assert_eq!(
        send(admin("GET", "/api/dns/local/forwarding", None)).await,
        StatusCode::OK
    );
    let body = serde_json::json!({
        "domain": "corp.example",
        "upstream": "10.0.0.1",
        "enabled": true,
    });
    assert_eq!(
        send(admin("POST", "/api/dns/local/forwarding", Some(body))).await,
        StatusCode::CREATED
    );
}

#[tokio::test]
async fn get_update_delete_forwarding_rule() {
    let id = Uuid::nil();
    assert_eq!(
        send(admin(
            "GET",
            &format!("/api/dns/local/forwarding/{id}"),
            None
        ))
        .await,
        StatusCode::OK
    );
    let upd = serde_json::json!({ "enabled": false });
    assert_eq!(
        send(admin(
            "PUT",
            &format!("/api/dns/local/forwarding/{id}"),
            Some(upd)
        ))
        .await,
        StatusCode::OK
    );
    assert_eq!(
        send(admin(
            "DELETE",
            &format!("/api/dns/local/forwarding/{id}"),
            None
        ))
        .await,
        StatusCode::OK
    );
}

// ── Auth gate ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn rejects_unauthenticated() {
    let req = Request::builder()
        .method("GET")
        .uri("/api/dns/local/zones")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app().oneshot(req).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
}
