use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;
use wardnet_common::event::WardnetEvent;
use wardnet_common::routing::{RoutingTarget, RuleCreator};

use wardnetd_services::error::AppError;
use wardnetd_services::event::{BroadcastEventBus, EventPublisher};
use wardnetd_services::routing::RoutingService;

use crate::routing_listener::RoutingListener;

// ---------------------------------------------------------------------------
// MockRoutingService
// ---------------------------------------------------------------------------

/// Records which methods were called so tests can assert dispatch behaviour.
struct MockRoutingService {
    calls: Mutex<Vec<RoutingCall>>,
    /// When `true`, `handle_route_table_lost` returns an error.
    error_on_route_table_lost: Mutex<bool>,
}

/// Describes a single call made to the mock routing service.
#[derive(Debug, Clone, PartialEq, Eq)]
enum RoutingCall {
    ApplyRule {
        device_id: Uuid,
        device_ip: String,
        target: RoutingTarget,
    },
    ApplyRuleForDevice {
        device_id: Uuid,
        target: RoutingTarget,
    },
    ApplyRuleForDiscovered {
        device_id: Uuid,
        ip: String,
    },
    RemoveDeviceRoutes {
        device_id: Uuid,
        device_ip: String,
    },
    HandleIpChange {
        device_id: Uuid,
        old_ip: String,
        new_ip: String,
    },
    HandleTunnelUp {
        tunnel_id: Uuid,
    },
    HandleTunnelDown {
        tunnel_id: Uuid,
    },
    HandleRouteTableLost {
        table: u32,
    },
}

impl MockRoutingService {
    fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            error_on_route_table_lost: Mutex::new(false),
        }
    }

    fn take_calls(&self) -> Vec<RoutingCall> {
        std::mem::take(&mut *self.calls.lock().unwrap())
    }
}

#[async_trait]
#[allow(clippy::similar_names)]
impl RoutingService for MockRoutingService {
    async fn apply_rule(
        &self,
        device_id: Uuid,
        device_ip: &str,
        target: &RoutingTarget,
    ) -> Result<(), AppError> {
        self.calls.lock().unwrap().push(RoutingCall::ApplyRule {
            device_id,
            device_ip: device_ip.to_owned(),
            target: target.clone(),
        });
        Ok(())
    }

    async fn remove_device_routes(&self, device_id: Uuid, device_ip: &str) -> Result<(), AppError> {
        self.calls
            .lock()
            .unwrap()
            .push(RoutingCall::RemoveDeviceRoutes {
                device_id,
                device_ip: device_ip.to_owned(),
            });
        Ok(())
    }

    async fn handle_ip_change(
        &self,
        device_id: Uuid,
        old_ip: &str,
        new_ip: &str,
    ) -> Result<(), AppError> {
        self.calls
            .lock()
            .unwrap()
            .push(RoutingCall::HandleIpChange {
                device_id,
                old_ip: old_ip.to_owned(),
                new_ip: new_ip.to_owned(),
            });
        Ok(())
    }

    async fn handle_tunnel_down(&self, tunnel_id: Uuid) -> Result<(), AppError> {
        self.calls
            .lock()
            .unwrap()
            .push(RoutingCall::HandleTunnelDown { tunnel_id });
        Ok(())
    }

    async fn handle_tunnel_up(&self, tunnel_id: Uuid) -> Result<(), AppError> {
        self.calls
            .lock()
            .unwrap()
            .push(RoutingCall::HandleTunnelUp { tunnel_id });
        Ok(())
    }

    async fn reconcile(&self) -> Result<(), AppError> {
        Ok(())
    }

    async fn handle_route_table_lost(&self, table: u32) -> Result<(), AppError> {
        self.calls
            .lock()
            .unwrap()
            .push(RoutingCall::HandleRouteTableLost { table });
        if *self.error_on_route_table_lost.lock().unwrap() {
            return Err(AppError::Internal(anyhow::anyhow!(
                "mock: handle_route_table_lost forced error"
            )));
        }
        Ok(())
    }

    async fn devices_using_tunnel(&self, _tunnel_id: Uuid) -> Result<Vec<Uuid>, AppError> {
        Ok(Vec::new())
    }

    async fn apply_rule_for_device(
        &self,
        device_id: Uuid,
        target: &RoutingTarget,
    ) -> Result<(), AppError> {
        self.calls
            .lock()
            .unwrap()
            .push(RoutingCall::ApplyRuleForDevice {
                device_id,
                target: target.clone(),
            });
        Ok(())
    }

    async fn apply_rule_for_discovered_device(
        &self,
        device_id: Uuid,
        ip: &str,
    ) -> Result<(), AppError> {
        self.calls
            .lock()
            .unwrap()
            .push(RoutingCall::ApplyRuleForDiscovered {
                device_id,
                ip: ip.to_owned(),
            });
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// FailingRoutingService
// ---------------------------------------------------------------------------

/// A routing service mock that returns errors for specific methods.
struct FailingRoutingService;

#[async_trait]
#[allow(clippy::similar_names)]
impl RoutingService for FailingRoutingService {
    async fn apply_rule(
        &self,
        _device_id: Uuid,
        _device_ip: &str,
        _target: &RoutingTarget,
    ) -> Result<(), AppError> {
        Err(AppError::Internal(anyhow::anyhow!("apply rule failed")))
    }

    async fn remove_device_routes(
        &self,
        _device_id: Uuid,
        _device_ip: &str,
    ) -> Result<(), AppError> {
        Err(AppError::Internal(anyhow::anyhow!("remove routes failed")))
    }

    async fn handle_ip_change(
        &self,
        _device_id: Uuid,
        _old_ip: &str,
        _new_ip: &str,
    ) -> Result<(), AppError> {
        Err(AppError::Internal(anyhow::anyhow!("ip change failed")))
    }

    async fn handle_tunnel_down(&self, _tunnel_id: Uuid) -> Result<(), AppError> {
        Err(AppError::Internal(anyhow::anyhow!("tunnel down failed")))
    }

    async fn handle_tunnel_up(&self, _tunnel_id: Uuid) -> Result<(), AppError> {
        Err(AppError::Internal(anyhow::anyhow!("tunnel up failed")))
    }

    async fn reconcile(&self) -> Result<(), AppError> {
        Ok(())
    }

    async fn handle_route_table_lost(&self, _table: u32) -> Result<(), AppError> {
        Ok(())
    }

    async fn devices_using_tunnel(&self, _tunnel_id: Uuid) -> Result<Vec<Uuid>, AppError> {
        Ok(Vec::new())
    }

    async fn apply_rule_for_device(
        &self,
        _device_id: Uuid,
        _target: &RoutingTarget,
    ) -> Result<(), AppError> {
        Err(AppError::Internal(anyhow::anyhow!(
            "apply_rule_for_device failed"
        )))
    }

    async fn apply_rule_for_discovered_device(
        &self,
        _device_id: Uuid,
        _ip: &str,
    ) -> Result<(), AppError> {
        Err(AppError::Internal(anyhow::anyhow!(
            "apply_rule_for_discovered_device failed"
        )))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
#[allow(clippy::similar_names)]
async fn routing_rule_changed_triggers_apply_rule_for_device() {
    let device_id = Uuid::new_v4();
    let tunnel_id = Uuid::new_v4();
    let target = RoutingTarget::Tunnel { tunnel_id };

    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing = Arc::new(MockRoutingService::new());

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing.clone(), &parent);

    bus.publish(WardnetEvent::RoutingRuleChanged {
        device_id,
        target: target.clone(),
        previous_target: None,
        changed_by: RuleCreator::User,
        timestamp: Utc::now(),
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;

    let calls = routing.take_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0],
        RoutingCall::ApplyRuleForDevice { device_id, target }
    );
}

#[tokio::test]
async fn device_gone_triggers_remove_device_routes() {
    let device_id = Uuid::new_v4();
    let last_ip = "192.168.1.42";

    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing = Arc::new(MockRoutingService::new());

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing.clone(), &parent);

    bus.publish(WardnetEvent::DeviceGone {
        device_id,
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
        last_ip: last_ip.to_owned(),
        timestamp: Utc::now(),
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;

    let calls = routing.take_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0],
        RoutingCall::RemoveDeviceRoutes {
            device_id,
            device_ip: last_ip.to_owned(),
        }
    );
}

#[tokio::test]
async fn tunnel_down_triggers_handle_tunnel_down() {
    let tunnel_id = Uuid::new_v4();

    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing = Arc::new(MockRoutingService::new());

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing.clone(), &parent);

    bus.publish(WardnetEvent::TunnelDown {
        tunnel_id,
        interface_name: "wg_ward0".to_owned(),
        reason: "peer timeout".to_owned(),
        timestamp: Utc::now(),
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;

    let calls = routing.take_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0], RoutingCall::HandleTunnelDown { tunnel_id });
}

#[tokio::test]
async fn tunnel_up_triggers_handle_tunnel_up() {
    let tunnel_id = Uuid::new_v4();

    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing = Arc::new(MockRoutingService::new());

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing.clone(), &parent);

    bus.publish(WardnetEvent::TunnelUp {
        tunnel_id,
        interface_name: "wg_ward0".to_owned(),
        endpoint: "198.51.100.1:51820".to_owned(),
        timestamp: Utc::now(),
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;

    let calls = routing.take_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0], RoutingCall::HandleTunnelUp { tunnel_id });
}

#[tokio::test]
async fn device_discovered_triggers_apply_rule_for_discovered_device() {
    let device_id = Uuid::new_v4();
    let ip = "192.168.1.99";

    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing = Arc::new(MockRoutingService::new());

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing.clone(), &parent);

    bus.publish(WardnetEvent::DeviceDiscovered {
        device_id,
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
        ip: ip.to_owned(),
        hostname: None,
        timestamp: Utc::now(),
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;

    let calls = routing.take_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0],
        RoutingCall::ApplyRuleForDiscovered {
            device_id,
            ip: ip.to_owned(),
        }
    );
}

#[tokio::test]
async fn device_ip_changed_triggers_handle_ip_change() {
    let device_id = Uuid::new_v4();
    let old_ip = "192.168.1.10";
    let new_ip = "192.168.1.20";

    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing = Arc::new(MockRoutingService::new());

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing.clone(), &parent);

    bus.publish(WardnetEvent::DeviceIpChanged {
        device_id,
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
        old_ip: old_ip.to_owned(),
        new_ip: new_ip.to_owned(),
        timestamp: Utc::now(),
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;

    let calls = routing.take_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0],
        RoutingCall::HandleIpChange {
            device_id,
            old_ip: old_ip.to_owned(),
            new_ip: new_ip.to_owned(),
        }
    );
}

#[tokio::test]
async fn shutdown_without_events_completes_cleanly() {
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing = Arc::new(MockRoutingService::new());

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing.clone(), &parent);

    listener.shutdown().await;

    let calls = routing.take_calls();
    assert!(calls.is_empty());
}

#[tokio::test]
async fn unrelated_events_are_ignored() {
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing = Arc::new(MockRoutingService::new());

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing.clone(), &parent);

    bus.publish(WardnetEvent::TunnelStatsUpdated {
        tunnel_id: Uuid::new_v4(),
        status: wardnet_common::tunnel::TunnelStatus::Up,
        bytes_tx: 1000,
        bytes_rx: 2000,
        last_handshake: None,
        timestamp: Utc::now(),
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;

    let calls = routing.take_calls();
    assert!(
        calls.is_empty(),
        "stats event should not trigger any routing call"
    );
}

// ---------------------------------------------------------------------------
// Error branch tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn routing_service_error_does_not_panic_on_ip_change() {
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing: Arc<dyn RoutingService> = Arc::new(FailingRoutingService);

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing, &parent);

    bus.publish(WardnetEvent::DeviceIpChanged {
        device_id: Uuid::new_v4(),
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
        old_ip: "192.168.1.10".to_owned(),
        new_ip: "192.168.1.20".to_owned(),
        timestamp: Utc::now(),
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;
}

#[tokio::test]
async fn routing_service_error_does_not_panic_on_device_gone() {
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing: Arc<dyn RoutingService> = Arc::new(FailingRoutingService);

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing, &parent);

    bus.publish(WardnetEvent::DeviceGone {
        device_id: Uuid::new_v4(),
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
        last_ip: "192.168.1.10".to_owned(),
        timestamp: Utc::now(),
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;
}

#[tokio::test]
async fn routing_service_error_does_not_panic_on_tunnel_up() {
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing: Arc<dyn RoutingService> = Arc::new(FailingRoutingService);

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing, &parent);

    bus.publish(WardnetEvent::TunnelUp {
        tunnel_id: Uuid::new_v4(),
        interface_name: "wg_ward0".to_owned(),
        endpoint: "198.51.100.1:51820".to_owned(),
        timestamp: Utc::now(),
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;
}

#[tokio::test]
async fn routing_service_error_does_not_panic_on_tunnel_down() {
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing: Arc<dyn RoutingService> = Arc::new(FailingRoutingService);

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing, &parent);

    bus.publish(WardnetEvent::TunnelDown {
        tunnel_id: Uuid::new_v4(),
        interface_name: "wg_ward0".to_owned(),
        reason: "test".to_owned(),
        timestamp: Utc::now(),
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;
}

#[tokio::test]
async fn routing_service_error_does_not_panic_on_routing_rule_change() {
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing: Arc<dyn RoutingService> = Arc::new(FailingRoutingService);

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing, &parent);

    bus.publish(WardnetEvent::RoutingRuleChanged {
        device_id: Uuid::new_v4(),
        target: RoutingTarget::Tunnel {
            tunnel_id: Uuid::new_v4(),
        },
        previous_target: None,
        changed_by: RuleCreator::User,
        timestamp: Utc::now(),
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;
}

#[tokio::test]
async fn routing_service_error_does_not_panic_on_device_discovered() {
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing: Arc<dyn RoutingService> = Arc::new(FailingRoutingService);

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing, &parent);

    bus.publish(WardnetEvent::DeviceDiscovered {
        device_id: Uuid::new_v4(),
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
        ip: "192.168.1.10".to_owned(),
        hostname: None,
        timestamp: Utc::now(),
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;
}

#[tokio::test]
async fn route_table_lost_triggers_handle_route_table_lost() {
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing = Arc::new(MockRoutingService::new());

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing.clone(), &parent);

    bus.publish(WardnetEvent::RouteTableLost {
        table: 100,
        timestamp: Utc::now(),
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;

    let calls = routing.take_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0], RoutingCall::HandleRouteTableLost { table: 100 });
}

#[tokio::test]
async fn route_table_lost_error_does_not_panic() {
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing = Arc::new(MockRoutingService::new());
    *routing.error_on_route_table_lost.lock().unwrap() = true;

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing.clone(), &parent);

    bus.publish(WardnetEvent::RouteTableLost {
        table: 100,
        timestamp: Utc::now(),
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;

    let calls = routing.take_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0], RoutingCall::HandleRouteTableLost { table: 100 });
}
