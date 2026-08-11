use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use wardnet_common::auth::AuthContext;
use wardnet_common::event::WardnetEvent;

use crate::event::EventPublisher;
use crate::push::PushService;

/// Background task that turns domain events into Web Push notifications.
///
/// Subscribes to the event bus and forwards every event to
/// [`PushService::handle_event`] under an admin context (push delivery is a
/// system action, not a user request). The service decides audience and
/// content; this listener is intentionally thin so the mapping logic stays
/// unit-testable in `wardnetd-services`.
pub struct PushNotificationListener {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl PushNotificationListener {
    /// Start the push listener. `parent` roots the task's tracing span.
    pub fn start(
        events: &Arc<dyn EventPublisher>,
        push: Arc<dyn PushService>,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "push_listener");
        let rx = events.subscribe();
        let handle = tokio::spawn(event_loop(rx, push, cancel.clone()).instrument(span));
        Self { cancel, handle }
    }

    /// Cancel the background task and wait for it to finish.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("push listener shut down");
    }
}

async fn event_loop(
    mut rx: tokio::sync::broadcast::Receiver<WardnetEvent>,
    push: Arc<dyn PushService>,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            result = rx.recv() => {
                match result {
                    Ok(event) => handle_event(&event, &*push).await,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "push listener: lagged behind event bus: skipped={n}");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::info!("push listener: event bus closed");
                        break;
                    }
                }
            }
        }
    }
}

async fn handle_event(event: &WardnetEvent, push: &dyn PushService) {
    // Deliver under an admin context: `handle_event` opens with
    // `require_admin`, matching how other background listeners drive
    // auth-gated services (see `routing_listener`).
    if let Err(error) =
        crate::auth_context::with_context(AuthContext::system(), push.handle_event(event)).await
    {
        tracing::warn!(%error, "push listener: failed to handle event");
    }
}
