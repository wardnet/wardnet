use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;
use wardnet_types::device::{Device, DeviceType};
use wardnet_types::event::WardnetEvent;
use wardnet_types::routing::{RoutingRule, RoutingTarget, RuleCreator};

use crate::error::AppError;
use crate::event::{BroadcastEventBus, EventPublisher};
use crate::repository::{DeviceRepository, DeviceRow};
use crate::routing_listener::RoutingListener;
use crate::service::RoutingService;

// ---------------------------------------------------------------------------
// MockRoutingService
// ---------------------------------------------------------------------------

/// Records which methods were called so tests can assert dispatch behaviour.
struct MockRoutingService {
    calls: Mutex<Vec<RoutingCall>>,
}

/// Describes a single call made to the mock routing service.
#[derive(Debug, Clone, PartialEq, Eq)]
enum RoutingCall {
    ApplyRule {
        device_id: Uuid,
        device_ip: String,
        target: RoutingTarget,
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
}

impl MockRoutingService {
    fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
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

    async fn devices_using_tunnel(&self, _tunnel_id: Uuid) -> Result<Vec<Uuid>, AppError> {
        Ok(Vec::new())
    }
}

// ---------------------------------------------------------------------------
// MockDeviceRepo
// ---------------------------------------------------------------------------

/// Mock device repository that returns a pre-configured device and optional rule.
struct MockDeviceRepo {
    device: Mutex<Option<Device>>,
    rule: Mutex<Option<RoutingRule>>,
}

impl MockDeviceRepo {
    fn with_device(device: Device) -> Self {
        Self {
            device: Mutex::new(Some(device)),
            rule: Mutex::new(None),
        }
    }

    fn with_device_and_rule(device: Device, rule: RoutingRule) -> Self {
        Self {
            device: Mutex::new(Some(device)),
            rule: Mutex::new(Some(rule)),
        }
    }

    fn empty() -> Self {
        Self {
            device: Mutex::new(None),
            rule: Mutex::new(None),
        }
    }
}

#[async_trait]
impl DeviceRepository for MockDeviceRepo {
    async fn find_by_ip(&self, _ip: &str) -> anyhow::Result<Option<Device>> {
        Ok(self.device.lock().unwrap().clone())
    }

    async fn find_by_id(&self, _id: &str) -> anyhow::Result<Option<Device>> {
        Ok(self.device.lock().unwrap().clone())
    }

    async fn find_by_mac(&self, _mac: &str) -> anyhow::Result<Option<Device>> {
        Ok(self.device.lock().unwrap().clone())
    }

    async fn find_all(&self) -> anyhow::Result<Vec<Device>> {
        Ok(self.device.lock().unwrap().clone().into_iter().collect())
    }

    async fn insert(&self, _device: &DeviceRow) -> anyhow::Result<()> {
        Ok(())
    }

    async fn update_last_seen_and_ip(
        &self,
        _id: &str,
        _ip: &str,
        _last_seen: &str,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn update_last_seen_batch(&self, _updates: &[(String, String)]) -> anyhow::Result<()> {
        Ok(())
    }

    async fn update_hostname(&self, _id: &str, _hostname: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn update_name_and_type(
        &self,
        _id: &str,
        _name: Option<&str>,
        _device_type: &str,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn find_stale(&self, _before: &str) -> anyhow::Result<Vec<Device>> {
        Ok(Vec::new())
    }

    async fn find_rule_for_device(&self, _device_id: &str) -> anyhow::Result<Option<RoutingRule>> {
        Ok(self.rule.lock().unwrap().clone())
    }

    async fn upsert_user_rule(
        &self,
        _device_id: &str,
        _target_json: &str,
        _now: &str,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn update_admin_locked(&self, _id: &str, _locked: bool) -> anyhow::Result<()> {
        Ok(())
    }

    async fn switch_tunnel_rules_to_direct(
        &self,
        _tid: &str,
        _now: &str,
    ) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }

    async fn count(&self) -> anyhow::Result<i64> {
        Ok(i64::from(self.device.lock().unwrap().is_some()))
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a test device with the given ID and IP.
fn make_device(id: Uuid, ip: &str) -> Device {
    Device {
        id,
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
        name: None,
        hostname: None,
        manufacturer: None,
        device_type: DeviceType::Unknown,
        first_seen: Utc::now(),
        last_seen: Utc::now(),
        last_ip: ip.to_owned(),
        admin_locked: false,
    }
}

// ---------------------------------------------------------------------------
// Failing Mock Services (for error branch coverage)
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

    async fn devices_using_tunnel(&self, _tunnel_id: Uuid) -> Result<Vec<Uuid>, AppError> {
        Ok(Vec::new())
    }
}

/// A device repository that always returns errors.
struct FailingDeviceRepo;

#[async_trait]
impl DeviceRepository for FailingDeviceRepo {
    async fn find_by_ip(&self, _ip: &str) -> anyhow::Result<Option<Device>> {
        anyhow::bail!("find_by_ip failed")
    }

    async fn find_by_id(&self, _id: &str) -> anyhow::Result<Option<Device>> {
        anyhow::bail!("find_by_id failed")
    }

    async fn find_by_mac(&self, _mac: &str) -> anyhow::Result<Option<Device>> {
        anyhow::bail!("find_by_mac failed")
    }

    async fn find_all(&self) -> anyhow::Result<Vec<Device>> {
        anyhow::bail!("find_all failed")
    }

    async fn insert(&self, _device: &DeviceRow) -> anyhow::Result<()> {
        anyhow::bail!("insert failed")
    }

    async fn update_last_seen_and_ip(
        &self,
        _id: &str,
        _ip: &str,
        _last_seen: &str,
    ) -> anyhow::Result<()> {
        anyhow::bail!("update failed")
    }

    async fn update_last_seen_batch(&self, _updates: &[(String, String)]) -> anyhow::Result<()> {
        anyhow::bail!("update batch failed")
    }

    async fn update_hostname(&self, _id: &str, _hostname: &str) -> anyhow::Result<()> {
        anyhow::bail!("update hostname failed")
    }

    async fn update_name_and_type(
        &self,
        _id: &str,
        _name: Option<&str>,
        _device_type: &str,
    ) -> anyhow::Result<()> {
        anyhow::bail!("update name_and_type failed")
    }

    async fn find_stale(&self, _before: &str) -> anyhow::Result<Vec<Device>> {
        anyhow::bail!("find_stale failed")
    }

    async fn find_rule_for_device(&self, _device_id: &str) -> anyhow::Result<Option<RoutingRule>> {
        anyhow::bail!("find_rule failed")
    }

    async fn upsert_user_rule(
        &self,
        _device_id: &str,
        _target_json: &str,
        _now: &str,
    ) -> anyhow::Result<()> {
        anyhow::bail!("upsert rule failed")
    }

    async fn switch_tunnel_rules_to_direct(
        &self,
        _tid: &str,
        _now: &str,
    ) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }

    async fn update_admin_locked(&self, _id: &str, _locked: bool) -> anyhow::Result<()> {
        anyhow::bail!("update admin locked failed")
    }

    async fn count(&self) -> anyhow::Result<i64> {
        anyhow::bail!("count failed")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
#[allow(clippy::similar_names)]
async fn routing_rule_changed_triggers_apply_rule() {
    let device_id = Uuid::new_v4();
    let device_ip = "192.168.1.50";
    let tunnel_id = Uuid::new_v4();
    let target = RoutingTarget::Tunnel { tunnel_id };

    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing = Arc::new(MockRoutingService::new());
    let devices = Arc::new(MockDeviceRepo::with_device(make_device(
        device_id, device_ip,
    )));

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing.clone(), devices, &parent);

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
        RoutingCall::ApplyRule {
            device_id,
            device_ip: device_ip.to_owned(),
            target,
        }
    );
}

#[tokio::test]
async fn device_gone_triggers_remove_device_routes() {
    let device_id = Uuid::new_v4();
    let last_ip = "192.168.1.42";

    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing = Arc::new(MockRoutingService::new());
    let devices = Arc::new(MockDeviceRepo::empty());

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing.clone(), devices, &parent);

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
    let devices = Arc::new(MockDeviceRepo::empty());

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing.clone(), devices, &parent);

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
    let devices = Arc::new(MockDeviceRepo::empty());

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing.clone(), devices, &parent);

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
async fn device_discovered_with_rule_triggers_apply_rule() {
    let device_id = Uuid::new_v4();
    let ip = "192.168.1.99";
    let tunnel_id = Uuid::new_v4();
    let target = RoutingTarget::Tunnel { tunnel_id };

    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing = Arc::new(MockRoutingService::new());
    let devices = Arc::new(MockDeviceRepo::with_device_and_rule(
        make_device(device_id, ip),
        RoutingRule {
            device_id,
            target: target.clone(),
            created_by: RuleCreator::User,
        },
    ));

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing.clone(), devices, &parent);

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
        RoutingCall::ApplyRule {
            device_id,
            device_ip: ip.to_owned(),
            target,
        }
    );
}

#[tokio::test]
async fn device_discovered_without_rule_is_ignored() {
    let device_id = Uuid::new_v4();
    let ip = "192.168.1.99";

    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing = Arc::new(MockRoutingService::new());
    // Device exists but has no routing rule.
    let devices = Arc::new(MockDeviceRepo::with_device(make_device(device_id, ip)));

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing.clone(), devices, &parent);

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
    assert!(
        calls.is_empty(),
        "no routing call expected when device has no rule"
    );
}

#[tokio::test]
async fn device_ip_changed_triggers_handle_ip_change() {
    let device_id = Uuid::new_v4();
    let old_ip = "192.168.1.10";
    let new_ip = "192.168.1.20";

    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing = Arc::new(MockRoutingService::new());
    let devices = Arc::new(MockDeviceRepo::empty());

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing.clone(), devices, &parent);

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
    let devices = Arc::new(MockDeviceRepo::empty());

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing.clone(), devices, &parent);

    listener.shutdown().await;

    let calls = routing.take_calls();
    assert!(calls.is_empty());
}

#[tokio::test]
async fn unrelated_events_are_ignored() {
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing = Arc::new(MockRoutingService::new());
    let devices = Arc::new(MockDeviceRepo::empty());

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing.clone(), devices, &parent);

    // TunnelStatsUpdated is not handled by routing listener.
    bus.publish(WardnetEvent::TunnelStatsUpdated {
        tunnel_id: Uuid::new_v4(),
        status: wardnet_types::tunnel::TunnelStatus::Up,
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
    let devices: Arc<dyn DeviceRepository> = Arc::new(MockDeviceRepo::empty());

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing, devices, &parent);

    bus.publish(WardnetEvent::DeviceIpChanged {
        device_id: Uuid::new_v4(),
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
        old_ip: "192.168.1.10".to_owned(),
        new_ip: "192.168.1.20".to_owned(),
        timestamp: Utc::now(),
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;
    // Error is logged but no panic.
}

#[tokio::test]
async fn routing_service_error_does_not_panic_on_device_gone() {
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing: Arc<dyn RoutingService> = Arc::new(FailingRoutingService);
    let devices: Arc<dyn DeviceRepository> = Arc::new(MockDeviceRepo::empty());

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing, devices, &parent);

    bus.publish(WardnetEvent::DeviceGone {
        device_id: Uuid::new_v4(),
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
        last_ip: "192.168.1.10".to_owned(),
        timestamp: Utc::now(),
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;
    // Error is logged but no panic.
}

#[tokio::test]
async fn routing_service_error_does_not_panic_on_tunnel_up() {
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing: Arc<dyn RoutingService> = Arc::new(FailingRoutingService);
    let devices: Arc<dyn DeviceRepository> = Arc::new(MockDeviceRepo::empty());

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing, devices, &parent);

    bus.publish(WardnetEvent::TunnelUp {
        tunnel_id: Uuid::new_v4(),
        interface_name: "wg_ward0".to_owned(),
        endpoint: "198.51.100.1:51820".to_owned(),
        timestamp: Utc::now(),
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;
    // Error is logged but no panic.
}

#[tokio::test]
async fn routing_service_error_does_not_panic_on_tunnel_down() {
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing: Arc<dyn RoutingService> = Arc::new(FailingRoutingService);
    let devices: Arc<dyn DeviceRepository> = Arc::new(MockDeviceRepo::empty());

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing, devices, &parent);

    bus.publish(WardnetEvent::TunnelDown {
        tunnel_id: Uuid::new_v4(),
        interface_name: "wg_ward0".to_owned(),
        reason: "test".to_owned(),
        timestamp: Utc::now(),
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;
    // Error is logged but no panic.
}

#[tokio::test]
async fn device_repo_error_does_not_panic_on_routing_rule_change() {
    let device_id = Uuid::new_v4();
    let tunnel_id = Uuid::new_v4();

    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing = Arc::new(MockRoutingService::new());
    let devices: Arc<dyn DeviceRepository> = Arc::new(FailingDeviceRepo);

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing.clone(), devices, &parent);

    bus.publish(WardnetEvent::RoutingRuleChanged {
        device_id,
        target: RoutingTarget::Tunnel { tunnel_id },
        previous_target: None,
        changed_by: RuleCreator::User,
        timestamp: Utc::now(),
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;

    // find_by_id fails, so apply_rule is never called.
    let calls = routing.take_calls();
    assert!(
        calls.is_empty(),
        "no routing calls expected when device lookup fails"
    );
}

#[tokio::test]
async fn device_repo_error_does_not_panic_on_device_discovered() {
    let device_id = Uuid::new_v4();

    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing = Arc::new(MockRoutingService::new());
    let devices: Arc<dyn DeviceRepository> = Arc::new(FailingDeviceRepo);

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing.clone(), devices, &parent);

    bus.publish(WardnetEvent::DeviceDiscovered {
        device_id,
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
        ip: "192.168.1.10".to_owned(),
        hostname: None,
        timestamp: Utc::now(),
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;

    // find_rule_for_device fails, so no routing calls.
    let calls = routing.take_calls();
    assert!(
        calls.is_empty(),
        "no routing calls expected when device repo fails"
    );
}

#[tokio::test]
#[allow(clippy::similar_names)]
async fn routing_service_error_on_apply_rule_does_not_panic() {
    let device_id = Uuid::new_v4();
    let tunnel_id = Uuid::new_v4();
    let device_ip = "192.168.1.50";

    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing: Arc<dyn RoutingService> = Arc::new(FailingRoutingService);
    let devices: Arc<dyn DeviceRepository> = Arc::new(MockDeviceRepo::with_device(make_device(
        device_id, device_ip,
    )));

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing, devices, &parent);

    bus.publish(WardnetEvent::RoutingRuleChanged {
        device_id,
        target: RoutingTarget::Tunnel { tunnel_id },
        previous_target: None,
        changed_by: RuleCreator::User,
        timestamp: Utc::now(),
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;
    // apply_rule returns error, which is logged but no panic.
}

#[tokio::test]
async fn routing_rule_changed_device_not_found_does_not_panic() {
    let device_id = Uuid::new_v4();
    let tunnel_id = Uuid::new_v4();

    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let routing = Arc::new(MockRoutingService::new());
    // Empty repo: device lookup returns None.
    let devices: Arc<dyn DeviceRepository> = Arc::new(MockDeviceRepo::empty());

    let parent = tracing::info_span!("test");
    let listener = RoutingListener::start(&bus, routing.clone(), devices, &parent);

    bus.publish(WardnetEvent::RoutingRuleChanged {
        device_id,
        target: RoutingTarget::Tunnel { tunnel_id },
        previous_target: None,
        changed_by: RuleCreator::User,
        timestamp: Utc::now(),
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;

    // Device not found, so no apply_rule call.
    let calls = routing.take_calls();
    assert!(
        calls.is_empty(),
        "no routing calls expected when device not found"
    );
}
