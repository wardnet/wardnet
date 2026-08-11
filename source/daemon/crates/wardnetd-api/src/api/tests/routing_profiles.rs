//! Tests for the routing-profile API endpoints (`/api/routing/...`, issue #241).
//!
//! Unlike the thin-delegation tests elsewhere, these drive the real
//! [`RoutingProfileServiceImpl`] over an in-memory database, so the router, the
//! service validation, and the repository are all exercised end to end.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{get, put};
use serde_json::json;
use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use tower::ServiceExt;
use uuid::Uuid;

use crate::state::AppState;
use crate::tests::stubs::{
    AlwaysSessionAuth, StubBackupService, StubDdnsService, StubDeviceService, StubDhcpServer,
    StubDhcpService, StubDiscoveryService, StubDnsFilterService, StubDnsLocalService,
    StubDnsServer, StubDnsService, StubEventPublisher, StubJobService, StubLogService,
    StubNetworkZoneService, StubProviderService, StubRoutingService, StubRuleRequestService,
    StubStatsService, StubSystemService, StubTlsService, StubTunnelService, StubUpdateService,
    StubZoneExceptionService,
};
use wardnet_common::routing_profile::RoutingProfile;
use wardnetd_data::repository::SqliteRoutingProfileRepository;
use wardnetd_services::LogService;
use wardnetd_services::routing_profile::RoutingProfileServiceImpl;

const DEVICE_ID: &str = "00000000-0000-0000-0000-0000000000a1";

async fn seeded_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../wardnetd-data/migrations")
        .run(&pool)
        .await
        .unwrap();
    // A device the assignment endpoint can bind profiles to (FK target).
    sqlx::query(
        "INSERT INTO devices (id, mac, last_ip, device_type, first_seen, last_seen, zone_id) \
         VALUES (?, 'aa:bb:cc:dd:ee:a1', '192.168.1.42', 'unknown', ?, ?, \
                 '00000000-0000-0000-0000-000000000201')",
    )
    .bind(DEVICE_ID)
    .bind("2026-07-20T00:00:00Z")
    .bind("2026-07-20T00:00:00Z")
    .execute(&pool)
    .await
    .unwrap();
    pool
}

async fn app() -> Router {
    use crate::api::routing_profiles as h;

    let pool = seeded_pool().await;
    let repo = Arc::new(SqliteRoutingProfileRepository::new(pool));
    let (tx, _rx) = tokio::sync::mpsc::channel(16);
    let service = Arc::new(RoutingProfileServiceImpl::new(
        repo,
        Arc::new(StubTunnelService),
        Arc::new(crate::tests::stubs::StubDeviceService),
        tx,
    ));

    let state = AppState::new(
        Arc::new(AlwaysSessionAuth),
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
        StubJobService::new_arc(),
        Arc::new(StubStatsService),
        Arc::new(StubRuleRequestService),
        Arc::new(StubZoneExceptionService),
    )
    .with_routing_profile_service(service);

    Router::new()
        .route(
            "/api/routing/profiles",
            get(h::list_profiles).post(h::create_profile),
        )
        .route(
            "/api/routing/profiles/{id}",
            get(h::get_profile)
                .put(h::update_profile)
                .delete(h::delete_profile),
        )
        .route(
            "/api/routing/profiles/{id}/rules",
            get(h::list_rules).post(h::create_rule),
        )
        .route(
            "/api/routing/rules/{id}",
            put(h::update_rule).delete(h::delete_rule),
        )
        .route(
            "/api/routing/devices/{device_id}/profiles",
            get(h::get_device_profiles).put(h::set_device_profiles),
        )
        .route(
            "/api/routing/profiles/{id}/devices",
            get(h::get_profile_devices),
        )
        // Resolve the session cookie into the admin auth-context and propagate
        // it into the task-local the service's `require_admin` reads.
        .layer(wardnetd_services::auth_context::AuthContextLayer)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::api::middleware::resolve_auth_context,
        ))
        .with_state(state)
}

const COOKIE: &str = "wardnet_session=test";

fn post(path: &str, body: &serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .header("Cookie", COOKIE)
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn put_req(path: &str, body: &serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("PUT")
        .uri(path)
        .header("content-type", "application/json")
        .header("Cookie", COOKIE)
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn get_req(path: &str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .header("Cookie", COOKIE)
        .body(Body::empty())
        .unwrap()
}

fn delete_req(path: &str) -> Request<Body> {
    Request::builder()
        .method("DELETE")
        .uri(path)
        .header("Cookie", COOKIE)
        .body(Body::empty())
        .unwrap()
}

async fn body_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn create_profile(app: &Router, name: &str) -> RoutingProfile {
    let resp = app
        .clone()
        .oneshot(post("/api/routing/profiles", &json!({ "name": name })))
        .await
        .unwrap();
    let st = resp.status();
    let v = body_json(resp).await;
    assert_eq!(st, StatusCode::CREATED, "body: {v}");
    serde_json::from_value(v["profile"].clone()).unwrap()
}

#[tokio::test]
async fn profile_create_list_rename_delete() {
    let app = app().await;

    let profile = create_profile(&app, "Streaming").await;

    let resp = app
        .clone()
        .oneshot(get_req("/api/routing/profiles"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = body_json(resp).await;
    assert_eq!(v["profiles"].as_array().unwrap().len(), 1);

    let resp = app
        .clone()
        .oneshot(put_req(
            &format!("/api/routing/profiles/{}", profile.id),
            &json!({ "name": "Streaming UK" }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app
        .clone()
        .oneshot(delete_req(&format!("/api/routing/profiles/{}", profile.id)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn empty_profile_name_is_rejected() {
    let app = app().await;
    let resp = app
        .oneshot(post("/api/routing/profiles", &json!({ "name": "  " })))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn duplicate_profile_name_conflicts() {
    let app = app().await;
    create_profile(&app, "Dup").await;
    let resp = app
        .oneshot(post("/api/routing/profiles", &json!({ "name": "Dup" })))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn direct_rule_create_list_and_bad_pattern() {
    let app = app().await;
    let profile = create_profile(&app, "Carveouts").await;
    let rules_path = format!("/api/routing/profiles/{}/rules", profile.id);

    // A direct carve-out never touches the tunnel service.
    let resp = app
        .clone()
        .oneshot(post(
            &rules_path,
            &json!({
                "pattern": "bank.example.com",
                "target": { "type": "direct" },
                "enabled": true
            }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    let resp = app.clone().oneshot(get_req(&rules_path)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = body_json(resp).await;
    assert_eq!(v["rules"].as_array().unwrap().len(), 1);

    // A malformed pattern is rejected before it reaches the database.
    let resp = app
        .oneshot(post(
            &rules_path,
            &json!({
                "pattern": "a.*.com",
                "target": { "type": "direct" },
                "enabled": true
            }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn device_assignment_roundtrip_in_priority_order() {
    let app = app().await;
    let first = create_profile(&app, "first").await;
    let second = create_profile(&app, "second").await;
    let path = format!("/api/routing/devices/{DEVICE_ID}/profiles");

    let resp = app
        .clone()
        .oneshot(put_req(
            &path,
            &json!({ "profile_ids": [second.id, first.id] }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app.clone().oneshot(get_req(&path)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = body_json(resp).await;
    let ids: Vec<Uuid> = serde_json::from_value(v["profile_ids"].clone()).unwrap();
    assert_eq!(ids, vec![second.id, first.id]);
}

#[tokio::test]
async fn profile_devices_lists_assigned_devices() {
    let app = app().await;
    let profile = create_profile(&app, "Assigned").await;
    let dev_path = format!("/api/routing/profiles/{}/devices", profile.id);

    // Before any assignment the reverse list is empty.
    let resp = app.clone().oneshot(get_req(&dev_path)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = body_json(resp).await;
    assert!(v["device_ids"].as_array().unwrap().is_empty());

    // Assign the profile to the seeded device.
    let assign_path = format!("/api/routing/devices/{DEVICE_ID}/profiles");
    let resp = app
        .clone()
        .oneshot(put_req(
            &assign_path,
            &json!({ "profile_ids": [profile.id] }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Now the device shows up under the profile's "used by" list.
    let resp = app.clone().oneshot(get_req(&dev_path)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = body_json(resp).await;
    let ids: Vec<Uuid> = serde_json::from_value(v["device_ids"].clone()).unwrap();
    assert_eq!(ids, vec![DEVICE_ID.parse::<Uuid>().unwrap()]);

    // An unknown profile is a 404, not an empty list.
    let resp = app
        .oneshot(get_req(&format!(
            "/api/routing/profiles/{}/devices",
            Uuid::new_v4()
        )))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn assigning_unknown_profile_is_rejected() {
    let app = app().await;
    let path = format!("/api/routing/devices/{DEVICE_ID}/profiles");
    let resp = app
        .oneshot(put_req(&path, &json!({ "profile_ids": [Uuid::new_v4()] })))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_single_profile_and_empty_update_is_rejected() {
    let app = app().await;
    let profile = create_profile(&app, "Fetch Me").await;

    // GET /profiles/{id} returns the one profile.
    let resp = app
        .clone()
        .oneshot(get_req(&format!("/api/routing/profiles/{}", profile.id)))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = body_json(resp).await;
    assert_eq!(v["profile"]["name"], "Fetch Me");

    // A PUT that changes nothing is a bad request, not a silent no-op.
    let resp = app
        .oneshot(put_req(
            &format!("/api/routing/profiles/{}", profile.id),
            &json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rule_update_then_delete() {
    let app = app().await;
    let profile = create_profile(&app, "Editable").await;
    let rules_path = format!("/api/routing/profiles/{}/rules", profile.id);

    // Create a direct rule and capture its id.
    let resp = app
        .clone()
        .oneshot(post(
            &rules_path,
            &json!({
                "pattern": "old.example.com",
                "target": { "type": "direct" },
                "enabled": true
            }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);
    let v = body_json(resp).await;
    let rule_id = v["rule"]["id"].as_str().unwrap().to_owned();

    // Update the pattern + disable it.
    let resp = app
        .clone()
        .oneshot(put_req(
            &format!("/api/routing/rules/{rule_id}"),
            &json!({ "pattern": "new.example.com", "enabled": false }),
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = body_json(resp).await;
    assert_eq!(v["rule"]["pattern"], "new.example.com");
    assert_eq!(v["rule"]["enabled"], false);

    // Delete it; the profile's rule list is then empty.
    let resp = app
        .clone()
        .oneshot(delete_req(&format!("/api/routing/rules/{rule_id}")))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app.oneshot(get_req(&rules_path)).await.unwrap();
    let v = body_json(resp).await;
    assert!(v["rules"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn duplicate_rule_pattern_in_profile_conflicts() {
    let app = app().await;
    let profile = create_profile(&app, "Dup Rules").await;
    let rules_path = format!("/api/routing/profiles/{}/rules", profile.id);
    let body = json!({
        "pattern": "dup.example.com",
        "target": { "type": "direct" },
        "enabled": true
    });

    let resp = app.clone().oneshot(post(&rules_path, &body)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // The same pattern in the same profile is a conflict, not a second row.
    let resp = app.oneshot(post(&rules_path, &body)).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
}
