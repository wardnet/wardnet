use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::api::{DeviceMeResponse, DnsCaptureSettingsResponse, SetMyRuleResponse};
use wardnet_common::device::{Device, DeviceType};
use wardnet_common::routing::RoutingTarget;
use wardnet_common::rule_request::{DeviceRuleRequest, RuleRequestKind, RuleRequestStatus};
use wardnetd_data::repository::RuleRequestRepository;

use crate::auth_context;
use crate::device::DeviceService;
use crate::error::AppError;
use crate::event::EventPublisher;
use crate::rule_request::{RuleRequestService, RuleRequestServiceImpl};
use wardnet_common::auth::AuthContext;
use wardnet_common::event::WardnetEvent;

const DEVICE_ID: &str = "00000000-0000-0000-0000-000000000001";

// --- Mock repository --------------------------------------------------------

#[derive(Default)]
struct MockRuleRequestRepo {
    decide_returns_none: bool,
}

#[async_trait]
impl RuleRequestRepository for MockRuleRequestRepo {
    async fn insert(
        &self,
        id: &str,
        device_id: &str,
        kind: RuleRequestKind,
        domain: &str,
        reason: Option<&str>,
        created_at: &str,
    ) -> anyhow::Result<DeviceRuleRequest> {
        Ok(DeviceRuleRequest {
            id: id.to_owned(),
            device_id: device_id.to_owned(),
            kind,
            domain: domain.to_owned(),
            reason: reason.map(str::to_owned),
            status: RuleRequestStatus::Pending,
            created_at: created_at.to_owned(),
            decided_at: None,
            decided_by: None,
        })
    }

    async fn list_by_device(&self, device_id: &str) -> anyhow::Result<Vec<DeviceRuleRequest>> {
        Ok(vec![sample(device_id, RuleRequestStatus::Pending)])
    }

    async fn list_all(
        &self,
        _status: Option<RuleRequestStatus>,
    ) -> anyhow::Result<Vec<DeviceRuleRequest>> {
        Ok(vec![sample(DEVICE_ID, RuleRequestStatus::Pending)])
    }

    async fn update_status(
        &self,
        id: &str,
        status: RuleRequestStatus,
        decided_by: &str,
        decided_at: &str,
    ) -> anyhow::Result<Option<DeviceRuleRequest>> {
        if self.decide_returns_none {
            return Ok(None);
        }
        Ok(Some(DeviceRuleRequest {
            id: id.to_owned(),
            status,
            decided_by: Some(decided_by.to_owned()),
            decided_at: Some(decided_at.to_owned()),
            ..sample(DEVICE_ID, status)
        }))
    }
}

fn sample(device_id: &str, status: RuleRequestStatus) -> DeviceRuleRequest {
    DeviceRuleRequest {
        id: "req-1".to_owned(),
        device_id: device_id.to_owned(),
        kind: RuleRequestKind::Block,
        domain: "ads.example.com".to_owned(),
        reason: None,
        status,
        created_at: "2026-06-18T00:00:00Z".to_owned(),
        decided_at: None,
        decided_by: None,
    }
}

// --- Mock device service (only get_device_for_ip matters) -------------------

struct MockDeviceService {
    found: bool,
}

fn sample_device() -> Device {
    Device {
        id: Uuid::parse_str(DEVICE_ID).unwrap(),
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
        name: None,
        hostname: None,
        manufacturer: None,
        device_type: DeviceType::Unknown,
        first_seen: chrono::Utc::now(),
        last_seen: chrono::Utc::now(),
        last_ip: "192.168.1.100".to_owned(),
        admin_locked: false,
        zone_id: "00000000-0000-0000-0000-000000000201".parse().unwrap(),
        dns_capture_enabled: false,
        dns_capture_cap_count: 1000,
        dns_capture_cap_days: 7,
        connection_mode: wardnet_common::device::DeviceConnectionMode::Lan,
    }
}

#[async_trait]
impl DeviceService for MockDeviceService {
    async fn get_device(
        &self,
        _device_id: &str,
    ) -> Result<Option<wardnet_common::device::Device>, AppError> {
        unimplemented!("not used by rule-request tests")
    }
    async fn clear_remote_connection_mode(&self, _device_id: &str) -> Result<(), AppError> {
        unimplemented!("not used by rule-request tests")
    }
    async fn get_device_for_ip(&self, _ip: &str) -> Result<DeviceMeResponse, AppError> {
        Ok(DeviceMeResponse {
            device: if self.found {
                Some(sample_device())
            } else {
                None
            },
            current_rule: None,
            admin_locked: false,
            available_tunnels: vec![],
            zone: None,
            routing_profiles: vec![],
        })
    }
    async fn set_rule_for_ip(
        &self,
        _ip: &str,
        _t: RoutingTarget,
    ) -> Result<SetMyRuleResponse, AppError> {
        unimplemented!()
    }
    async fn set_rule(&self, _id: &str, _t: RoutingTarget) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn current_rules(&self) -> Result<HashMap<Uuid, RoutingTarget>, AppError> {
        unimplemented!()
    }
    async fn get_rule_for_device(
        &self,
        _device_id: &str,
    ) -> Result<Option<RoutingTarget>, AppError> {
        unimplemented!()
    }
    async fn update_admin_locked(&self, _id: &str, _locked: bool) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn get_dns_capture_settings(
        &self,
        _id: &str,
    ) -> Result<DnsCaptureSettingsResponse, AppError> {
        unimplemented!()
    }
    async fn update_dns_capture_settings(
        &self,
        _id: &str,
        _enabled: Option<bool>,
        _cap_count: Option<i64>,
        _cap_days: Option<i64>,
    ) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn set_my_capture_enabled(
        &self,
        _ip: &str,
        _enabled: bool,
    ) -> Result<DnsCaptureSettingsResponse, AppError> {
        unimplemented!()
    }
    async fn fetch_pending_dns_events(
        &self,
        _device_id: &str,
        _after_id: i64,
        _limit: i64,
    ) -> Result<Vec<wardnet_common::api::DnsEventItem>, AppError> {
        unimplemented!()
    }
    async fn ack_dns_events(&self, _device_id: &str, _up_to_id: i64) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn list_capture_enabled_device_ids(&self) -> Result<Vec<String>, AppError> {
        unimplemented!()
    }
    async fn get_device_capture_settings(
        &self,
        _device_id: &str,
    ) -> Result<Option<(bool, i64, i64)>, AppError> {
        unimplemented!()
    }
}

/// Captures published events so tests can assert on them.
#[derive(Default)]
struct CapturingEventPublisher {
    events: std::sync::Mutex<Vec<WardnetEvent>>,
}

impl EventPublisher for CapturingEventPublisher {
    fn publish(&self, event: WardnetEvent) {
        self.events.lock().unwrap().push(event);
    }
    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<WardnetEvent> {
        unimplemented!()
    }
}

fn build(found: bool, decide_none: bool) -> RuleRequestServiceImpl {
    build_with_events(found, decide_none).0
}

fn build_with_events(
    found: bool,
    decide_none: bool,
) -> (RuleRequestServiceImpl, Arc<CapturingEventPublisher>) {
    let events = Arc::new(CapturingEventPublisher::default());
    let service = RuleRequestServiceImpl::new(
        Arc::new(MockRuleRequestRepo {
            decide_returns_none: decide_none,
        }),
        Arc::new(MockDeviceService { found }),
        events.clone(),
    );
    (service, events)
}

fn admin_ctx() -> AuthContext {
    AuthContext::Admin {
        admin_id: Uuid::parse_str("00000000-0000-0000-0000-000000000099").unwrap(),
    }
}

// --- Tests ------------------------------------------------------------------

#[tokio::test]
async fn create_normalizes_domain_and_resolves_device() {
    let svc = build(true, false);
    let req = svc
        .create_for_ip(
            "1.2.3.4",
            RuleRequestKind::Block,
            "  ADS.Example.COM ",
            Some("  ".into()),
        )
        .await
        .unwrap();
    assert_eq!(req.domain, "ads.example.com");
    assert_eq!(req.device_id, DEVICE_ID);
    // Whitespace-only reason is dropped.
    assert_eq!(req.reason, None);
}

#[tokio::test]
async fn create_publishes_rule_request_created_event() {
    let (svc, events) = build_with_events(true, false);
    let req = svc
        .create_for_ip("1.2.3.4", RuleRequestKind::Allow, "blocked.example", None)
        .await
        .unwrap();

    let published = events.events.lock().unwrap();
    assert_eq!(published.len(), 1);
    match &published[0] {
        WardnetEvent::RuleRequestCreated {
            request_id,
            device_id,
            kind,
            domain,
            ..
        } => {
            assert_eq!(request_id, &req.id);
            assert_eq!(device_id, DEVICE_ID);
            assert_eq!(*kind, RuleRequestKind::Allow);
            assert_eq!(domain, "blocked.example");
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[tokio::test]
async fn create_rejects_empty_domain() {
    let svc = build(true, false);
    let err = svc
        .create_for_ip("1.2.3.4", RuleRequestKind::Block, "   ", None)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::BadRequest(_)));
}

#[tokio::test]
async fn create_unknown_ip_returns_not_found() {
    let svc = build(false, false);
    let err = svc
        .create_for_ip("9.9.9.9", RuleRequestKind::Allow, "x.com", None)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::NotFound(_)));
}

#[tokio::test]
async fn admin_list_requires_admin() {
    let svc = build(true, false);
    let err = auth_context::with_context(AuthContext::Anonymous, async { svc.list(None).await })
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Forbidden(_)));
}

#[tokio::test]
async fn admin_list_ok_with_admin_context() {
    let svc = build(true, false);
    let out = auth_context::with_context(admin_ctx(), async { svc.list(None).await })
        .await
        .unwrap();
    assert_eq!(out.len(), 1);
}

#[tokio::test]
async fn decide_rejects_pending_status() {
    let svc = build(true, false);
    let err = auth_context::with_context(admin_ctx(), async {
        svc.decide("req-1", RuleRequestStatus::Pending).await
    })
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::BadRequest(_)));
}

#[tokio::test]
async fn decide_unknown_returns_not_found() {
    let svc = build(true, true);
    let err = auth_context::with_context(admin_ctx(), async {
        svc.decide("nope", RuleRequestStatus::Approved).await
    })
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::NotFound(_)));
}

#[tokio::test]
async fn decide_ok_records_admin_and_status() {
    let svc = build(true, false);
    let req = auth_context::with_context(admin_ctx(), async {
        svc.decide("req-1", RuleRequestStatus::Approved).await
    })
    .await
    .unwrap();
    assert_eq!(req.status, RuleRequestStatus::Approved);
    assert!(req.decided_by.is_some());
    assert!(req.decided_at.is_some());
}
