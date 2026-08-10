//! DNS server lifecycle runner.
//!
//! After the Stage 7 split (issue #221), filter rebuilding lives in
//! [`crate::dns_filter::DnsFilterRunner`]. This runner is responsible for
//! starting/stopping the DNS server in response to
//! [`WardnetEvent::DnsConfigChanged`], and for rebuilding the
//! [`AuthoritativeView`] on [`WardnetEvent::DnsLocalChanged`].

use std::sync::Arc;

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use wardnet_common::auth::AuthContext;
use wardnet_common::event::WardnetEvent;

use crate::auth_context;
use crate::dns::authoritative::AuthoritativeView;
use crate::dns::server::DnsServer;
use crate::dns::service::DnsService;
use crate::dns_local::DnsLocalService;
use crate::event::EventPublisher;

pub struct DnsRunner {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl DnsRunner {
    pub fn start(
        service: Arc<dyn DnsService>,
        server: Arc<dyn DnsServer>,
        dns_local: Arc<dyn DnsLocalService>,
        events: &dyn EventPublisher,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "dns_runner");
        let event_rx = events.subscribe();

        let handle = tokio::spawn(
            runner_loop(service, server, dns_local, event_rx, cancel.clone()).instrument(span),
        );

        Self { cancel, handle }
    }

    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("DNS runner shut down");
    }
}

/// Build an `AuthoritativeView` from the local-DNS service.
///
/// `DnsLocalService` is auth-gated, so each read runs under an admin
/// [`auth_context`] just like the other background runners. Errors are logged
/// and result in an empty view so the server can still start.
async fn build_authoritative_view(
    service: &Arc<dyn DnsLocalService>,
    admin_ctx: &AuthContext,
) -> AuthoritativeView {
    let (zones, records, rules) = tokio::join!(
        auth_context::with_context(admin_ctx.clone(), service.list_zones()),
        auth_context::with_context(admin_ctx.clone(), service.list_records()),
        auth_context::with_context(admin_ctx.clone(), service.list_forwarding_rules()),
    );
    match (zones, records, rules) {
        (Ok(z), Ok(r), Ok(ru)) => AuthoritativeView::build(&z.zones, r.records, ru.rules),
        (zones, records, rules) => {
            if let Err(e) = &zones {
                tracing::error!(error = %e, "failed to load DNS zones for authoritative view");
            }
            if let Err(e) = &records {
                tracing::error!(error = %e, "failed to load DNS records for authoritative view");
            }
            if let Err(e) = &rules {
                tracing::error!(error = %e, "failed to load DNS forwarding rules for authoritative view");
            }
            AuthoritativeView::empty()
        }
    }
}

async fn runner_loop(
    service: Arc<dyn DnsService>,
    server: Arc<dyn DnsServer>,
    dns_local: Arc<dyn DnsLocalService>,
    mut event_rx: broadcast::Receiver<WardnetEvent>,
    cancel: CancellationToken,
) {
    let admin_ctx = AuthContext::system();

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

    // Load the initial authoritative view so local records are served from the
    // very first query — before any DnsLocalChanged event fires.
    let initial_view = build_authoritative_view(&dns_local, &admin_ctx).await;
    server.update_authoritative_view(initial_view).await;

    loop {
        tokio::select! {
            () = cancel.cancelled() => {
                tracing::info!("DNS runner cancellation received");
                break;
            }
            result = event_rx.recv() => {
                match result {
                    Ok(WardnetEvent::DnsConfigChanged { .. }) => {
                        match auth_context::with_context(admin_ctx.clone(), service.get_dns_config()).await {
                            Ok(config) => {
                                let should_run = config.enabled;
                                server.update_config(config).await;
                                if should_run && !server.is_running() {
                                    if let Err(e) = server.start().await {
                                        tracing::error!(error = %e, "failed to start DNS server after config change");
                                    }
                                } else if !should_run && server.is_running()
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
                    Ok(WardnetEvent::DnsLocalChanged { domain, .. }) => {
                        let view = build_authoritative_view(&dns_local, &admin_ctx).await;
                        server.update_authoritative_view(view).await;
                        server.invalidate_subtree(&domain).await;
                        tracing::debug!(
                            domain = %domain,
                            "rebuilt authoritative view and evicted domain subtree from cache"
                        );
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

    if let Err(e) = server.stop().await {
        tracing::error!(error = %e, "failed to stop DNS server during shutdown");
    }
}

#[cfg(test)]
mod tests;
