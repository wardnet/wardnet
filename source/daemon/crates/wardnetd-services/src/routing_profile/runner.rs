//! Background runner for the routing-profile subsystem.
//!
//! Two jobs, both off the DNS hot path:
//! - drains the enforcement queue fed by
//!   [`RoutingProfileService::note_resolution`] and installs each matched
//!   domain's `ip rule` via [`RoutingService::route_resolved_domain`];
//! - ticks periodically to expire TTL'd leases via
//!   [`RoutingService::gc_domain_routes`].
//!
//! It also bootstraps the compiled routing view once at startup, and rebuilds
//! it on [`WardnetEvent::DeviceIpChanged`] is *not* needed — the view is keyed
//! by device id, not IP — so the only rebuild trigger is a profile/assignment
//! mutation, which the service handles inline.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use wardnet_common::auth::AuthContext;

use crate::auth_context;
use crate::routing::RoutingService;
use crate::routing_profile::service::{DomainRouteRequest, RoutingProfileService};

/// How often expired domain-route leases are swept.
const GC_INTERVAL: Duration = Duration::from_secs(30);

pub struct DomainRouteRunner {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl DomainRouteRunner {
    pub fn start(
        profiles: Arc<dyn RoutingProfileService>,
        routing: Arc<dyn RoutingService>,
        requests: mpsc::Receiver<DomainRouteRequest>,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "domain_route_runner");
        let handle =
            tokio::spawn(runner_loop(profiles, routing, requests, cancel.clone()).instrument(span));
        Self { cancel, handle }
    }

    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("domain route runner shut down");
    }
}

async fn runner_loop(
    profiles: Arc<dyn RoutingProfileService>,
    routing: Arc<dyn RoutingService>,
    mut requests: mpsc::Receiver<DomainRouteRequest>,
    cancel: CancellationToken,
) {
    let admin_ctx = AuthContext::system();

    // Bootstrap the compiled view so lookups work before the first mutation.
    if let Err(e) = auth_context::with_context(admin_ctx.clone(), profiles.refresh_view()).await {
        tracing::error!(error = %e, "failed to bootstrap routing-profile view");
    }

    let mut gc = tokio::time::interval(GC_INTERVAL);
    gc.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            maybe = requests.recv() => {
                let Some(req) = maybe else { break };
                let device_ip = req.device_ip.to_string();
                let fut = routing.route_resolved_domain(
                    &device_ip,
                    &req.resolved_ips,
                    &req.target,
                    req.ttl_secs,
                );
                if let Err(e) = auth_context::with_context(admin_ctx.clone(), fut).await {
                    tracing::warn!(error = %e, "failed to enforce domain route");
                }
            }
            _ = gc.tick() => {
                if let Err(e) =
                    auth_context::with_context(admin_ctx.clone(), routing.gc_domain_routes()).await
                {
                    tracing::warn!(error = %e, "failed to gc domain routes");
                }
            }
        }
    }
}
