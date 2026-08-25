//! Tests for [`AccessRequestListener`] (issue #919).
//!
//! The listener is what keeps the access-request inbox honest when an admin
//! grants Private DNS from the Remote Access card instead of by approving a
//! pending request. It exists as a bus listener rather than a direct call
//! because approving already depends on `PrivateDnsService`, so the reverse
//! edge would close a cycle — see the listener's own docs and ADR-0033 §3.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use chrono::Utc;
use tokio::time::{Duration, sleep};
use uuid::Uuid;
use wardnet_common::access_request::{
    AccessRequestKind, AccessRequestStatus, ApprovalParams, DeviceAccessRequest,
};
use wardnet_common::auth::AuthContext;
use wardnet_common::event::WardnetEvent;

use wardnetd_services::access_request::AccessRequestService;
use wardnetd_services::error::AppError;
use wardnetd_services::event::{BroadcastEventBus, EventPublisher};

use crate::access_request_listener::AccessRequestListener;

/// Records what `resolve_pending` was asked to do, and asserts it arrives
/// under an auth context — the real implementation is `require_admin`-gated,
/// so a listener that forgot the system-context wrapper would 403 in
/// production while every unit test still passed.
/// One recorded `resolve_pending` call.
#[derive(Debug, PartialEq, Eq)]
struct ResolveCall {
    device_id: Uuid,
    kind: AccessRequestKind,
    status: AccessRequestStatus,
    decided_by: Option<String>,
}

#[derive(Default)]
struct RecordingAccessRequests {
    calls: AtomicUsize,
    seen: std::sync::Mutex<Vec<ResolveCall>>,
    /// Had an auth context on every call.
    authed: std::sync::Mutex<Vec<bool>>,
    /// Resolve returns an error — the listener must warn and keep running.
    fail: bool,
    /// Resolve finds nothing pending — the common case, and not an error.
    nothing_pending: bool,
}

impl RecordingAccessRequests {
    fn count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl AccessRequestService for RecordingAccessRequests {
    async fn create_for_ip(
        &self,
        _ip: &str,
        _kind: AccessRequestKind,
        _domain: Option<String>,
        _reason: Option<String>,
    ) -> Result<DeviceAccessRequest, AppError> {
        unimplemented!("not exercised by AccessRequestListener")
    }
    async fn list_for_ip(&self, _ip: &str) -> Result<Vec<DeviceAccessRequest>, AppError> {
        unimplemented!("not exercised by AccessRequestListener")
    }
    async fn list(
        &self,
        _status: Option<AccessRequestStatus>,
    ) -> Result<Vec<DeviceAccessRequest>, AppError> {
        unimplemented!("not exercised by AccessRequestListener")
    }
    async fn decide(
        &self,
        _id: &str,
        _status: AccessRequestStatus,
        _params: Option<ApprovalParams>,
    ) -> Result<DeviceAccessRequest, AppError> {
        unimplemented!("not exercised by AccessRequestListener")
    }

    async fn resolve_pending(
        &self,
        device_id: Uuid,
        kind: AccessRequestKind,
        status: AccessRequestStatus,
        decided_by: Option<String>,
    ) -> Result<Option<DeviceAccessRequest>, AppError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.authed
            .lock()
            .unwrap()
            .push(wardnetd_services::auth_context::try_current().is_some());
        self.seen.lock().unwrap().push(ResolveCall {
            device_id,
            kind,
            status,
            decided_by: decided_by.clone(),
        });

        if self.fail {
            return Err(AppError::Internal(anyhow::anyhow!("database is away")));
        }
        if self.nothing_pending {
            return Ok(None);
        }
        Ok(Some(DeviceAccessRequest {
            id: "req-1".to_owned(),
            device_id: device_id.to_string(),
            kind,
            domain: None,
            reason: None,
            status,
            created_at: Utc::now().to_rfc3339(),
            decided_at: Some(Utc::now().to_rfc3339()),
            decided_by,
        }))
    }
}

fn grant_created(device_id: Uuid, granted_by: Option<&str>) -> WardnetEvent {
    WardnetEvent::PrivateDnsGrantCreated {
        device_id,
        granted_by: granted_by.map(str::to_owned),
        timestamp: Utc::now(),
    }
}

/// Publish one event, let the listener drain it, and return the service.
async fn publish(
    service: Arc<RecordingAccessRequests>,
    event: WardnetEvent,
) -> Arc<RecordingAccessRequests> {
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let parent = tracing::info_span!("test");
    let listener = AccessRequestListener::start(
        &bus,
        Arc::clone(&service) as Arc<dyn AccessRequestService>,
        &parent,
    );
    bus.publish(event);
    sleep(Duration::from_millis(100)).await;
    listener.shutdown().await;
    service
}

#[tokio::test]
async fn a_grant_resolves_the_device_s_pending_request() {
    let device_id = Uuid::new_v4();
    let svc = publish(
        Arc::new(RecordingAccessRequests::default()),
        grant_created(device_id, Some("admin-1")),
    )
    .await;

    assert_eq!(svc.count(), 1);
    let seen = svc.seen.lock().unwrap();
    assert_eq!(
        seen[0],
        ResolveCall {
            device_id,
            kind: AccessRequestKind::PrivateDns,
            status: AccessRequestStatus::Approved,
            decided_by: Some("admin-1".to_owned()),
        },
        "the grant's device and acting admin must reach the inbox verbatim"
    );
}

/// `resolve_pending` is admin-gated, and this task has no request context of
/// its own — so it has to supply one. Without the wrapper the call would be
/// refused in production while the service's own unit tests stayed green.
#[tokio::test]
async fn resolve_runs_under_an_auth_context() {
    let svc = publish(
        Arc::new(RecordingAccessRequests::default()),
        grant_created(Uuid::new_v4(), Some("admin-1")),
    )
    .await;
    assert_eq!(svc.authed.lock().unwrap().as_slice(), &[true]);
}

/// A grant made with no request outstanding is the common case by far, and
/// must not be logged or treated as a failure.
#[tokio::test]
async fn nothing_pending_is_not_an_error() {
    let svc = publish(
        Arc::new(RecordingAccessRequests {
            nothing_pending: true,
            ..RecordingAccessRequests::default()
        }),
        grant_created(Uuid::new_v4(), None),
    )
    .await;
    assert_eq!(svc.count(), 1);
}

/// The grant is already persisted and working; a failure to reconcile the
/// inbox leaves a row an admin can still decide by hand, so it must not take
/// the listener down.
#[tokio::test]
async fn a_resolve_error_does_not_stop_the_listener() {
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let svc = Arc::new(RecordingAccessRequests {
        fail: true,
        ..RecordingAccessRequests::default()
    });
    let parent = tracing::info_span!("test");
    let listener = AccessRequestListener::start(
        &bus,
        Arc::clone(&svc) as Arc<dyn AccessRequestService>,
        &parent,
    );

    bus.publish(grant_created(Uuid::new_v4(), Some("admin-1")));
    sleep(Duration::from_millis(100)).await;
    // A second grant after the failure must still be handled.
    bus.publish(grant_created(Uuid::new_v4(), Some("admin-2")));
    sleep(Duration::from_millis(100)).await;
    listener.shutdown().await;

    assert_eq!(svc.count(), 2);
}

/// A revoke is the `DoT` listener's business, not the inbox's — and resolving
/// on one would mark a request approved because access was taken away.
#[tokio::test]
async fn unrelated_events_are_ignored() {
    let svc = publish(
        Arc::new(RecordingAccessRequests::default()),
        WardnetEvent::PrivateDnsGrantRevoked {
            device_id: Uuid::new_v4(),
            timestamp: Utc::now(),
        },
    )
    .await;
    assert_eq!(svc.count(), 0);
}

/// A grant with no human behind it (a system-context caller) carries no
/// `granted_by`, and the inbox must record that rather than inventing one.
#[tokio::test]
async fn a_grant_without_an_admin_records_no_decider() {
    let svc = publish(
        Arc::new(RecordingAccessRequests::default()),
        grant_created(Uuid::new_v4(), None),
    )
    .await;
    assert_eq!(svc.seen.lock().unwrap()[0].decided_by, None);
}

#[tokio::test]
async fn shutdown_without_events_completes_cleanly() {
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let svc = Arc::new(RecordingAccessRequests::default());
    let parent = tracing::info_span!("test");
    let listener = AccessRequestListener::start(
        &bus,
        Arc::clone(&svc) as Arc<dyn AccessRequestService>,
        &parent,
    );
    listener.shutdown().await;
    assert_eq!(svc.count(), 0);
}

/// Several grants in quick succession each name a different device, so unlike
/// the snapshot listeners there is nothing to coalesce — every one must be
/// reconciled.
#[tokio::test]
async fn every_grant_in_a_burst_is_reconciled() {
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let svc = Arc::new(RecordingAccessRequests::default());
    let parent = tracing::info_span!("test");
    let listener = AccessRequestListener::start(
        &bus,
        Arc::clone(&svc) as Arc<dyn AccessRequestService>,
        &parent,
    );

    for _ in 0..5 {
        bus.publish(grant_created(Uuid::new_v4(), Some("admin-1")));
    }
    sleep(Duration::from_millis(200)).await;
    listener.shutdown().await;

    assert_eq!(svc.count(), 5, "each device's grant needs its own resolve");
}

/// `AuthContext::system()` is a `User` carrying the admin role, so a grant
/// made under it must still not name the nil user as the decider.
#[tokio::test]
async fn the_system_context_is_not_recorded_as_a_decider() {
    // Mirrors what `grant_device` derives before publishing.
    let granted_by = match Some(AuthContext::system()) {
        Some(AuthContext::User(u)) if u.user_id() != Uuid::nil() => Some(u.user_id().to_string()),
        _ => None,
    };
    assert_eq!(granted_by, None);
}
