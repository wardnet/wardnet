use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;
use wardnet_common::event::WardnetEvent;

use crate::DhcpService;
use crate::auth_context;
use crate::dhcp::server::DhcpServer;
use crate::event::EventPublisher;

/// Background runner for the DHCP server.
///
/// Manages the lifecycle of the DHCP server based on configuration:
/// - Starts the server if DHCP is enabled on startup.
/// - Periodically cleans up expired leases.
/// - Listens for configuration change events to restart the server.
pub struct DhcpRunner {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl DhcpRunner {
    /// Start the DHCP runner as a background task.
    ///
    /// The `parent` span is used as the parent for the `dhcp_runner` child
    /// span, ensuring all log output includes the root version field.
    pub fn start(
        service: Arc<dyn DhcpService>,
        server: Arc<dyn DhcpServer>,
        events: &dyn EventPublisher,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "dhcp_runner");
        let event_rx = events.subscribe();

        let handle =
            tokio::spawn(runner_loop(service, server, event_rx, cancel.clone()).instrument(span));

        Self { cancel, handle }
    }

    /// Cancel the background task and wait for it to finish.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("DHCP runner shut down");
    }
}

/// Main runner loop: start server if enabled, clean up expired leases,
/// and listen for configuration change events.
async fn runner_loop(
    service: Arc<dyn DhcpService>,
    server: Arc<dyn DhcpServer>,
    mut event_rx: broadcast::Receiver<WardnetEvent>,
    cancel: CancellationToken,
) {
    let admin_ctx = AuthContext::Admin {
        admin_id: Uuid::nil(),
    };

    // Check if DHCP is enabled and start the server.
    match auth_context::with_context(admin_ctx.clone(), service.get_dhcp_config()).await {
        Ok(config) if config.enabled => {
            tracing::info!("DHCP is enabled, starting server");
            if let Err(e) = server.start().await {
                tracing::error!(error = %e, "failed to start DHCP server");
            }
        }
        Ok(_) => {
            tracing::info!("DHCP is disabled, server not started");
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to load DHCP config on startup");
        }
    }

    // Periodic lease cleanup interval.
    let mut cleanup_interval = tokio::time::interval(Duration::from_mins(1));

    loop {
        tokio::select! {
            () = cancel.cancelled() => {
                tracing::info!("DHCP runner cancellation received");
                break;
            }
            _ = cleanup_interval.tick() => {
                if let Err(e) = auth_context::with_context(admin_ctx.clone(), service.cleanup_expired()).await {
                    tracing::error!(error = %e, "failed to clean up expired DHCP leases");
                }
            }
            result = event_rx.recv() => {
                match result {
                    Ok(event) => {
                        tracing::debug!(?event, "DHCP runner received event");
                        // Future: react to config change events to restart/stop the server.
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "DHCP runner lagged behind event bus");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::info!("DHCP runner: event bus closed");
                        break;
                    }
                }
            }
        }
    }

    // Always stop the server when the runner loop exits, regardless of reason.
    if let Err(e) = server.stop().await {
        tracing::error!(error = %e, "failed to stop DHCP server during shutdown");
    }
}
