use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use wardnet_common::event::WardnetEvent;

use crate::diagnostics::{Diagnostic, DiagnosticSink};
use crate::event::EventPublisher;

/// Background task that turns error-flavoured domain events into diagnostics.
///
/// Subscribes to the event bus and records every event that
/// [`Diagnostic::from_event`] recognises into the shared [`DiagnosticSink`].
/// Intentionally thin — the recognise/convert logic lives in
/// [`crate::diagnostics`] so it stays unit-testable without a running bus.
pub struct DiagnosticsListener {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl DiagnosticsListener {
    /// Start the listener. `parent` roots the task's tracing span.
    pub fn start(
        events: &Arc<dyn EventPublisher>,
        sink: Arc<dyn DiagnosticSink>,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "diagnostics_listener");
        let rx = events.subscribe();
        let handle = tokio::spawn(event_loop(rx, sink, cancel.clone()).instrument(span));
        Self { cancel, handle }
    }

    /// Cancel the background task and wait for it to finish.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("diagnostics listener shut down");
    }
}

async fn event_loop(
    mut rx: tokio::sync::broadcast::Receiver<WardnetEvent>,
    sink: Arc<dyn DiagnosticSink>,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            result = rx.recv() => {
                match result {
                    Ok(event) => {
                        if let Some(diagnostic) = Diagnostic::from_event(&event) {
                            sink.record(diagnostic);
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "diagnostics listener: lagged behind event bus: skipped={n}");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::info!("diagnostics listener: event bus closed");
                        break;
                    }
                }
            }
        }
    }
}
