//! Background task that live-reloads Network-Zone packet enforcement (#736).
//!
//! Mirrors [`RoutingListener`](crate::routing_listener::RoutingListener): it
//! subscribes to the domain event bus and dispatches the events that affect a
//! device's zone rules to the [`ZoneEnforcementService`]. It is a *separate*
//! subscriber from the routing listener — the two enforce different layers
//! (packet gating vs kernel routing) and neither blocks the other.

use std::sync::Arc;

use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use wardnet_common::auth::AuthContext;
use wardnet_common::event::WardnetEvent;

use wardnetd_services::ZoneEnforcementService;
use wardnetd_services::event::EventPublisher;

/// Owns the spawned zone-enforcement event loop and its cancellation handle.
pub struct ZoneEnforcementListener {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl ZoneEnforcementListener {
    /// Start the zone-enforcement listener.
    ///
    /// The `parent` span is used as the parent for the
    /// `zone_enforcement_listener` child span so all log output from the spawned
    /// task carries the root version field.
    pub fn start(
        events: &Arc<dyn EventPublisher>,
        enforcer: Arc<dyn ZoneEnforcementService>,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "zone_enforcement_listener");

        let rx = events.subscribe();

        let handle = tokio::spawn(event_loop(rx, enforcer, cancel.clone()).instrument(span));

        Self { cancel, handle }
    }

    /// Cancel the background task and wait for it to finish.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("zone enforcement listener shut down");
    }
}

async fn event_loop(
    mut rx: tokio::sync::broadcast::Receiver<WardnetEvent>,
    enforcer: Arc<dyn ZoneEnforcementService>,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            result = rx.recv() => {
                match result {
                    Ok(event) => handle_event(event, &*enforcer).await,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(skipped = n, "zone enforcement listener: lagged behind event bus: skipped={n}");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        tracing::info!("zone enforcement listener: event bus closed");
                        break;
                    }
                }
            }
        }
    }
}

/// Run `op` inside an admin auth context and log any error under `context`.
///
/// The enforcer methods are admin-guarded (like the routing service); the
/// listener supplies a nil-admin context exactly as the routing listener does.
async fn with_admin<F>(context: &str, op: F)
where
    F: std::future::Future<Output = Result<(), wardnetd_services::error::AppError>>,
{
    if let Err(e) = wardnetd_services::auth_context::with_context(
        AuthContext::Admin {
            admin_id: uuid::Uuid::nil(),
        },
        op,
    )
    .await
    {
        tracing::warn!(error = %e, "zone enforcement: {context} failed: {e}");
    }
}

async fn handle_event(event: WardnetEvent, enforcer: &dyn ZoneEnforcementService) {
    match event {
        // A zone's policy changed — re-apply every member's rules.
        WardnetEvent::NetworkZoneChanged { zone_id, .. } => {
            with_admin("apply_zone", enforcer.apply_zone(zone_id)).await;
        }

        // A device moved zones — release its lease so it re-IPs into the new
        // subnet, then re-apply its rules and recompute the L3 isolation state.
        WardnetEvent::DeviceZoneChanged { device_id, .. } => {
            with_admin("handle_zone_change", enforcer.handle_zone_change(device_id)).await;
        }

        // A freshly-discovered device lands in the default-for-new zone — install
        // its rules (e.g. the admin-UI gate for a Guest/IoT device).
        WardnetEvent::DeviceDiscovered { device_id, .. } => {
            with_admin(
                "apply_device (discovered)",
                enforcer.apply_device(device_id),
            )
            .await;
        }

        // A device's IP changed — re-key its rules from the old IP to the new.
        WardnetEvent::DeviceIpChanged {
            device_id,
            old_ip,
            new_ip,
            ..
        } => {
            with_admin(
                "handle_ip_change",
                enforcer.handle_ip_change(device_id, &old_ip, &new_ip),
            )
            .await;
        }

        // A device departed — tear down its zone rules.
        WardnetEvent::DeviceGone {
            device_id, last_ip, ..
        } => {
            with_admin("remove_device", enforcer.remove_device(device_id, &last_ip)).await;
        }

        // The global default policy flipped — unbind any device whose zone now
        // forbids the resolved target (the one edge the #735 write gate misses).
        WardnetEvent::DefaultPolicyChanged { policy, .. } => {
            with_admin(
                "handle_default_policy_changed",
                enforcer.handle_default_policy_changed(&policy),
            )
            .await;
        }

        // Everything else is irrelevant to zone packet enforcement.
        WardnetEvent::RoutingRuleChanged { .. }
        | WardnetEvent::DeviceAdminLocked { .. }
        | WardnetEvent::TunnelUp { .. }
        | WardnetEvent::TunnelStartFailed { .. }
        | WardnetEvent::TunnelDown { .. }
        | WardnetEvent::TunnelConnecting { .. }
        | WardnetEvent::TunnelReconnecting { .. }
        | WardnetEvent::TunnelStatsUpdated { .. }
        | WardnetEvent::TunnelDnsOverrideChanged { .. }
        | WardnetEvent::RouteTableLost { .. }
        | WardnetEvent::DhcpLeaseAssigned { .. }
        | WardnetEvent::DhcpLeaseRenewed { .. }
        | WardnetEvent::DhcpLeaseExpired { .. }
        | WardnetEvent::DhcpConflictDetected { .. }
        | WardnetEvent::DnsServerStarted { .. }
        | WardnetEvent::DnsServerStopped { .. }
        | WardnetEvent::DnsConfigChanged { .. }
        | WardnetEvent::DnsFilterBlocklistUpdated { .. }
        | WardnetEvent::DnsFilterChanged { .. }
        | WardnetEvent::DnsFilterRebuilt { .. }
        | WardnetEvent::DnsLocalChanged { .. }
        | WardnetEvent::UpdateAvailable { .. }
        | WardnetEvent::UpdateProgress { .. }
        | WardnetEvent::UpdateCompleted { .. }
        | WardnetEvent::UpdateFailed { .. }
        | WardnetEvent::DeviceCaptureSettingsChanged { .. }
        // New-device quarantine (#738) only drives an admin push; the device
        // already landed in the default-for-new zone via `DeviceDiscovered`.
        // Rule requests (#482) likewise only drive an admin push.
        | WardnetEvent::NewDeviceQuarantined { .. }
        | WardnetEvent::RuleRequestCreated { .. }
        // Entitlement changes are handled by the dedicated `entitlement_listener`.
        | WardnetEvent::EntitlementChanged { .. }
        | WardnetEvent::DnsEventInserted { .. } => {}

        // A cross-zone exception changed — rebuild the L3 isolation state so its
        // ACCEPT rules are re-derived (issue #737).
        WardnetEvent::ZoneExceptionsChanged { .. } => {
            with_admin(
                "handle_exceptions_changed",
                enforcer.handle_exceptions_changed(),
            )
            .await;
        }
    }
}
