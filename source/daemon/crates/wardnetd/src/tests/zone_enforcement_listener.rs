//! Dispatch tests for [`ZoneEnforcementListener`]: publish each event on the
//! bus and assert it maps to the right [`ZoneEnforcementService`] call (and that
//! irrelevant events map to none, and a failing call is swallowed).

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;
use wardnet_common::event::WardnetEvent;

use wardnetd_services::ZoneEnforcementService;
use wardnetd_services::error::AppError;
use wardnetd_services::event::{BroadcastEventBus, EventPublisher};

use crate::zone_enforcement_listener::ZoneEnforcementListener;

/// One recorded call to the mock enforcer.
#[derive(Debug, Clone, PartialEq, Eq)]
enum EnforceCall {
    ApplyZone(Uuid),
    ApplyDevice(Uuid),
    HandleIpChange {
        device_id: Uuid,
        old_ip: String,
        new_ip: String,
    },
    RemoveDevice {
        device_id: Uuid,
        last_ip: String,
    },
    DefaultPolicyChanged(String),
}

struct MockEnforcer {
    calls: Mutex<Vec<EnforceCall>>,
    /// When `true`, every method returns an error (to exercise the listener's
    /// error-logging branch without panicking).
    fail: bool,
}

impl MockEnforcer {
    fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            fail: false,
        }
    }

    fn failing() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            fail: true,
        }
    }

    fn record(&self, call: EnforceCall) -> Result<(), AppError> {
        self.calls.lock().unwrap().push(call);
        if self.fail {
            Err(AppError::Internal(anyhow::anyhow!("mock enforcer failed")))
        } else {
            Ok(())
        }
    }

    fn take_calls(&self) -> Vec<EnforceCall> {
        std::mem::take(&mut *self.calls.lock().unwrap())
    }
}

#[async_trait]
impl ZoneEnforcementService for MockEnforcer {
    async fn reconcile(&self) -> Result<(), AppError> {
        Ok(())
    }

    async fn apply_device(&self, device_id: Uuid) -> Result<(), AppError> {
        self.record(EnforceCall::ApplyDevice(device_id))
    }

    async fn apply_zone(&self, zone_id: Uuid) -> Result<(), AppError> {
        self.record(EnforceCall::ApplyZone(zone_id))
    }

    async fn handle_ip_change(
        &self,
        device_id: Uuid,
        old_ip: &str,
        new_ip: &str,
    ) -> Result<(), AppError> {
        self.record(EnforceCall::HandleIpChange {
            device_id,
            old_ip: old_ip.to_owned(),
            new_ip: new_ip.to_owned(),
        })
    }

    async fn remove_device(&self, device_id: Uuid, last_ip: &str) -> Result<(), AppError> {
        self.record(EnforceCall::RemoveDevice {
            device_id,
            last_ip: last_ip.to_owned(),
        })
    }

    async fn handle_default_policy_changed(&self, policy: &str) -> Result<(), AppError> {
        self.record(EnforceCall::DefaultPolicyChanged(policy.to_owned()))
    }
}

/// Publish `event`, let the listener drain it, and return the recorded calls.
async fn dispatch(enforcer: Arc<MockEnforcer>, event: WardnetEvent) -> Vec<EnforceCall> {
    let bus: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(16));
    let parent = tracing::info_span!("test");
    let listener = ZoneEnforcementListener::start(&bus, enforcer.clone(), &parent);

    bus.publish(event);
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    listener.shutdown().await;

    enforcer.take_calls()
}

#[tokio::test]
async fn network_zone_changed_triggers_apply_zone() {
    let zone_id = Uuid::new_v4();
    let enforcer = Arc::new(MockEnforcer::new());
    let calls = dispatch(
        enforcer,
        WardnetEvent::NetworkZoneChanged {
            zone_id,
            timestamp: Utc::now(),
        },
    )
    .await;
    assert_eq!(calls, vec![EnforceCall::ApplyZone(zone_id)]);
}

#[tokio::test]
async fn device_zone_changed_triggers_apply_device() {
    let device_id = Uuid::new_v4();
    let enforcer = Arc::new(MockEnforcer::new());
    let calls = dispatch(
        enforcer,
        WardnetEvent::DeviceZoneChanged {
            device_id,
            old_zone_id: Uuid::new_v4(),
            new_zone_id: Uuid::new_v4(),
            timestamp: Utc::now(),
        },
    )
    .await;
    assert_eq!(calls, vec![EnforceCall::ApplyDevice(device_id)]);
}

#[tokio::test]
async fn device_discovered_triggers_apply_device() {
    let device_id = Uuid::new_v4();
    let enforcer = Arc::new(MockEnforcer::new());
    let calls = dispatch(
        enforcer,
        WardnetEvent::DeviceDiscovered {
            device_id,
            mac: "02:00:00:00:00:01".to_owned(),
            ip: "192.168.1.10".to_owned(),
            hostname: None,
            timestamp: Utc::now(),
        },
    )
    .await;
    assert_eq!(calls, vec![EnforceCall::ApplyDevice(device_id)]);
}

#[tokio::test]
async fn device_ip_changed_triggers_handle_ip_change() {
    let device_id = Uuid::new_v4();
    let enforcer = Arc::new(MockEnforcer::new());
    let calls = dispatch(
        enforcer,
        WardnetEvent::DeviceIpChanged {
            device_id,
            mac: "02:00:00:00:00:02".to_owned(),
            old_ip: "192.168.1.20".to_owned(),
            new_ip: "192.168.1.21".to_owned(),
            timestamp: Utc::now(),
        },
    )
    .await;
    assert_eq!(
        calls,
        vec![EnforceCall::HandleIpChange {
            device_id,
            old_ip: "192.168.1.20".to_owned(),
            new_ip: "192.168.1.21".to_owned(),
        }]
    );
}

#[tokio::test]
async fn device_gone_triggers_remove_device() {
    let device_id = Uuid::new_v4();
    let enforcer = Arc::new(MockEnforcer::new());
    let calls = dispatch(
        enforcer,
        WardnetEvent::DeviceGone {
            device_id,
            mac: "02:00:00:00:00:03".to_owned(),
            last_ip: "192.168.1.30".to_owned(),
            timestamp: Utc::now(),
        },
    )
    .await;
    assert_eq!(
        calls,
        vec![EnforceCall::RemoveDevice {
            device_id,
            last_ip: "192.168.1.30".to_owned(),
        }]
    );
}

#[tokio::test]
async fn default_policy_changed_triggers_handler() {
    let enforcer = Arc::new(MockEnforcer::new());
    let calls = dispatch(
        enforcer,
        WardnetEvent::DefaultPolicyChanged {
            policy: "direct".to_owned(),
            timestamp: Utc::now(),
        },
    )
    .await;
    assert_eq!(
        calls,
        vec![EnforceCall::DefaultPolicyChanged("direct".to_owned())]
    );
}

#[tokio::test]
async fn irrelevant_event_triggers_nothing() {
    let enforcer = Arc::new(MockEnforcer::new());
    let calls = dispatch(
        enforcer,
        WardnetEvent::TunnelUp {
            tunnel_id: Uuid::new_v4(),
            interface_name: "wg_ward0".to_owned(),
            endpoint: "1.2.3.4:51820".to_owned(),
            timestamp: Utc::now(),
        },
    )
    .await;
    assert!(
        calls.is_empty(),
        "routing/tunnel events are ignored: {calls:?}"
    );
}

#[tokio::test]
async fn failing_enforcer_call_is_swallowed() {
    // A returned error must be logged, not propagated (the listener keeps running).
    let device_id = Uuid::new_v4();
    let enforcer = Arc::new(MockEnforcer::failing());
    let calls = dispatch(
        enforcer,
        WardnetEvent::DeviceZoneChanged {
            device_id,
            old_zone_id: Uuid::new_v4(),
            new_zone_id: Uuid::new_v4(),
            timestamp: Utc::now(),
        },
    )
    .await;
    // The call was still dispatched (and its error swallowed by the listener).
    assert_eq!(calls, vec![EnforceCall::ApplyDevice(device_id)]);
}
