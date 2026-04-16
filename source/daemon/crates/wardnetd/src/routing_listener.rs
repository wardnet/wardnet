use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use wardnet_types::event::WardnetEvent;

use crate::event::EventPublisher;
use crate::repository::DeviceRepository;
use crate::service::RoutingService;

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
        devices: Arc<dyn DeviceRepository>,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "routing_listener");

        // Subscribe before spawning to avoid missing events published between
        // spawn and the first `rx.recv()`.
        let rx = events.subscribe();

        let handle =
            tokio::spawn(event_loop(rx, routing, devices, cancel.clone()).instrument(span));

        Self { cancel, handle }
    }

    /// Cancel the background task and wait for it to finish.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("routing listener shut down");
    }
}

/// Subscribe to domain events and dispatch routing-relevant ones.
async fn event_loop(
    mut rx: tokio::sync::broadcast::Receiver<WardnetEvent>,
    routing: Arc<dyn RoutingService>,
    devices: Arc<dyn DeviceRepository>,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            result = rx.recv() => {
                match result {
                    Ok(event) => {
                        handle_event(event, &*routing, &*devices).await;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "routing listener: lagged behind event bus");
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

/// Dispatch a single domain event to the appropriate routing service method.
async fn handle_event(
    event: WardnetEvent,
    routing: &dyn RoutingService,
    devices: &dyn DeviceRepository,
) {
    match event {
        WardnetEvent::RoutingRuleChanged {
            device_id, target, ..
        } => {
            handle_routing_rule_changed(device_id, &target, routing, devices).await;
        }

        WardnetEvent::DeviceDiscovered { device_id, ip, .. } => {
            handle_device_discovered(device_id, &ip, routing, devices).await;
        }

        WardnetEvent::DeviceIpChanged {
            device_id,
            old_ip,
            new_ip,
            ..
        } => {
            if let Err(e) = crate::auth_context::with_context(
                wardnet_types::auth::AuthContext::Admin {
                    admin_id: uuid::Uuid::nil(),
                },
                routing.handle_ip_change(device_id, &old_ip, &new_ip),
            )
            .await
            {
                tracing::warn!(error = %e, device_id = %device_id, "failed to handle IP change");
            }
        }

        WardnetEvent::DeviceGone {
            device_id, last_ip, ..
        } => {
            if let Err(e) = crate::auth_context::with_context(
                wardnet_types::auth::AuthContext::Admin {
                    admin_id: uuid::Uuid::nil(),
                },
                routing.remove_device_routes(device_id, &last_ip),
            )
            .await
            {
                tracing::warn!(
                    error = %e, device_id = %device_id,
                    "failed to remove routes for departed device"
                );
            }
        }

        WardnetEvent::TunnelUp { tunnel_id, .. } => {
            if let Err(e) = crate::auth_context::with_context(
                wardnet_types::auth::AuthContext::Admin {
                    admin_id: uuid::Uuid::nil(),
                },
                routing.handle_tunnel_up(tunnel_id),
            )
            .await
            {
                tracing::warn!(error = %e, tunnel_id = %tunnel_id, "failed to handle tunnel up");
            }
        }

        WardnetEvent::TunnelDown { tunnel_id, .. } => {
            if let Err(e) = crate::auth_context::with_context(
                wardnet_types::auth::AuthContext::Admin {
                    admin_id: uuid::Uuid::nil(),
                },
                routing.handle_tunnel_down(tunnel_id),
            )
            .await
            {
                tracing::warn!(error = %e, tunnel_id = %tunnel_id, "failed to handle tunnel down");
            }
        }

        WardnetEvent::RouteTableLost { table, .. } => {
            if let Err(e) = crate::auth_context::with_context(
                wardnet_types::auth::AuthContext::Admin {
                    admin_id: uuid::Uuid::nil(),
                },
                routing.handle_route_table_lost(table),
            )
            .await
            {
                tracing::warn!(error = %e, table, "failed to handle route table lost");
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

/// Look up the device and apply the routing rule for a `RoutingRuleChanged` event.
async fn handle_routing_rule_changed(
    device_id: uuid::Uuid,
    target: &wardnet_types::routing::RoutingTarget,
    routing: &dyn RoutingService,
    devices: &dyn DeviceRepository,
) {
    tracing::debug!(device_id = %device_id, ?target, "handling routing rule change");

    match devices.find_by_id(&device_id.to_string()).await {
        Ok(Some(device)) => {
            if let Err(e) = crate::auth_context::with_context(
                wardnet_types::auth::AuthContext::Admin {
                    admin_id: uuid::Uuid::nil(),
                },
                routing.apply_rule(device_id, &device.last_ip, target),
            )
            .await
            {
                tracing::warn!(error = %e, device_id = %device_id, "failed to apply routing rule");
            }
        }
        Ok(None) => {
            tracing::warn!(device_id = %device_id, "device not found for routing rule change");
        }
        Err(e) => {
            tracing::warn!(
                error = %e, device_id = %device_id,
                "failed to look up device for routing rule change"
            );
        }
    }
}

/// Check if a newly discovered device has a routing rule and apply it.
async fn handle_device_discovered(
    device_id: uuid::Uuid,
    ip: &str,
    routing: &dyn RoutingService,
    devices: &dyn DeviceRepository,
) {
    tracing::debug!(device_id = %device_id, ip = %ip, "handling device discovered");

    match devices.find_rule_for_device(&device_id.to_string()).await {
        Ok(Some(rule)) => {
            if let Err(e) = crate::auth_context::with_context(
                wardnet_types::auth::AuthContext::Admin {
                    admin_id: uuid::Uuid::nil(),
                },
                routing.apply_rule(device_id, ip, &rule.target),
            )
            .await
            {
                tracing::warn!(
                    error = %e, device_id = %device_id,
                    "failed to apply rule for newly discovered device"
                );
            }
        }
        Ok(None) => {
            // No routing rule for this device — nothing to do.
        }
        Err(e) => {
            tracing::warn!(
                error = %e, device_id = %device_id,
                "failed to look up routing rule for discovered device"
            );
        }
    }
}
