use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_types::device::{Device, DeviceType};
use wardnet_types::routing::{RoutingRule, RoutingTarget, RuleCreator};

use crate::repository::DeviceRepository;
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
    async fn find_rule_for_device(&self, _id: &str) -> anyhow::Result<Option<RoutingRule>> {
        Ok(self.rule.clone())
    }
    async fn upsert_user_rule(&self, _id: &str, _json: &str, _now: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn count(&self) -> anyhow::Result<i64> {
        Ok(0)
    }
}

// -- Helpers --------------------------------------------------------------

fn sample_device(locked: bool) -> Device {
    Device {
        id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
        name: Some("My Phone".to_owned()),
        hostname: None,
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

// -- Tests ----------------------------------------------------------------

#[tokio::test]
async fn get_device_found_with_rule() {
    let svc = DeviceServiceImpl::new(Arc::new(MockDeviceRepo {
        device: Some(sample_device(false)),
        rule: Some(sample_rule()),
    }));

    let resp = svc.get_device_for_ip("192.168.1.10").await.unwrap();
    assert!(resp.device.is_some());
    assert_eq!(resp.current_rule, Some(RoutingTarget::Direct));
    assert!(!resp.admin_locked);
}

#[tokio::test]
async fn get_device_found_no_rule() {
    let svc = DeviceServiceImpl::new(Arc::new(MockDeviceRepo {
        device: Some(sample_device(false)),
        rule: None,
    }));

    let resp = svc.get_device_for_ip("192.168.1.10").await.unwrap();
    assert!(resp.device.is_some());
    assert!(resp.current_rule.is_none());
}

#[tokio::test]
async fn get_device_not_found() {
    let svc = DeviceServiceImpl::new(Arc::new(MockDeviceRepo {
        device: None,
        rule: None,
    }));

    let resp = svc.get_device_for_ip("10.0.0.99").await.unwrap();
    assert!(resp.device.is_none());
    assert!(resp.current_rule.is_none());
    assert!(!resp.admin_locked);
}

#[tokio::test]
async fn set_rule_success() {
    let svc = DeviceServiceImpl::new(Arc::new(MockDeviceRepo {
        device: Some(sample_device(false)),
        rule: None,
    }));

    let resp = svc
        .set_rule_for_ip("192.168.1.10", RoutingTarget::Default)
        .await
        .unwrap();
    assert_eq!(resp.target, RoutingTarget::Default);
    assert_eq!(resp.message, "routing rule updated");
}

#[tokio::test]
async fn set_rule_admin_locked() {
    let svc = DeviceServiceImpl::new(Arc::new(MockDeviceRepo {
        device: Some(sample_device(true)),
        rule: None,
    }));

    let result = svc
        .set_rule_for_ip("192.168.1.10", RoutingTarget::Direct)
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn set_rule_device_not_found() {
    let svc = DeviceServiceImpl::new(Arc::new(MockDeviceRepo {
        device: None,
        rule: None,
    }));

    let result = svc
        .set_rule_for_ip("10.0.0.99", RoutingTarget::Direct)
        .await;
    assert!(result.is_err());
}
