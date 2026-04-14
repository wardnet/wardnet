use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::broadcast;
use uuid::Uuid;
use wardnet_types::auth::AuthContext;
use wardnet_types::device::{Device, DeviceType};
use wardnet_types::event::WardnetEvent;
use wardnet_types::routing::{RoutingRule, RoutingTarget, RuleCreator};

use crate::auth_context;
use crate::event::EventPublisher;
use crate::repository::DeviceRepository;
use crate::repository::device::DeviceRow;
use crate::service::{DeviceService, DeviceServiceImpl};

// -- Mock repository ------------------------------------------------------

struct MockDeviceRepo {
    device: Option<Device>,
    rule: Option<RoutingRule>,
}

#[async_trait]
impl DeviceRepository for MockDeviceRepo {
    async fn find_by_ip(&self, _ip: &str) -> anyhow::Result<Option<Device>> {
        Ok(self.device.clone())
    }
    async fn find_by_id(&self, _id: &str) -> anyhow::Result<Option<Device>> {
        Ok(self.device.clone())
    }
    async fn find_by_mac(&self, _mac: &str) -> anyhow::Result<Option<Device>> {
        Ok(self.device.clone())
    }
    async fn find_all(&self) -> anyhow::Result<Vec<Device>> {
        Ok(self.device.clone().into_iter().collect())
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
        Ok(vec![])
    }
    async fn find_rule_for_device(&self, _id: &str) -> anyhow::Result<Option<RoutingRule>> {
        Ok(self.rule.clone())
    }
    async fn upsert_user_rule(&self, _id: &str, _json: &str, _now: &str) -> anyhow::Result<()> {
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
        Ok(0)
    }
}

// -- Mock event publisher -------------------------------------------------

/// Stub event publisher that discards all events.
struct MockEventPublisher;

impl EventPublisher for MockEventPublisher {
    fn publish(&self, _event: WardnetEvent) {}
    fn subscribe(&self) -> broadcast::Receiver<WardnetEvent> {
        let (tx, rx) = broadcast::channel(1);
        drop(tx);
        rx
    }
}

// -- Helpers --------------------------------------------------------------

fn sample_device(locked: bool) -> Device {
    Device {
        id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
        name: Some("My Phone".to_owned()),
        hostname: None,
        manufacturer: Some("Apple".to_owned()),
        device_type: DeviceType::Phone,
        first_seen: "2026-03-07T00:00:00Z".parse().unwrap(),
        last_seen: "2026-03-07T00:00:00Z".parse().unwrap(),
        last_ip: "192.168.1.10".to_owned(),
        admin_locked: locked,
    }
}

fn sample_rule() -> RoutingRule {
    RoutingRule {
        device_id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
        target: RoutingTarget::Direct,
        created_by: RuleCreator::User,
    }
}

fn admin_ctx() -> AuthContext {
    AuthContext::Admin {
        admin_id: Uuid::new_v4(),
    }
}

fn device_ctx(mac: &str) -> AuthContext {
    AuthContext::Device {
        mac: mac.to_owned(),
    }
}

fn make_svc(locked: bool, rule: Option<RoutingRule>) -> DeviceServiceImpl {
    DeviceServiceImpl::new(
        Arc::new(MockDeviceRepo {
            device: Some(sample_device(locked)),
            rule,
        }),
        Arc::new(MockEventPublisher),
    )
}

fn make_svc_no_device() -> DeviceServiceImpl {
    DeviceServiceImpl::new(
        Arc::new(MockDeviceRepo {
            device: None,
            rule: None,
        }),
        Arc::new(MockEventPublisher),
    )
}

// -- Tests: get_device_for_ip --------------------------------------------

#[tokio::test]
async fn get_device_found_with_rule() {
    let svc = make_svc(false, Some(sample_rule()));

    let resp = svc.get_device_for_ip("192.168.1.10").await.unwrap();
    assert!(resp.device.is_some());
    assert_eq!(resp.current_rule, Some(RoutingTarget::Direct));
    assert!(!resp.admin_locked);
}

#[tokio::test]
async fn get_device_found_no_rule() {
    let svc = make_svc(false, None);

    let resp = svc.get_device_for_ip("192.168.1.10").await.unwrap();
    assert!(resp.device.is_some());
    assert!(resp.current_rule.is_none());
}

#[tokio::test]
async fn get_device_not_found() {
    let svc = make_svc_no_device();

    let resp = svc.get_device_for_ip("10.0.0.99").await.unwrap();
    assert!(resp.device.is_none());
    assert!(resp.current_rule.is_none());
    assert!(!resp.admin_locked);
}

// -- Tests: set_rule_for_ip (auth context) --------------------------------

#[tokio::test]
async fn set_rule_device_context_own_device() {
    let svc = make_svc(false, None);
    let ctx = device_ctx("AA:BB:CC:DD:EE:01");

    let resp = auth_context::with_context(ctx, async {
        svc.set_rule_for_ip("192.168.1.10", RoutingTarget::Default)
            .await
    })
    .await
    .unwrap();

    assert_eq!(resp.target, RoutingTarget::Default);
    assert_eq!(resp.message, "routing rule updated");
}

#[tokio::test]
async fn set_rule_device_context_wrong_device_forbidden() {
    let svc = make_svc(false, None);
    let ctx = device_ctx("FF:FF:FF:FF:FF:FF");

    let result = auth_context::with_context(ctx, async {
        svc.set_rule_for_ip("192.168.1.10", RoutingTarget::Direct)
            .await
    })
    .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn set_rule_admin_locked_device_context_forbidden() {
    let svc = make_svc(true, None);
    let ctx = device_ctx("AA:BB:CC:DD:EE:01");

    let result = auth_context::with_context(ctx, async {
        svc.set_rule_for_ip("192.168.1.10", RoutingTarget::Direct)
            .await
    })
    .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn set_rule_admin_context_bypasses_lock() {
    let svc = make_svc(true, None);
    let ctx = admin_ctx();

    let resp = auth_context::with_context(ctx, async {
        svc.set_rule_for_ip("192.168.1.10", RoutingTarget::Direct)
            .await
    })
    .await
    .unwrap();

    assert_eq!(resp.target, RoutingTarget::Direct);
}

#[tokio::test]
async fn set_rule_anonymous_forbidden() {
    let svc = make_svc(false, None);

    let result = auth_context::with_context(AuthContext::Anonymous, async {
        svc.set_rule_for_ip("192.168.1.10", RoutingTarget::Direct)
            .await
    })
    .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn set_rule_device_not_found() {
    let svc = make_svc_no_device();
    let ctx = device_ctx("AA:BB:CC:DD:EE:01");

    let result = auth_context::with_context(ctx, async {
        svc.set_rule_for_ip("10.0.0.99", RoutingTarget::Direct)
            .await
    })
    .await;

    assert!(result.is_err());
}

// -- Tests: set_rule (by device ID) --------------------------------------

#[tokio::test]
async fn set_rule_by_id_admin_allowed() {
    let svc = make_svc(true, None);
    let ctx = admin_ctx();
    let device_id = "00000000-0000-0000-0000-000000000001";

    auth_context::with_context(ctx, async {
        svc.set_rule(device_id, RoutingTarget::Direct).await
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn set_rule_by_id_device_context_own_device() {
    let svc = make_svc(false, None);
    let ctx = device_ctx("AA:BB:CC:DD:EE:01");
    let device_id = "00000000-0000-0000-0000-000000000001";

    auth_context::with_context(ctx, async {
        svc.set_rule(device_id, RoutingTarget::Default).await
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn set_rule_by_id_device_context_foreign_device_forbidden() {
    let svc = make_svc(false, None);
    let ctx = device_ctx("FF:FF:FF:FF:FF:FF");
    let device_id = "00000000-0000-0000-0000-000000000001";

    let result = auth_context::with_context(ctx, async {
        svc.set_rule(device_id, RoutingTarget::Default).await
    })
    .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn set_rule_by_id_admin_locked_own_device_forbidden() {
    let svc = make_svc(true, None);
    let ctx = device_ctx("AA:BB:CC:DD:EE:01");
    let device_id = "00000000-0000-0000-0000-000000000001";

    let result = auth_context::with_context(ctx, async {
        svc.set_rule(device_id, RoutingTarget::Default).await
    })
    .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn set_rule_by_id_anonymous_forbidden() {
    let svc = make_svc(false, None);
    let device_id = "00000000-0000-0000-0000-000000000001";

    let result = auth_context::with_context(AuthContext::Anonymous, async {
        svc.set_rule(device_id, RoutingTarget::Default).await
    })
    .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn set_rule_by_id_device_not_found() {
    let svc = make_svc_no_device();
    let ctx = admin_ctx();
    let device_id = "00000000-0000-0000-0000-000000000099";

    let result = auth_context::with_context(ctx, async {
        svc.set_rule(device_id, RoutingTarget::Default).await
    })
    .await;

    assert!(result.is_err());
}

// -- Tests: update_admin_locked ------------------------------------------

#[tokio::test]
async fn update_admin_locked_admin_allowed() {
    let svc = make_svc(false, None);
    let ctx = admin_ctx();

    auth_context::with_context(ctx, async {
        svc.update_admin_locked("00000000-0000-0000-0000-000000000001", true)
            .await
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn update_admin_locked_device_context_forbidden() {
    let svc = make_svc(false, None);
    let ctx = device_ctx("AA:BB:CC:DD:EE:01");

    let result = auth_context::with_context(ctx, async {
        svc.update_admin_locked("00000000-0000-0000-0000-000000000001", true)
            .await
    })
    .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn update_admin_locked_anonymous_forbidden() {
    let svc = make_svc(false, None);

    let result = auth_context::with_context(AuthContext::Anonymous, async {
        svc.update_admin_locked("00000000-0000-0000-0000-000000000001", true)
            .await
    })
    .await;

    assert!(result.is_err());
}
