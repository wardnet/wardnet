use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use wardnet_common::auth::AuthContext;
use wardnet_common::event::WardnetEvent;

use wardnetd_services::event::EventPublisher;
use wardnetd_services::routing::RoutingService;

/// Background task that listens for domain events and updates kernel routing state.
///
/// Subscribes to the event bus and dispatches relevant events to the
/// [`RoutingService`]. Runs in a tokio task with a [`CancellationToken`] for
/// graceful shutdown.
pub struct RoutingListener {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl RoutingListener {
    /// Start the routing listener.
    ///
    /// The `parent` span is used as the parent for the `routing_listener` child
    /// span, ensuring all log output from the spawned task includes the root
    /// version field.
    pub fn start(
        events: &Arc<dyn EventPublisher>,
        routing: Arc<dyn RoutingService>,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "routing_listener");

        let rx = events.subscribe();

        let handle = tokio::spawn(event_loop(rx, routing, cancel.clone()).instrument(span));

        Self { cancel, handle }
    }

    /// Cancel the background task and wait for it to finish.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("routing listener shut down");
    }
}

async fn event_loop(
    mut rx: tokio::sync::broadcast::Receiver<WardnetEvent>,
    routing: Arc<dyn RoutingService>,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            result = rx.recv() => {
                match result {
                    Ok(event) => {
                        handle_event(event, &*routing).await;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "routing listener: lagged behind event bus: skipped={n}");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::info!("routing listener: event bus closed");
                        break;
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
async fn handle_event(event: WardnetEvent, routing: &dyn RoutingService) {
    match event {
        WardnetEvent::RoutingRuleChanged {
            device_id, target, ..
        } => {
            if let Err(e) = wardnetd_services::auth_context::with_context(
                AuthContext::Admin {
                    admin_id: uuid::Uuid::nil(),
                },
                routing.apply_rule_for_device(device_id, &target),
            )
            .await
            {
                tracing::warn!(error = %e, device_id = %device_id, "failed to apply routing rule for device {device_id}: {e}");
            }
        }

        WardnetEvent::DeviceDiscovered { device_id, ip, .. } => {
            if let Err(e) = wardnetd_services::auth_context::with_context(
                AuthContext::Admin {
                    admin_id: uuid::Uuid::nil(),
                },
                routing.apply_rule_for_discovered_device(device_id, &ip),
            )
            .await
            {
                tracing::warn!(error = %e, device_id = %device_id, "failed to apply rule for discovered device {device_id}: {e}");
            }
        }

        WardnetEvent::DeviceIpChanged {
            device_id,
            old_ip,
            new_ip,
            ..
        } => {
            if let Err(e) = wardnetd_services::auth_context::with_context(
                AuthContext::Admin {
                    admin_id: uuid::Uuid::nil(),
                },
                routing.handle_ip_change(device_id, &old_ip, &new_ip),
            )
            .await
            {
                tracing::warn!(error = %e, device_id = %device_id, "failed to handle IP change for {device_id}: {e}");
            }
        }

        WardnetEvent::DeviceGone {
            device_id, last_ip, ..
        } => {
            if let Err(e) = wardnetd_services::auth_context::with_context(
                AuthContext::Admin {
                    admin_id: uuid::Uuid::nil(),
                },
                routing.remove_device_routes(device_id, &last_ip),
            )
            .await
            {
                tracing::warn!(
                    error = %e, device_id = %device_id,
                    "failed to remove routes for departed device {device_id}: {e}"
                );
            }
        }

        WardnetEvent::TunnelUp { tunnel_id, .. } => {
            if let Err(e) = wardnetd_services::auth_context::with_context(
                AuthContext::Admin {
                    admin_id: uuid::Uuid::nil(),
                },
                routing.handle_tunnel_up(tunnel_id),
            )
            .await
            {
                tracing::warn!(error = %e, tunnel_id = %tunnel_id, "failed to handle tunnel up for {tunnel_id}: {e}");
            }
        }

        WardnetEvent::TunnelDown { tunnel_id, .. } => {
            if let Err(e) = wardnetd_services::auth_context::with_context(
                AuthContext::Admin {
                    admin_id: uuid::Uuid::nil(),
                },
                routing.handle_tunnel_down(tunnel_id),
            )
            .await
            {
                tracing::warn!(error = %e, tunnel_id = %tunnel_id, "failed to handle tunnel down for {tunnel_id}: {e}");
            }
        }

        WardnetEvent::RouteTableLost { table, .. } => {
            if let Err(e) = wardnetd_services::auth_context::with_context(
                AuthContext::Admin {
                    admin_id: uuid::Uuid::nil(),
                },
                routing.handle_route_table_lost(table),
            )
            .await
            {
                tracing::warn!(error = %e, table, "failed to handle route table lost {table}: {e}");
            }
        }

        // Events that do not affect routing.
        WardnetEvent::TunnelStatsUpdated { .. }
        | WardnetEvent::DhcpLeaseAssigned { .. }
        | WardnetEvent::DhcpLeaseRenewed { .. }
        | WardnetEvent::DhcpLeaseExpired { .. }
        | WardnetEvent::DhcpConflictDetected { .. }
        | WardnetEvent::DnsServerStarted { .. }
        | WardnetEvent::DnsServerStopped { .. }
        | WardnetEvent::DnsConfigChanged { .. }
        | WardnetEvent::DnsBlocklistUpdated { .. }
        | WardnetEvent::DnsFiltersChanged { .. } => {}
    }
}
