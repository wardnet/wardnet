//! Tests for the `/api/dns/filter/...` HTTP handlers.
//!
//! Wires each route directly against a `MockDnsFilterService` so the
//! handler's request-parsing, admin gate, and response-shape behaviour
//! is exercised without spinning up the full app builder.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::routing::{delete, get, post, put};
use chrono::Utc;
use tower::ServiceExt;
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
use wardnet_common::auth::AuthContext;
use wardnet_common::dns::{AllowlistEntry, Blocklist, CustomFilterRule, FilterAction};
use wardnet_common::dns_filter::{DeviceDnsFilterSettings, DnsFilterConfig, DnsFilterProfile};
use wardnet_common::jobs::JobDispatchedResponse;

use crate::state::AppState;
use crate::tests::stubs::{
    StubDeviceService, StubDhcpServer, StubDhcpService, StubDiscoveryService, StubDnsServer,
    StubDnsService, StubEventPublisher, StubLogService, StubNetworkZoneService,
    StubProviderService, StubRoutingService, StubSystemService, StubTunnelService,
};
use uuid::Uuid;
use wardnet_common::auth::{AuthenticatedUser, UserRole};
use wardnet_test_support::principal;
use wardnetd_services::LogService;
use wardnetd_services::auth::service::LoginResult;
use wardnetd_services::auth::{CurrentUser, LoginAttempt};
use wardnetd_services::dns_filter::service::CheckOutcome;
use wardnetd_services::error::AppError;
use wardnetd_services::{AuthService, DnsFilterService};

// ---------------------------------------------------------------------------
// Mock auth: every request looks like an admin session.
// ---------------------------------------------------------------------------

struct MockAuthService;

#[async_trait]
impl AuthService for MockAuthService {
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
        Ok(true)
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

// ---------------------------------------------------------------------------
// MockDnsFilterService — every read returns canned data; every write echoes
// back. The handlers just plumb (req, res); these tests are about the
// HTTP-layer wiring (route paths, JSON shape, auth gate).
// ---------------------------------------------------------------------------

const PROFILE_ID: Uuid = Uuid::from_u128(0x100);
const PROFILE_ID_B: Uuid = Uuid::from_u128(0x101);
const BLOCKLIST_ID: Uuid = Uuid::from_u128(0x200);
const RULE_ID: Uuid = Uuid::from_u128(0x300);
const ENTRY_ID: Uuid = Uuid::from_u128(0x400);
const DEVICE_ID: Uuid = Uuid::from_u128(0x500);

fn sample_profile() -> DnsFilterProfile {
    DnsFilterProfile {
        id: PROFILE_ID,
        name: "Sample".into(),
        description: None,
        builtin: false,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn sample_blocklist() -> Blocklist {
    Blocklist {
        id: BLOCKLIST_ID,
        profile_id: PROFILE_ID,
        name: "Sample blocklist".into(),
        url: "http://example.test/list.txt".into(),
        enabled: true,
        entry_count: 0,
        last_updated: None,
        cron_schedule: "0 3 * * *".into(),
        last_error: None,
        last_error_at: None,
        consecutive_failures: 0,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn sample_rule() -> CustomFilterRule {
    CustomFilterRule {
        id: RULE_ID,
        profile_id: PROFILE_ID,
        rule_text: "||example.com^".into(),
        enabled: true,
        comment: None,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn sample_allowlist() -> AllowlistEntry {
    AllowlistEntry {
        id: ENTRY_ID,
        profile_id: PROFILE_ID,
        domain: "safe.example.com".into(),
        reason: None,
        created_at: Utc::now(),
    }
}

fn sample_settings() -> DeviceDnsFilterSettings {
    DeviceDnsFilterSettings {
        device_id: DEVICE_ID,
        enabled: true,
        profile_ids: vec![PROFILE_ID],
        updated_at: Utc::now(),
    }
}

struct MockDnsFilterService;

#[async_trait]
impl DnsFilterService for MockDnsFilterService {
    async fn check(
        &self,
        _domain: &str,
        _qtype: hickory_proto::rr::RecordType,
        _client: IpAddr,
    ) -> CheckOutcome {
        CheckOutcome {
            action: FilterAction::Pass,
            would_have_blocked: false,
        }
    }
    async fn rebuild_all(&self) -> Result<(), AppError> {
        Ok(())
    }
    async fn list_profiles(&self) -> Result<ListProfilesResponse, AppError> {
        Ok(ListProfilesResponse {
            profiles: vec![sample_profile()],
        })
    }
    async fn get_profile(&self, _id: Uuid) -> Result<GetProfileResponse, AppError> {
        Ok(GetProfileResponse {
            profile: sample_profile(),
        })
    }
    async fn create_profile(
        &self,
        _r: CreateProfileRequest,
    ) -> Result<CreateProfileResponse, AppError> {
        Ok(CreateProfileResponse {
            profile: sample_profile(),
            message: "ok".into(),
        })
    }
    async fn update_profile(
        &self,
        _id: Uuid,
        _r: UpdateProfileRequest,
    ) -> Result<UpdateProfileResponse, AppError> {
        Ok(UpdateProfileResponse {
            profile: sample_profile(),
            message: "ok".into(),
        })
    }
    async fn delete_profile(&self, _id: Uuid) -> Result<DeleteProfileResponse, AppError> {
        Ok(DeleteProfileResponse {
            message: "deleted".into(),
        })
    }
    async fn list_blocklists(&self, _profile_id: Uuid) -> Result<ListBlocklistsResponse, AppError> {
        Ok(ListBlocklistsResponse {
            blocklists: vec![sample_blocklist()],
        })
    }
    async fn create_blocklist(
        &self,
        _profile_id: Uuid,
        _r: CreateBlocklistRequest,
    ) -> Result<CreateBlocklistResponse, AppError> {
        Ok(CreateBlocklistResponse {
            blocklist: sample_blocklist(),
            message: "ok".into(),
        })
    }
    async fn update_blocklist(
        &self,
        _profile_id: Uuid,
        _id: Uuid,
        _r: UpdateBlocklistRequest,
    ) -> Result<UpdateBlocklistResponse, AppError> {
        Ok(UpdateBlocklistResponse {
            blocklist: sample_blocklist(),
            message: "ok".into(),
        })
    }
    async fn delete_blocklist(
        &self,
        _profile_id: Uuid,
        _id: Uuid,
    ) -> Result<DeleteBlocklistResponse, AppError> {
        Ok(DeleteBlocklistResponse {
            message: "deleted".into(),
        })
    }
    async fn refresh_blocklist(
        &self,
        _profile_id: Uuid,
        _id: Uuid,
    ) -> Result<JobDispatchedResponse, AppError> {
        Ok(JobDispatchedResponse {
            job_id: Uuid::new_v4(),
        })
    }
    async fn list_allowlist(&self, _profile_id: Uuid) -> Result<ListAllowlistResponse, AppError> {
        Ok(ListAllowlistResponse {
            entries: vec![sample_allowlist()],
        })
    }
    async fn create_allowlist_entry(
        &self,
        _profile_id: Uuid,
        _r: CreateAllowlistRequest,
    ) -> Result<CreateAllowlistResponse, AppError> {
        Ok(CreateAllowlistResponse {
            entry: sample_allowlist(),
            message: "ok".into(),
        })
    }
    async fn delete_allowlist_entry(
        &self,
        _profile_id: Uuid,
        _id: Uuid,
    ) -> Result<DeleteAllowlistResponse, AppError> {
        Ok(DeleteAllowlistResponse {
            message: "deleted".into(),
        })
    }
    async fn list_custom_rules(
        &self,
        _profile_id: Uuid,
    ) -> Result<ListFilterRulesResponse, AppError> {
        Ok(ListFilterRulesResponse {
            rules: vec![sample_rule()],
        })
    }
    async fn create_custom_rule(
        &self,
        _profile_id: Uuid,
        _r: CreateFilterRuleRequest,
    ) -> Result<CreateFilterRuleResponse, AppError> {
        Ok(CreateFilterRuleResponse {
            rule: sample_rule(),
            message: "ok".into(),
        })
    }
    async fn update_custom_rule(
        &self,
        _profile_id: Uuid,
        _id: Uuid,
        _r: UpdateFilterRuleRequest,
    ) -> Result<UpdateFilterRuleResponse, AppError> {
        Ok(UpdateFilterRuleResponse {
            rule: sample_rule(),
            message: "ok".into(),
        })
    }
    async fn delete_custom_rule(
        &self,
        _profile_id: Uuid,
        _id: Uuid,
    ) -> Result<DeleteFilterRuleResponse, AppError> {
        Ok(DeleteFilterRuleResponse {
            message: "deleted".into(),
        })
    }
    async fn list_device_settings(
        &self,
        _params: ListDeviceFilterSettingsParams,
    ) -> Result<ListDeviceFilterSettingsResponse, AppError> {
        Ok(ListDeviceFilterSettingsResponse {
            devices: vec![sample_settings()],
        })
    }
    async fn get_device_settings(
        &self,
        _device_id: Uuid,
    ) -> Result<GetDeviceFilterSettingsResponse, AppError> {
        Ok(GetDeviceFilterSettingsResponse {
            settings: sample_settings(),
        })
    }
    async fn update_device_settings(
        &self,
        _device_id: Uuid,
        _r: UpdateDeviceFilterSettingsRequest,
    ) -> Result<UpdateDeviceFilterSettingsResponse, AppError> {
        Ok(UpdateDeviceFilterSettingsResponse {
            settings: sample_settings(),
            message: "ok".into(),
        })
    }
    async fn get_filter_config(&self) -> Result<DnsFilterConfigResponse, AppError> {
        Ok(DnsFilterConfigResponse {
            config: DnsFilterConfig {
                enabled: true,
                default_profile_ids: vec![PROFILE_ID],
            },
            filters_ready: true,
            pending_blocklists: 0,
        })
    }
    async fn update_filter_config(
        &self,
        _r: UpdateDnsFilterConfigRequest,
    ) -> Result<DnsFilterConfigResponse, AppError> {
        Ok(DnsFilterConfigResponse {
            config: DnsFilterConfig {
                enabled: true,
                default_profile_ids: vec![PROFILE_ID],
            },
            filters_ready: true,
            pending_blocklists: 0,
        })
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn connect_info() -> ConnectInfo<SocketAddr> {
    ConnectInfo(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 12345))
}

fn build_state() -> AppState {
    AppState::new(
        Arc::new(MockAuthService),
        Arc::new(crate::tests::stubs::StubBackupService),
        Arc::new(StubDeviceService),
        Arc::new(StubDhcpService),
        Arc::new(StubDnsService),
        Arc::new(MockDnsFilterService),
        Arc::new(crate::tests::stubs::StubDnsLocalService),
        Arc::new(crate::tests::stubs::StubDdnsService),
        Arc::new(crate::tests::stubs::StubTlsService),
        Arc::new(StubDiscoveryService),
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
        Arc::new(crate::tests::stubs::StubRuleRequestService),
        Arc::new(crate::tests::stubs::StubZoneExceptionService),
    )
}

/// Wire each `/api/dns/filter/...` route by hand against the mock state.
/// This is the same shape the production router builds (via
/// `register()` in `api/dns_filter.rs`) but kept inline so the tests
/// don't drag in the entire app builder + middleware tower.
fn router(state: AppState) -> Router {
    use crate::api::dns_filter::*;

    Router::new()
        .route(
            "/api/dns/filter/profiles",
            get(list_profiles).post(create_profile),
        )
        .route(
            "/api/dns/filter/profiles/{profile_id}",
            get(get_profile).put(update_profile).delete(delete_profile),
        )
        .route(
            "/api/dns/filter/profiles/{profile_id}/blocklists",
            get(list_blocklists).post(create_blocklist),
        )
        .route(
            "/api/dns/filter/profiles/{profile_id}/blocklists/{id}",
            put(update_blocklist).delete(delete_blocklist),
        )
        .route(
            "/api/dns/filter/profiles/{profile_id}/blocklists/{id}/refresh",
            post(refresh_blocklist),
        )
        .route(
            "/api/dns/filter/profiles/{profile_id}/allowlist",
            get(list_allowlist).post(create_allowlist_entry),
        )
        .route(
            "/api/dns/filter/profiles/{profile_id}/allowlist/{id}",
            delete(delete_allowlist_entry),
        )
        .route(
            "/api/dns/filter/profiles/{profile_id}/rules",
            get(list_custom_rules).post(create_custom_rule),
        )
        .route(
            "/api/dns/filter/profiles/{profile_id}/rules/{id}",
            put(update_custom_rule).delete(delete_custom_rule),
        )
        .route("/api/dns/filter/devices", get(list_device_settings))
        .route(
            "/api/dns/filter/devices/{device_id}",
            get(get_device_settings).put(update_device_settings),
        )
        .route(
            "/api/dns/filter/config",
            get(get_filter_config).put(update_filter_config),
        )
        .with_state(state)
}

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
    let json = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    (status, json)
}

async fn post_json(app: Router, uri: &str, body: &str) -> (StatusCode, serde_json::Value) {
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("Content-Type", "application/json")
                .header("Cookie", "wardnet_session=valid-token")
                .extension(connect_info())
                .body(Body::from(body.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let body = axum::body::to_bytes(resp.into_body(), 16384).await.unwrap();
    let json = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    (status, json)
}

async fn put_json(app: Router, uri: &str, body: &str) -> (StatusCode, serde_json::Value) {
    let resp = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(uri)
                .header("Content-Type", "application/json")
                .header("Cookie", "wardnet_session=valid-token")
                .extension(connect_info())
                .body(Body::from(body.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let body = axum::body::to_bytes(resp.into_body(), 16384).await.unwrap();
    let json = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    (status, json)
}

async fn delete_json(app: Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
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
    let json = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    (status, json)
}

// ---------------------------------------------------------------------------
// Profile routes
// ---------------------------------------------------------------------------

fn pid() -> String {
    PROFILE_ID.to_string()
}

#[tokio::test]
async fn list_profiles_returns_ok_with_array() {
    let app = router(build_state());
    let (status, json) = get_json(app, "/api/dns/filter/profiles").await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["profiles"].is_array());
    assert_eq!(json["profiles"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn create_profile_returns_201() {
    let app = router(build_state());
    let (status, json) = post_json(app, "/api/dns/filter/profiles", r#"{"name":"x"}"#).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(json["profile"]["id"], PROFILE_ID.to_string());
}

#[tokio::test]
async fn get_profile_returns_ok() {
    let app = router(build_state());
    let (status, json) = get_json(app, &format!("/api/dns/filter/profiles/{}", pid())).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["profile"]["id"], PROFILE_ID.to_string());
}

#[tokio::test]
async fn update_profile_returns_ok() {
    let app = router(build_state());
    let (status, _) = put_json(
        app,
        &format!("/api/dns/filter/profiles/{}", pid()),
        r#"{"name":"x"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn delete_profile_returns_ok() {
    let app = router(build_state());
    let (status, _) = delete_json(app, &format!("/api/dns/filter/profiles/{}", pid())).await;
    assert_eq!(status, StatusCode::OK);
}

// ---------------------------------------------------------------------------
// Blocklist routes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_blocklists_returns_ok() {
    let app = router(build_state());
    let (status, json) = get_json(
        app,
        &format!("/api/dns/filter/profiles/{}/blocklists", pid()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["blocklists"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn create_blocklist_returns_201() {
    let app = router(build_state());
    let body = r#"{"name":"a","url":"http://x","cron_schedule":"0 3 * * *","enabled":true}"#;
    let (status, _) = post_json(
        app,
        &format!("/api/dns/filter/profiles/{}/blocklists", pid()),
        body,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
}

#[tokio::test]
async fn update_blocklist_returns_ok() {
    let app = router(build_state());
    let (status, _) = put_json(
        app,
        &format!(
            "/api/dns/filter/profiles/{}/blocklists/{}",
            pid(),
            BLOCKLIST_ID
        ),
        r#"{"enabled":false}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn delete_blocklist_returns_ok() {
    let app = router(build_state());
    let (status, _) = delete_json(
        app,
        &format!(
            "/api/dns/filter/profiles/{}/blocklists/{}",
            pid(),
            BLOCKLIST_ID
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn refresh_blocklist_returns_202_with_job_id() {
    let app = router(build_state());
    let (status, json) = post_json(
        app,
        &format!(
            "/api/dns/filter/profiles/{}/blocklists/{}/refresh",
            pid(),
            BLOCKLIST_ID
        ),
        "",
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert!(json["job_id"].is_string());
}

// ---------------------------------------------------------------------------
// Allowlist routes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_allowlist_returns_ok() {
    let app = router(build_state());
    let (status, json) = get_json(
        app,
        &format!("/api/dns/filter/profiles/{}/allowlist", pid()),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["entries"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn create_allowlist_returns_201() {
    let app = router(build_state());
    let (status, _) = post_json(
        app,
        &format!("/api/dns/filter/profiles/{}/allowlist", pid()),
        r#"{"domain":"safe.example.com"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
}

#[tokio::test]
async fn delete_allowlist_returns_ok() {
    let app = router(build_state());
    let (status, _) = delete_json(
        app,
        &format!("/api/dns/filter/profiles/{}/allowlist/{}", pid(), ENTRY_ID),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

// ---------------------------------------------------------------------------
// Custom rule routes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_rules_returns_ok() {
    let app = router(build_state());
    let (status, _) = get_json(app, &format!("/api/dns/filter/profiles/{}/rules", pid())).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn create_rule_returns_201() {
    let app = router(build_state());
    let (status, _) = post_json(
        app,
        &format!("/api/dns/filter/profiles/{}/rules", pid()),
        r#"{"rule_text":"||example.com^","enabled":true}"#,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
}

#[tokio::test]
async fn update_rule_returns_ok() {
    let app = router(build_state());
    let (status, _) = put_json(
        app,
        &format!("/api/dns/filter/profiles/{}/rules/{}", pid(), RULE_ID),
        r#"{"enabled":false}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn delete_rule_returns_ok() {
    let app = router(build_state());
    let (status, _) = delete_json(
        app,
        &format!("/api/dns/filter/profiles/{}/rules/{}", pid(), RULE_ID),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

// ---------------------------------------------------------------------------
// Device settings routes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_device_settings_returns_ok() {
    let app = router(build_state());
    let (status, json) = get_json(app, "/api/dns/filter/devices").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["devices"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn list_device_settings_with_enabled_filter_returns_ok() {
    let app = router(build_state());
    let (status, _) = get_json(app, "/api/dns/filter/devices?enabled=false").await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn get_device_settings_returns_ok() {
    let app = router(build_state());
    let (status, _) = get_json(app, &format!("/api/dns/filter/devices/{DEVICE_ID}")).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn update_device_settings_returns_ok() {
    let app = router(build_state());
    let (status, _) = put_json(
        app,
        &format!("/api/dns/filter/devices/{DEVICE_ID}"),
        r#"{"enabled":true,"profile_ids":[]}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

// ---------------------------------------------------------------------------
// Config routes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_config_returns_ok() {
    let app = router(build_state());
    let (status, json) = get_json(app, "/api/dns/filter/config").await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["config"]["enabled"].as_bool().unwrap());
    let ids = json["config"]["default_profile_ids"].as_array().unwrap();
    assert_eq!(ids.len(), 1);
    assert_eq!(ids[0].as_str().unwrap(), PROFILE_ID.to_string());
}

#[tokio::test]
async fn update_config_returns_ok() {
    let app = router(build_state());
    let (status, _) = put_json(app, "/api/dns/filter/config", r#"{"enabled":false}"#).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn update_config_accepts_multi_default_ids() {
    let app = router(build_state());
    let body = format!(r#"{{"default_profile_ids":["{PROFILE_ID}","{PROFILE_ID_B}"]}}"#);
    let (status, _) = put_json(app, "/api/dns/filter/config", &body).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn update_config_accepts_empty_default_ids() {
    let app = router(build_state());
    let (status, _) = put_json(
        app,
        "/api/dns/filter/config",
        r#"{"default_profile_ids":[]}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

// ---------------------------------------------------------------------------
// Auth gate
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_profiles_unauthorized_without_session() {
    let app = router(build_state());
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/api/dns/filter/profiles")
                .extension(connect_info())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// Silence unused import warnings for AuthContext, etc. If a future test
// needs them they're already in scope.
#[allow(dead_code)]
fn _imports_used() {
    let _: AuthContext = AuthContext::Anonymous;
}
