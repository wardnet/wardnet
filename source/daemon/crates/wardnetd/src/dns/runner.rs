use std::sync::Arc;

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;
use wardnet_types::auth::AuthContext;
use wardnet_types::event::WardnetEvent;

use crate::auth_context;
use crate::dns::server::DnsServer;
use crate::event::EventPublisher;
use crate::service::DnsService;

/// Background runner for the DNS server.
///
/// Manages the lifecycle of the DNS server based on configuration:
/// - Starts the server if DNS is enabled on startup.
/// - Listens for configuration change events to reload or restart.
pub struct DnsRunner {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl DnsRunner {
    /// Start the DNS runner as a background task.
    pub fn start(
        service: Arc<dyn DnsService>,
        server: Arc<dyn DnsServer>,
        events: &dyn EventPublisher,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "dns_runner");
        let event_rx = events.subscribe();

        let handle =
            tokio::spawn(runner_loop(service, server, event_rx, cancel.clone()).instrument(span));

        Self { cancel, handle }
    }

    /// Cancel the background task and wait for it to finish.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("DNS runner shut down");
    }
}

/// Main runner loop: start server if enabled and listen for events.
async fn runner_loop(
    service: Arc<dyn DnsService>,
    server: Arc<dyn DnsServer>,
    mut event_rx: broadcast::Receiver<WardnetEvent>,
    cancel: CancellationToken,
) {
    let admin_ctx = AuthContext::Admin {
        admin_id: Uuid::nil(),
    };

    // Check if DNS is enabled and start the server.
    match auth_context::with_context(admin_ctx.clone(), service.get_dns_config()).await {
        Ok(config) if config.enabled => {
            tracing::info!("DNS is enabled, starting server");
            server.update_config(config).await;
            if let Err(e) = server.start().await {
                tracing::error!(error = %e, "failed to start DNS server");
            }
        }
        Ok(_) => {
            tracing::info!("DNS is disabled, server not started");
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to load DNS config on startup");
        }
    }

    loop {
        tokio::select! {
            () = cancel.cancelled() => {
                tracing::info!("DNS runner cancellation received");
                break;
            }
            result = event_rx.recv() => {
                match result {
                    Ok(WardnetEvent::DnsConfigChanged { .. }) => {
                        tracing::info!("DNS config changed, reloading");
                        match auth_context::with_context(admin_ctx.clone(), service.get_dns_config()).await {
                            Ok(config) => {
                                let should_run = config.enabled;
                                server.update_config(config).await;

                                if should_run && !server.is_running() {
                                    if let Err(e) = server.start().await {
                                        tracing::error!(error = %e, "failed to start DNS server after config change");
                                    }
                                } else if !should_run
                                    && server.is_running()
                                    && let Err(e) = server.stop().await
                                {
                                    tracing::error!(error = %e, "failed to stop DNS server after config change");
                                }
                            }
                            Err(e) => {
                                tracing::error!(error = %e, "failed to reload DNS config");
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "DNS runner lagged behind event bus");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::info!("DNS runner: event bus closed");
                        break;
                    }
                }
            }
        }
    }

    // Always stop the server when the runner loop exits.
    if let Err(e) = server.stop().await {
        tracing::error!(error = %e, "failed to stop DNS server during shutdown");
    }
}
