use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;
use wardnet_common::access_request::{AccessRequestKind, AccessRequestStatus};
use wardnet_common::auth::AuthContext;
use wardnet_common::event::WardnetEvent;

use wardnetd_services::access_request::AccessRequestService;
use wardnetd_services::event::EventPublisher;

/// Background task that reconciles the access-request inbox with grants made
/// outside it (issue #919).
///
/// An admin can reach the same end state from two places: approving a pending
/// Private-DNS request, or granting the device straight from the Remote Access
/// card. Without this, the second path would leave a phantom `pending` row
/// nagging the admin about something they already did.
///
/// **Why the bus and not a direct call.** Approving a Private-DNS request calls
/// into `PrivateDnsService` to mint the grant, so `PrivateDnsService` must not
/// call back into `AccessRequestService` — that would close a dependency cycle
/// between two services. Publishing `PrivateDnsGrantCreated` and reconciling
/// here keeps the service graph one-way, which is the same reason
/// [`crate::dns_device_snapshot_listener`] exists: the grant is persisted
/// before its event is published, so the bus is a race-free choke point.
///
/// The resolve is idempotent — the repository guards it on `status = 'pending'`
/// — so the approval path's own write and this listener's write cannot
/// double-apply, whichever lands first.
pub struct AccessRequestListener {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl AccessRequestListener {
    /// Start the listener.
    ///
    /// The `parent` span parents the `access_request_listener` child span so
    /// all task output carries the root version field.
    pub fn start(
        events: &Arc<dyn EventPublisher>,
        access_requests: Arc<dyn AccessRequestService>,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "access_request_listener");

        let rx = events.subscribe();

        let handle = tokio::spawn(event_loop(rx, access_requests, cancel.clone()).instrument(span));

        Self { cancel, handle }
    }

    /// Cancel the background task and wait for it to finish.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("access request listener shut down");
    }
}

async fn event_loop(
    mut rx: tokio::sync::broadcast::Receiver<WardnetEvent>,
    access_requests: Arc<dyn AccessRequestService>,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            result = rx.recv() => {
                match result {
                    Ok(WardnetEvent::PrivateDnsGrantCreated { device_id, granted_by, .. }) => {
                        resolve(&*access_requests, device_id, granted_by).await;
                    }
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        // A dropped grant event leaves at most a stale pending
                        // row, which the admin can still decide by hand — so
                        // there is nothing to resync, unlike a snapshot
                        // listener whose state would be wrong.
                        tracing::warn!(
                            skipped = n,
                            "access request listener: lagged behind event bus: skipped={n}"
                        );
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::info!("access request listener: event bus closed");
                        break;
                    }
                }
            }
        }
    }
}

async fn resolve(
    access_requests: &dyn AccessRequestService,
    device_id: Uuid,
    granted_by: Option<String>,
) {
    // Runs under the system context because this task has no request context
    // of its own, and `resolve_pending` is admin-gated like every other write
    // on that service — the same wrapper the other bus listeners use.
    match wardnetd_services::auth_context::with_context(
        AuthContext::system(),
        access_requests.resolve_pending(
            device_id,
            AccessRequestKind::PrivateDns,
            AccessRequestStatus::Approved,
            granted_by,
        ),
    )
    .await
    {
        // The common case by far: a grant made with no request outstanding.
        Ok(None) => {}
        Ok(Some(request)) => {
            tracing::info!(
                request_id = %request.id,
                device_id = %device_id,
                "resolved a pending Private DNS request from an out-of-band grant"
            );
        }
        Err(e) => {
            // Non-fatal: the grant is already persisted and working. The
            // request simply stays pending for the admin to decide by hand.
            tracing::warn!(
                error = %e,
                device_id = %device_id,
                "could not resolve pending Private DNS request after grant"
            );
        }
    }
}
