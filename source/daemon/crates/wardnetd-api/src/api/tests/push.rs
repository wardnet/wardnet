//! Tests for the push notification API endpoints.
//! GET /api/push/vapid-public-key, POST/DELETE /api/push/subscriptions.

use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post};
use tower::ServiceExt;
use wardnet_common::api::WebPushSubscription;
use wardnet_common::event::WardnetEvent;
use wardnetd_data::repository::StoredNotification;

use crate::state::AppState;
use crate::tests::stubs::{AlwaysSessionAuth, test_app_state};
use wardnetd_services::error::AppError;
use wardnetd_services::push::PushService;

/// Records nothing — just returns canned success so the handler wiring is
/// exercised end-to-end.
struct MockPushService;

#[async_trait]
impl PushService for MockPushService {
    async fn vapid_public_key(&self) -> Result<String, AppError> {
        Ok("BTestApplicationServerKey".to_owned())
    }
    async fn subscribe(&self, _sub: WebPushSubscription) -> Result<(), AppError> {
        Ok(())
    }
    async fn unsubscribe(&self, _endpoint: Option<String>) -> Result<(), AppError> {
        Ok(())
    }
    async fn handle_event(&self, _event: &WardnetEvent) -> Result<(), AppError> {
        Ok(())
    }
    async fn recent_notifications(&self, limit: u32) -> Result<Vec<StoredNotification>, AppError> {
        Ok(vec![StoredNotification {
            id: "n1".to_owned(),
            kind: "new_device_quarantined".to_owned(),
            title: "New device".to_owned(),
            body: "New device Phone joined, in Guest. Approve in the app.".to_owned(),
            url: Some("/devices".to_owned()),
            subject_id: Some("device-1".to_owned()),
            created_at: "2026-07-03T00:00:00Z".to_owned(),
        }]
        .into_iter()
        .take(limit as usize)
        .collect())
    }
    async fn clear_notifications(&self) -> Result<(), AppError> {
        Ok(())
    }
}

fn router(state: AppState) -> Router {
    use crate::api::push::{
        clear_notifications, get_vapid_public_key, list_notifications, subscribe, unsubscribe,
    };

    Router::new()
        .route("/api/push/vapid-public-key", get(get_vapid_public_key))
        .route(
            "/api/push/subscriptions",
            post(subscribe).delete(unsubscribe),
        )
        .route(
            "/api/push/notifications",
            get(list_notifications).delete(clear_notifications),
        )
        .with_state(state)
}

async fn send(
    app: Router,
    method: &str,
    uri: &str,
    body: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    let builder = Request::builder().method(method).uri(uri);
    let req = if let Some(b) = body {
        builder
            .header("Content-Type", "application/json")
            .body(Body::from(b.to_owned()))
            .unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 16384).await.unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

const SUB_BODY: &str =
    r#"{"endpoint":"https://push.example.com/x","keys":{"p256dh":"pk","auth":"au"}}"#;

#[tokio::test]
async fn vapid_public_key_returns_the_key() {
    let app = router(test_app_state().with_push_service(Arc::new(MockPushService)));
    let (status, json) = send(app, "GET", "/api/push/vapid-public-key", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["key"], "BTestApplicationServerKey");
}

#[tokio::test]
async fn subscribe_returns_ok() {
    let app = router(test_app_state().with_push_service(Arc::new(MockPushService)));
    let (status, json) = send(app, "POST", "/api/push/subscriptions", Some(SUB_BODY)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["message"], "subscribed");
}

#[tokio::test]
async fn unsubscribe_with_and_without_endpoint_returns_ok() {
    let app = router(test_app_state().with_push_service(Arc::new(MockPushService)));
    let (status, json) = send(
        app,
        "DELETE",
        "/api/push/subscriptions?endpoint=https://push.example.com/x",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["message"], "unsubscribed");

    let app = router(test_app_state().with_push_service(Arc::new(MockPushService)));
    let (status, _) = send(app, "DELETE", "/api/push/subscriptions", None).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn default_noop_push_service_errors_on_vapid_but_accepts_mutations() {
    // No `with_push_service` -> AppState's default `NoopPushService`. Exercises
    // the no-op branch: vapid errors (500), subscribe/unsubscribe succeed.
    let app = router(test_app_state());
    let (status, _) = send(app, "GET", "/api/push/vapid-public-key", None).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);

    let app = router(test_app_state());
    let (status, _) = send(app, "POST", "/api/push/subscriptions", Some(SUB_BODY)).await;
    assert_eq!(status, StatusCode::OK);

    let app = router(test_app_state());
    let (status, _) = send(app, "DELETE", "/api/push/subscriptions", None).await;
    assert_eq!(status, StatusCode::OK);
}

/// Like [`send`] but with an admin session cookie attached.
async fn send_as_admin(app: Router, method: &str, uri: &str) -> (StatusCode, serde_json::Value) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("Cookie", "wardnet_session=test")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 16384).await.unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

#[tokio::test]
async fn list_notifications_rejects_anonymous_callers() {
    // Default StubAuthService validates no session -> SessionAuth rejects.
    let app = router(test_app_state().with_push_service(Arc::new(MockPushService)));
    let (status, _) = send(app, "GET", "/api/push/notifications", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let app = router(test_app_state().with_push_service(Arc::new(MockPushService)));
    let (status, _) = send(app, "DELETE", "/api/push/notifications", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn list_notifications_returns_the_feed_for_admins() {
    let app = router(
        test_app_state()
            .with_auth_service(Arc::new(AlwaysSessionAuth))
            .with_push_service(Arc::new(MockPushService)),
    );
    let (status, json) = send_as_admin(app, "GET", "/api/push/notifications?limit=10").await;
    assert_eq!(status, StatusCode::OK);
    let items = json["notifications"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["kind"], "new_device_quarantined");
    assert_eq!(items[0]["url"], "/devices");
    assert_eq!(items[0]["subject_id"], "device-1");
    assert_eq!(items[0]["created_at"], "2026-07-03T00:00:00Z");
}

#[tokio::test]
async fn clear_notifications_returns_ok_for_admins() {
    let app = router(
        test_app_state()
            .with_auth_service(Arc::new(AlwaysSessionAuth))
            .with_push_service(Arc::new(MockPushService)),
    );
    let (status, json) = send_as_admin(app, "DELETE", "/api/push/notifications").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["message"], "cleared");
}

#[tokio::test]
async fn default_noop_push_service_handle_event_is_ok() {
    // handle_event isn't reachable over HTTP; call it directly to cover the
    // no-op default.
    let state = test_app_state();
    let event = WardnetEvent::DnsServerStarted {
        timestamp: chrono::Utc::now(),
    };
    state.push_service().handle_event(&event).await.unwrap();
}
