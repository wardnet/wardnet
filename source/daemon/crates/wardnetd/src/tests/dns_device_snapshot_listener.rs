use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use arc_swap::ArcSwap;
use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;
use wardnet_common::dns::UpstreamId;
use wardnet_common::event::WardnetEvent;
use wardnet_common::routing::{RoutingTarget, RuleCreator};

use wardnetd_services::error::AppError;
use wardnetd_services::event::{BroadcastEventBus, EventPublisher};
use wardnetd_services::routing::RoutingService;

use crate::dns_device_snapshot_listener::DnsDeviceSnapshotListener;

// ---------------------------------------------------------------------------
// CountingRoutingService
// ---------------------------------------------------------------------------

/// Counts `rebuild_dns_device_upstream_snapshot` calls (and asserts they
/// arrive under an admin auth context, since the real impl is
/// `require_admin`-gated); everything else is unreachable from the
/// listener.
struct CountingRoutingService {
    rebuild_calls: AtomicUsize,
    /// When `true`, the rebuild returns an error — the listener must warn
    /// and keep running.
    fail: bool,
}

impl CountingRoutingService {
    fn new() -> Self {
        Self {
            rebuild_calls: AtomicUsize::new(0),
            fail: false,
        }
    }

    fn failing() -> Self {
        Self {
            rebuild_calls: AtomicUsize::new(0),
            fail: true,
        }
    }

    fn call_count(&self) -> usize {
        self.rebuild_calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
#[allow(clippy::similar_names)]
impl RoutingService for CountingRoutingService {
    async fn apply_rule(
        &self,
        _device_id: Uuid,
        _device_ip: &str,
        _target: &RoutingTarget,
    ) -> Result<(), AppError> {
        unimplemented!("not exercised by DnsDeviceSnapshotListener")
    }
    async fn remove_device_routes(
        &self,
        _device_id: Uuid,
        _device_ip: &str,
    ) -> Result<(), AppError> {
        unimplemented!("not exercised by DnsDeviceSnapshotListener")
    }
    async fn handle_ip_change(
        &self,
        _device_id: Uuid,
        _old_ip: &str,
        _new_ip: &str,
    ) -> Result<(), AppError> {
        unimplemented!("not exercised by DnsDeviceSnapshotListener")
    }
    async fn handle_tunnel_down(&self, _tunnel_id: Uuid) -> Result<(), AppError> {
        unimplemented!("not exercised by DnsDeviceSnapshotListener")
    }
    async fn handle_tunnel_up(&self, _tunnel_id: Uuid) -> Result<(), AppError> {
        unimplemented!("not exercised by DnsDeviceSnapshotListener")
    }
    async fn reconcile(&self) -> Result<(), AppError> {
        unimplemented!("not exercised by DnsDeviceSnapshotListener")
    }
    async fn handle_route_table_lost(&self, _table: u32) -> Result<(), AppError> {
        unimplemented!("not exercised by DnsDeviceSnapshotListener")
    }
    async fn devices_using_tunnel(&self, _tunnel_id: Uuid) -> Result<Vec<Uuid>, AppError> {
        unimplemented!("not exercised by DnsDeviceSnapshotListener")
    }
    async fn apply_rule_for_device(
        &self,
        _device_id: Uuid,
        _target: &RoutingTarget,
    ) -> Result<(), AppError> {
        unimplemented!("not exercised by DnsDeviceSnapshotListener")
    }
    async fn apply_rule_for_discovered_device(
        &self,
        _device_id: Uuid,
        _ip: &str,
    ) -> Result<(), AppError> {
        unimplemented!("not exercised by DnsDeviceSnapshotListener")
    }
    async fn set_switchback_targets(
        &self,
        _device_id: Uuid,
        _device_ip: String,
        _target_cidrs: Vec<String>,
    ) -> Result<(), AppError> {
        unimplemented!("not exercised by DnsDeviceSnapshotListener")
    }
    async fn route_resolved_domain(
        &self,
        _device_ip: &str,
        _resolved_ips: &[IpAddr],
        _target: &wardnet_common::routing_profile::DomainRoutingTarget,
        _ttl_secs: u32,
    ) -> Result<(), AppError> {
        unimplemented!("not exercised by DnsDeviceSnapshotListener")
    }
    async fn gc_domain_routes(&self) -> Result<(), AppError> {
        unimplemented!("not exercised by DnsDeviceSnapshotListener")
    }
    async fn set_default_policy(&self, _policy: &str) -> Result<(), AppError> {
        unimplemented!("not exercised by DnsDeviceSnapshotListener")
    }
    async fn default_policy(&self) -> Result<String, AppError> {
        unimplemented!("not exercised by DnsDeviceSnapshotListener")
    }
    async fn handle_default_policy_changed(&self) -> Result<(), AppError> {
        unimplemented!("not exercised by DnsDeviceSnapshotListener")
    }
    fn dns_upstream_snapshot(&self) -> Arc<ArcSwap<HashMap<IpAddr, UpstreamId>>> {
        unimplemented!("not exercised by DnsDeviceSnapshotListener")
    }
    async fn rebuild_dns_upstream_snapshot(&self) -> Result<(), AppError> {
        unimplemented!("not exercised by DnsDeviceSnapshotListener")
    }
    fn dns_device_upstream_snapshot(&self) -> Arc<ArcSwap<HashMap<Uuid, UpstreamId>>> {
        unimplemented!("not exercised by DnsDeviceSnapshotListener")
    }
    async fn rebuild_dns_device_upstream_snapshot(&self) -> Result<(), AppError> {
        // The real impl is admin-gated; assert the listener established
        // the context it needs.
        wardnetd_services::auth_context::require_admin()?;
        self.rebuild_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail {
            return Err(AppError::Internal(anyhow::anyhow!(
                "mock: rebuild_dns_device_upstream_snapshot forced error"
            )));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn tunnel_up_event() -> WardnetEvent {
    WardnetEvent::TunnelUp {
        tunnel_id: Uuid::new_v4(),
        interface_name: "wg_ward0".to_owned(),
        endpoint: "1.2.3.4:51820".to_owned(),
        timestamp: Utc::now(),
    }
}

/// Publish `event`, give the listener time to react, shut it down, and
/// return the rebuild call count.
async fn publish_and_count(routing: &Arc<CountingRoutingService>, event: WardnetEvent) -> usize {
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let parent = tracing::info_span!("test");
    let listener = DnsDeviceSnapshotListener::start(
        &bus,
        Arc::clone(routing) as Arc<dyn RoutingService>,
        &parent,
    );

    bus.publish(event);

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;

    routing.call_count()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn each_relevant_event_kind_triggers_a_rebuild() {
    let events: Vec<WardnetEvent> = vec![
        WardnetEvent::RoutingRuleChanged {
            device_id: Uuid::new_v4(),
            target: RoutingTarget::Direct,
            previous_target: None,
            changed_by: RuleCreator::User,
            timestamp: Utc::now(),
        },
        WardnetEvent::DefaultPolicyChanged {
            policy: "direct".to_owned(),
            timestamp: Utc::now(),
        },
        tunnel_up_event(),
        WardnetEvent::TunnelDown {
            tunnel_id: Uuid::new_v4(),
            interface_name: "wg_ward0".to_owned(),
            reason: "test".to_owned(),
            timestamp: Utc::now(),
        },
        // The gate is `status != Down`, so entering Connecting matters as
        // much as reaching Up — on-demand bring-up publishes only this.
        WardnetEvent::TunnelConnecting {
            tunnel_id: Uuid::new_v4(),
            interface_name: "wg_ward0".to_owned(),
            endpoint: "1.2.3.4:51820".to_owned(),
            timestamp: Utc::now(),
        },
        WardnetEvent::TunnelReconnecting {
            tunnel_id: Uuid::new_v4(),
            interface_name: "wg_ward0".to_owned(),
            last_handshake: None,
            timestamp: Utc::now(),
        },
        WardnetEvent::TunnelDnsOverrideChanged {
            tunnel_id: Uuid::new_v4(),
            timestamp: Utc::now(),
        },
        WardnetEvent::DeviceZoneChanged {
            device_id: Uuid::new_v4(),
            old_zone_id: Uuid::new_v4(),
            new_zone_id: Uuid::new_v4(),
            timestamp: Utc::now(),
        },
        WardnetEvent::NetworkZoneChanged {
            zone_id: Uuid::new_v4(),
            timestamp: Utc::now(),
        },
    ];

    for event in events {
        let routing = Arc::new(CountingRoutingService::new());
        let variant = format!("{event:?}");
        let calls = publish_and_count(&routing, event).await;
        assert_eq!(calls, 1, "expected one rebuild for {variant}");
    }
}

#[tokio::test]
async fn unrelated_events_are_ignored() {
    let routing = Arc::new(CountingRoutingService::new());
    let calls = publish_and_count(
        &routing,
        WardnetEvent::DhcpLeaseAssigned {
            lease_id: Uuid::new_v4(),
            mac: "AA:BB:CC:DD:EE:01".to_owned(),
            ip: "192.168.1.10".to_owned(),
            hostname: None,
            timestamp: Utc::now(),
        },
    )
    .await;
    assert_eq!(calls, 0, "an irrelevant event must not trigger a rebuild");
}

#[tokio::test]
async fn rebuild_error_does_not_stop_the_listener() {
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing = Arc::new(CountingRoutingService::failing());
    let parent = tracing::info_span!("test");
    let listener = DnsDeviceSnapshotListener::start(
        &bus,
        Arc::clone(&routing) as Arc<dyn RoutingService>,
        &parent,
    );

    bus.publish(tunnel_up_event());
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    // A second event after the failure must still be handled.
    bus.publish(tunnel_up_event());
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;

    assert_eq!(routing.call_count(), 2);
}

#[tokio::test]
async fn burst_of_relevant_events_collapses_into_fewer_rebuilds() {
    // A tunnel deletion or zone clamp publishes one `RoutingRuleChanged`
    // per affected device; `drain_pending` should collapse whatever is
    // queued by the time the handler runs into a single rebuild.
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing = Arc::new(CountingRoutingService::new());
    let parent = tracing::info_span!("test");
    let listener = DnsDeviceSnapshotListener::start(
        &bus,
        Arc::clone(&routing) as Arc<dyn RoutingService>,
        &parent,
    );

    for _ in 0..5 {
        bus.publish(WardnetEvent::RoutingRuleChanged {
            device_id: Uuid::new_v4(),
            target: RoutingTarget::Direct,
            previous_target: None,
            changed_by: RuleCreator::User,
            timestamp: Utc::now(),
        });
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;

    let calls = routing.call_count();
    assert!(
        (1..5).contains(&calls),
        "5 queued events should coalesce into fewer rebuilds, got {calls}"
    );
}

#[tokio::test]
async fn shutdown_without_events_completes_cleanly() {
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing = Arc::new(CountingRoutingService::new());
    let parent = tracing::info_span!("test");
    let listener = DnsDeviceSnapshotListener::start(
        &bus,
        Arc::clone(&routing) as Arc<dyn RoutingService>,
        &parent,
    );

    listener.shutdown().await;

    assert_eq!(routing.call_count(), 0);
}
