use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;
use wardnet_types::event::WardnetEvent;
use wardnet_types::routing::RoutingTarget;

use crate::event::EventPublisher;
use crate::service::{RoutingService, TunnelService};

/// Tracks per-tunnel idle countdowns and the set of known-active tunnels.
///
/// When a tunnel loses all its assigned devices, we record the instant that
/// happened. A periodic sweep checks whether the idle duration has exceeded the
/// configured timeout and tears the tunnel down if so.
struct IdleState {
    /// Tunnels with an active idle countdown. Value is when the countdown started.
    idle_since: HashMap<Uuid, tokio::time::Instant>,
    /// Tunnels that are currently up (learned from `TunnelUp`, removed on
    /// `TunnelDown`). Used to know which tunnels to recheck on `DeviceGone`.
    active_tunnels: HashSet<Uuid>,
}

/// Watches for device routing changes and tears down tunnels that have been
/// idle (no devices routing through them) for longer than a configurable timeout.
///
/// Subscribes to the event bus for `RoutingRuleChanged`, `DeviceGone`,
/// `TunnelUp`, and `TunnelDown` events. When the last device using a tunnel
/// departs, starts an idle countdown. If no device reclaims the tunnel before
/// the timeout elapses, instructs [`TunnelService`] to tear it down.
pub struct IdleTunnelWatcher {
    /// Token used to signal the background task to stop.
    cancel: CancellationToken,
    /// Handle to the spawned event-loop task.
    handle: tokio::task::JoinHandle<()>,
}

impl IdleTunnelWatcher {
    /// Start the idle tunnel watcher.
    ///
    /// Spawns a background task that listens for domain events and manages
    /// per-tunnel idle countdowns. When a tunnel has no devices routing through
    /// it for longer than `idle_timeout_secs`, it is torn down automatically.
    ///
    /// The `parent` span is used as the parent for the `idle_watcher` child
    /// span, ensuring all log output from the spawned task includes the root
    /// version field.
    pub fn start(
        events: Arc<dyn EventPublisher>,
        tunnel_service: Arc<dyn TunnelService>,
        routing_service: Arc<dyn RoutingService>,
        idle_timeout_secs: u64,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let idle_timeout = Duration::from_secs(idle_timeout_secs);
        let span = tracing::info_span!(parent: parent, "idle_watcher");

        let handle = tokio::spawn(
            event_loop(
                events,
                tunnel_service,
                routing_service,
                idle_timeout,
                cancel.clone(),
            )
            .instrument(span),
        );

        Self { cancel, handle }
    }

    /// Cancel the background task and wait for it to finish.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("idle tunnel watcher shut down");
    }
}

/// Main event loop: subscribe to domain events, handle routing changes, and
/// periodically sweep for tunnels whose idle countdown has expired.
async fn event_loop(
    events: Arc<dyn EventPublisher>,
    tunnel_service: Arc<dyn TunnelService>,
    routing: Arc<dyn RoutingService>,
    idle_timeout: Duration,
    cancel: CancellationToken,
) {
    let mut rx = events.subscribe();
    let mut idle_state = IdleState {
        idle_since: HashMap::new(),
        active_tunnels: HashSet::new(),
    };
    let mut sweep_interval = tokio::time::interval(Duration::from_secs(30));

    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            result = rx.recv() => {
                match result {
                    Ok(event) => {
                        handle_event(&event, &*routing, &mut idle_state).await;
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "idle watcher: lagged behind event bus");
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        tracing::info!("idle watcher: event bus closed");
                        break;
                    }
                }
            }
            _ = sweep_interval.tick() => {
                sweep_idle(&*tunnel_service, &mut idle_state, idle_timeout).await;
            }
        }
    }
}

/// Process a single domain event and update the idle state accordingly.
///
/// - `RoutingRuleChanged`: check whether the previous tunnel target is now idle
///   and whether the new tunnel target should cancel an active countdown.
/// - `DeviceGone`: check all active tunnels — `remove_device_routes` does not
///   emit `RoutingRuleChanged`, so we must query directly.
/// - `TunnelUp`: track the tunnel as active.
/// - `TunnelDown`: remove any countdown and stop tracking the tunnel.
/// - All other events are ignored.
async fn handle_event(event: &WardnetEvent, routing: &dyn RoutingService, state: &mut IdleState) {
    match event {
        WardnetEvent::RoutingRuleChanged {
            target,
            previous_target,
            ..
        } => {
            // If previous target was a tunnel, check if it's now idle.
            if let Some(RoutingTarget::Tunnel { tunnel_id }) = previous_target {
                let users = crate::auth_context::with_context(
                    wardnet_types::auth::AuthContext::Admin {
                        admin_id: Uuid::nil(),
                    },
                    routing.devices_using_tunnel(*tunnel_id),
                )
                .await;
                let users = match users {
                    Ok(u) => u,
                    Err(e) => {
                        tracing::warn!(error = %e, tunnel_id = %tunnel_id, "failed to query devices using tunnel");
                        return;
                    }
                };
                if users.is_empty() {
                    tracing::info!(
                        tunnel_id = %tunnel_id,
                        "tunnel has no active devices, starting idle countdown"
                    );
                    state
                        .idle_since
                        .entry(*tunnel_id)
                        .or_insert_with(tokio::time::Instant::now);
                }
            }
            // If new target is a tunnel, cancel idle countdown.
            if let RoutingTarget::Tunnel { tunnel_id } = target
                && state.idle_since.remove(tunnel_id).is_some()
            {
                tracing::info!(
                    tunnel_id = %tunnel_id,
                    "tunnel reclaimed by device, cancelling idle countdown"
                );
            }
        }
        WardnetEvent::DeviceGone { .. } => {
            // A device left the network. Check all active tunnels to see
            // if any now have zero devices routing through them.
            // Note: remove_device_routes does NOT emit RoutingRuleChanged,
            // so we must check directly.
            let active_tunnels: Vec<Uuid> = state.active_tunnels.iter().copied().collect();
            for tid in active_tunnels {
                // Skip tunnels that already have an idle countdown.
                if state.idle_since.contains_key(&tid) {
                    continue;
                }
                let users = crate::auth_context::with_context(
                    wardnet_types::auth::AuthContext::Admin {
                        admin_id: Uuid::nil(),
                    },
                    routing.devices_using_tunnel(tid),
                )
                .await;
                let users = match users {
                    Ok(u) => u,
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            tunnel_id = %tid,
                            "failed to query tunnel users after device gone"
                        );
                        continue;
                    }
                };
                if users.is_empty() {
                    tracing::info!(
                        tunnel_id = %tid,
                        "tunnel has no active devices after device departure, starting idle countdown"
                    );
                    state
                        .idle_since
                        .entry(tid)
                        .or_insert_with(tokio::time::Instant::now);
                }
            }
        }
        WardnetEvent::TunnelUp { tunnel_id, .. } => {
            state.active_tunnels.insert(*tunnel_id);
        }
        WardnetEvent::TunnelDown { tunnel_id, .. } => {
            state.active_tunnels.remove(tunnel_id);
            // Already down — remove countdown if any.
            if state.idle_since.remove(tunnel_id).is_some() {
                tracing::debug!(
                    tunnel_id = %tunnel_id,
                    "tunnel already down, removing idle countdown"
                );
            }
        }
        _ => {}
    }
}

/// Check all active idle countdowns and tear down tunnels whose timeout has elapsed.
async fn sweep_idle(tunnel_service: &dyn TunnelService, state: &mut IdleState, timeout: Duration) {
    let now = tokio::time::Instant::now();
    let mut expired = Vec::new();

    for (tunnel_id, since) in &state.idle_since {
        if now.duration_since(*since) >= timeout {
            expired.push(*tunnel_id);
        }
    }

    for tunnel_id in expired {
        state.idle_since.remove(&tunnel_id);
        tracing::info!(tunnel_id = %tunnel_id, "tearing down idle tunnel");
        if let Err(e) = crate::auth_context::with_context(
            wardnet_types::auth::AuthContext::Admin {
                admin_id: Uuid::nil(),
            },
            tunnel_service.tear_down_internal(tunnel_id, "idle timeout"),
        )
        .await
        {
            tracing::warn!(
                error = %e,
                tunnel_id = %tunnel_id,
                "failed to tear down idle tunnel"
            );
        }
    }
}
