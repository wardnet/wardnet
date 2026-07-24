//! Background task that disables Premium-only runtime state the moment the box
//! loses entitlement (issue #266).
//!
//! Personal VPN (the inbound-WireGuard server) and Private DNS (the `DoT`
//! listener, issue #912) are Premium. Each feature's request-time gate blocks
//! enabling/granting without entitlement, and its `reconcile()` disables an
//! enabled feature on the next restart after a loss — but neither tears down
//! *running* state the instant a subscription lapses. This listener does: it
//! subscribes to the domain event bus and reacts to
//! [`WardnetEvent::EntitlementChanged`] with `entitled: false` by disabling
//! both features immediately (the Private-DNS disable also publishes
//! `PrivateDnsChanged`, which stops the `:853` listener via the `DoT` runner).
//!
//! Mirrors [`ZoneEnforcementListener`](crate::zone_enforcement_listener): a
//! dedicated subscriber, cancellation-aware, supplying a nil-admin context for
//! the admin-guarded service call.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use wardnet_common::auth::AuthContext;
use wardnet_common::event::WardnetEvent;

use wardnetd_services::event::EventPublisher;
use wardnetd_services::{InboundWgService, PrivateDnsService};

/// Owns the spawned entitlement-listener event loop and its cancellation handle.
pub struct EntitlementListener {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl EntitlementListener {
    /// Start the entitlement listener. The `parent` span roots the
    /// `entitlement_listener` child span so its log output carries the version
    /// field like every other background component.
    pub fn start(
        events: &Arc<dyn EventPublisher>,
        inbound_wg: Arc<dyn InboundWgService>,
        private_dns: Arc<dyn PrivateDnsService>,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "entitlement_listener");
        let rx = events.subscribe();
        let handle =
            tokio::spawn(event_loop(rx, inbound_wg, private_dns, cancel.clone()).instrument(span));
        Self { cancel, handle }
    }

    /// Cancel the background task and wait for it to finish.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("entitlement listener shut down");
    }
}

async fn event_loop(
    mut rx: tokio::sync::broadcast::Receiver<WardnetEvent>,
    inbound_wg: Arc<dyn InboundWgService>,
    private_dns: Arc<dyn PrivateDnsService>,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            result = rx.recv() => {
                match result {
                    // The only event this listener cares about: entitlement lost.
                    Ok(WardnetEvent::EntitlementChanged { entitled: false, .. }) => {
                        disable_inbound_wg(&*inbound_wg).await;
                        disable_private_dns(&*private_dns).await;
                    }
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "entitlement listener: lagged behind event bus: skipped={n}");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::info!("entitlement listener: event bus closed");
                        break;
                    }
                }
            }
        }
    }
}

/// Disable the inbound-WireGuard server on entitlement loss, in a nil-admin
/// context because the service methods are admin-guarded (the same pattern the
/// zone-enforcement listener uses for background enforcement).
async fn disable_inbound_wg(inbound_wg: &dyn InboundWgService) {
    if let Err(e) = wardnetd_services::auth_context::with_context(
        AuthContext::Admin {
            admin_id: uuid::Uuid::nil(),
        },
        disable_if_enabled(inbound_wg),
    )
    .await
    {
        tracing::warn!(error = %e, "entitlement listener: disabling inbound WireGuard failed: {e}");
    }
}

/// The admin-guarded operation: turn the server off if it is currently on
/// (an already-off server is a silent no-op). The explicit `Result` return
/// type is what lets the caller's `with_context` future infer — an inline
/// `async {}` block there cannot.
async fn disable_if_enabled(
    inbound_wg: &dyn InboundWgService,
) -> Result<(), wardnetd_services::error::AppError> {
    let config = inbound_wg.get_config().await?;
    if config.enabled {
        tracing::warn!("entitlement lost: disabling the inbound WireGuard (Personal VPN) server");
        inbound_wg.set_config(false, config.listen_port).await?;
    }
    Ok(())
}

/// Disable Private DNS on entitlement loss, same shape as
/// [`disable_inbound_wg`]. Disabling is deliberately not entitlement-gated on
/// the service side, so this always succeeds for a lapsed box.
async fn disable_private_dns(private_dns: &dyn PrivateDnsService) {
    if let Err(e) = wardnetd_services::auth_context::with_context(
        AuthContext::Admin {
            admin_id: uuid::Uuid::nil(),
        },
        disable_private_dns_if_enabled(private_dns),
    )
    .await
    {
        tracing::warn!(error = %e, "entitlement listener: disabling Private DNS failed: {e}");
    }
}

/// The admin-guarded Private-DNS half of the teardown.
async fn disable_private_dns_if_enabled(
    private_dns: &dyn PrivateDnsService,
) -> Result<(), wardnetd_services::error::AppError> {
    if private_dns.status().await?.enabled {
        tracing::warn!("entitlement lost: disabling Private DNS (the DoT listener)");
        private_dns.set_enabled(false).await?;
    }
    Ok(())
}
