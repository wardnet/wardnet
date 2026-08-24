use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::access_request::{AccessRequestKind, AccessRequestStatus, DeviceAccessRequest};
use wardnet_common::api::{DeviceMeResponse, DnsCaptureSettingsResponse, SetMyRuleResponse};
use wardnet_common::device::{Device, DeviceType};
use wardnet_common::routing::RoutingTarget;
use wardnetd_data::repository::AccessRequestRepository;

use crate::access_request::{
    AccessRequestService, AccessRequestServiceImpl, ApproverRegistry, PrivateDnsApprover,
};
use crate::auth_context;
use crate::device::DeviceService;
use crate::error::AppError;
use crate::event::EventPublisher;
use crate::private_dns::{PrivateDnsGrant, PrivateDnsService, PrivateDnsStatus};
use wardnet_common::auth::AuthContext;
use wardnet_common::event::WardnetEvent;
use wardnet_test_support::principal;

const DEVICE_ID: &str = "00000000-0000-0000-0000-000000000001";

// --- Mock repository --------------------------------------------------------

#[derive(Default)]
struct MockAccessRequestRepo {
    decide_returns_none: bool,
    /// What `find_by_id` hands back — lets a test stage a `private_dns`
    /// request, or one that was already decided.
    stored_kind: Option<AccessRequestKind>,
    stored_status: Option<AccessRequestStatus>,
    missing: bool,
    resolve_calls: AtomicUsize,
}

#[async_trait]
impl AccessRequestRepository for MockAccessRequestRepo {
    async fn insert(
        &self,
        id: &str,
        device_id: &str,
        kind: AccessRequestKind,
        domain: Option<&str>,
        reason: Option<&str>,
        created_at: &str,
    ) -> anyhow::Result<DeviceAccessRequest> {
        Ok(DeviceAccessRequest {
            id: id.to_owned(),
            device_id: device_id.to_owned(),
            kind,
            domain: domain.map(str::to_owned),
            reason: reason.map(str::to_owned),
            status: AccessRequestStatus::Pending,
            created_at: created_at.to_owned(),
            decided_at: None,
            decided_by: None,
        })
    }

    async fn list_by_device(&self, device_id: &str) -> anyhow::Result<Vec<DeviceAccessRequest>> {
        Ok(vec![sample(device_id, AccessRequestStatus::Pending)])
    }

    async fn list_all(
        &self,
        _status: Option<AccessRequestStatus>,
    ) -> anyhow::Result<Vec<DeviceAccessRequest>> {
        Ok(vec![sample(DEVICE_ID, AccessRequestStatus::Pending)])
    }

    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<DeviceAccessRequest>> {
        if self.missing {
            return Ok(None);
        }
        Ok(Some(DeviceAccessRequest {
            id: id.to_owned(),
            kind: self.stored_kind.unwrap_or(AccessRequestKind::Block),
            status: self.stored_status.unwrap_or(AccessRequestStatus::Pending),
            ..sample(DEVICE_ID, AccessRequestStatus::Pending)
        }))
    }

    async fn update_status(
        &self,
        id: &str,
        status: AccessRequestStatus,
        decided_by: &str,
        decided_at: &str,
    ) -> anyhow::Result<Option<DeviceAccessRequest>> {
        if self.decide_returns_none {
            return Ok(None);
        }
        Ok(Some(DeviceAccessRequest {
            id: id.to_owned(),
            status,
            decided_by: Some(decided_by.to_owned()),
            decided_at: Some(decided_at.to_owned()),
            ..sample(DEVICE_ID, status)
        }))
    }

    async fn resolve_pending(
        &self,
        device_id: &str,
        kind: AccessRequestKind,
        status: AccessRequestStatus,
        decided_by: Option<&str>,
        decided_at: &str,
    ) -> anyhow::Result<Option<DeviceAccessRequest>> {
        self.resolve_calls.fetch_add(1, Ordering::SeqCst);
        Ok(Some(DeviceAccessRequest {
            id: "req-1".to_owned(),
            device_id: device_id.to_owned(),
            kind,
            status,
            decided_by: decided_by.map(str::to_owned),
            decided_at: Some(decided_at.to_owned()),
            ..sample(device_id, status)
        }))
    }
}

fn sample(device_id: &str, status: AccessRequestStatus) -> DeviceAccessRequest {
    DeviceAccessRequest {
        id: "req-1".to_owned(),
        device_id: device_id.to_owned(),
        kind: AccessRequestKind::Block,
        domain: Some("ads.example.com".to_owned()),
        reason: None,
        status,
        created_at: "2026-06-18T00:00:00Z".to_owned(),
        decided_at: None,
        decided_by: None,
    }
}

// --- Mock Private DNS service ----------------------------------------------

#[derive(Default)]
struct MockPrivateDns {
    enabled: bool,
    grants: AtomicUsize,
    /// Simulate a device that was granted from the Remote Access card while
    /// the request sat pending.
    already_granted: AtomicBool,
    /// Simulate the feature being switched off between request and approval.
    grant_fails_disabled: bool,
    /// How many times the device was notified of its grant.
    notifications: AtomicUsize,
    /// Simulate a push that cannot be delivered.
    notify_fails: bool,
}

#[async_trait]
impl PrivateDnsService for MockPrivateDns {
    async fn status(&self) -> Result<PrivateDnsStatus, AppError> {
        Ok(PrivateDnsStatus {
            enabled: self.enabled,
            domain: None,
        })
    }

    async fn is_enabled(&self) -> Result<bool, AppError> {
        Ok(self.enabled)
    }

    async fn set_enabled(&self, _enabled: bool) -> Result<PrivateDnsStatus, AppError> {
        unimplemented!("not used by access-request tests")
    }

    async fn grant_device(&self, device_id: Uuid) -> Result<PrivateDnsGrant, AppError> {
        if self.grant_fails_disabled {
            return Err(AppError::Conflict("Private DNS is disabled".to_owned()));
        }
        if self.already_granted.load(Ordering::SeqCst) {
            return Err(AppError::Conflict(
                "device already has a Private DNS grant".to_owned(),
            ));
        }
        self.grants.fetch_add(1, Ordering::SeqCst);
        Ok(PrivateDnsGrant {
            id: Uuid::new_v4(),
            device_id,
            token: "abcdefghijklmnop".to_owned(),
            created_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    async fn device_grant(&self, device_id: Uuid) -> Result<Option<PrivateDnsGrant>, AppError> {
        if !self.already_granted.load(Ordering::SeqCst) {
            return Ok(None);
        }
        Ok(Some(PrivateDnsGrant {
            id: Uuid::new_v4(),
            device_id,
            token: "abcdefghijklmnop".to_owned(),
            created_at: chrono::Utc::now().to_rfc3339(),
        }))
    }

    async fn notify_device(&self, _device_id: Uuid) -> Result<bool, AppError> {
        if self.notify_fails {
            return Err(AppError::Internal(anyhow::anyhow!("push gateway down")));
        }
        self.notifications.fetch_add(1, Ordering::SeqCst);
        Ok(true)
    }

    async fn revoke_grant(&self, _grant_id: Uuid) -> Result<(), AppError> {
        unimplemented!("not used by access-request tests")
    }

    async fn list_grants(&self) -> Result<Vec<PrivateDnsGrant>, AppError> {
        Ok(vec![])
    }

    async fn resolve_token(&self, _token: &str) -> Result<Option<PrivateDnsGrant>, AppError> {
        Ok(None)
    }

    async fn reconcile(&self) -> Result<(), AppError> {
        Ok(())
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
        manufacturer_source: None,
        is_randomized: false,
        device_type: DeviceType::Unknown,
        first_seen: chrono::Utc::now(),
        last_seen: chrono::Utc::now(),
        last_ip: "192.168.1.100".to_owned(),
        admin_locked: false,
        zone_id: "00000000-0000-0000-0000-000000000201".parse().unwrap(),
        owner_user_id: None,
        dns_capture_enabled: false,
        dns_capture_cap_count: 1000,
        dns_capture_cap_days: 7,
        connection_mode: wardnet_common::device::DeviceConnectionMode::Lan,
        managed: false,
    }
}

#[async_trait]
impl DeviceService for MockDeviceService {
    async fn clear_rule(&self, _device_id: &str) -> Result<(), crate::error::AppError> {
        Ok(())
    }

    async fn mark_managed(&self, _device_id: &str) -> Result<(), crate::error::AppError> {
        Ok(())
    }

    async fn clear_managed(&self, _device_id: &str) -> Result<(), crate::error::AppError> {
        Ok(())
    }

    async fn set_device_owner(
        &self,
        _device_id: &str,
        _owner_user_id: Option<uuid::Uuid>,
    ) -> Result<(), AppError> {
        unimplemented!()
    }

    async fn get_device(
        &self,
        _device_id: &str,
    ) -> Result<Option<wardnet_common::device::Device>, AppError> {
        unimplemented!("not used by access-request tests")
    }
    async fn clear_remote_connection_mode(&self, _device_id: &str) -> Result<(), AppError> {
        unimplemented!("not used by access-request tests")
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

/// Test-scoped builder. Defaults match the common case: device found, the
/// decision persists, Private DNS enabled, and the Private-DNS approver
/// registered.
struct Fixture {
    found: bool,
    decide_none: bool,
    repo: MockAccessRequestRepo,
    private_dns: MockPrivateDns,
    register_approver: bool,
}

impl Default for Fixture {
    fn default() -> Self {
        Self {
            found: true,
            decide_none: false,
            repo: MockAccessRequestRepo::default(),
            private_dns: MockPrivateDns {
                enabled: true,
                ..MockPrivateDns::default()
            },
            register_approver: true,
        }
    }
}

impl Fixture {
    fn build(
        self,
    ) -> (
        AccessRequestServiceImpl,
        Arc<CapturingEventPublisher>,
        Arc<MockPrivateDns>,
        Arc<MockAccessRequestRepo>,
    ) {
        let events = Arc::new(CapturingEventPublisher::default());
        let private_dns = Arc::new(self.private_dns);
        let repo = Arc::new(MockAccessRequestRepo {
            decide_returns_none: self.decide_none,
            ..self.repo
        });
        let approvers = if self.register_approver {
            ApproverRegistry::new(vec![Arc::new(PrivateDnsApprover::new(private_dns.clone()))])
        } else {
            ApproverRegistry::default()
        };
        let service = AccessRequestServiceImpl::new(
            repo.clone(),
            Arc::new(MockDeviceService { found: self.found }),
            private_dns.clone(),
            approvers,
            events.clone(),
        );
        (service, events, private_dns, repo)
    }
}

fn build(found: bool, decide_none: bool) -> AccessRequestServiceImpl {
    Fixture {
        found,
        decide_none,
        ..Fixture::default()
    }
    .build()
    .0
}

fn admin_ctx() -> AuthContext {
    principal::admin_context(Uuid::parse_str("00000000-0000-0000-0000-000000000099").unwrap())
}

// --- Creating ---------------------------------------------------------------

#[tokio::test]
async fn create_normalizes_domain_and_resolves_device() {
    let svc = build(true, false);
    let req = svc
        .create_for_ip(
            "1.2.3.4",
            AccessRequestKind::Block,
            Some("  ADS.Example.COM ".into()),
            Some("  ".into()),
        )
        .await
        .unwrap();
    assert_eq!(req.domain.as_deref(), Some("ads.example.com"));
    assert_eq!(req.device_id, DEVICE_ID);
    // Whitespace-only reason is dropped.
    assert_eq!(req.reason, None);
}

#[tokio::test]
async fn create_publishes_access_request_created_event() {
    let (svc, events, ..) = Fixture::default().build();
    let req = svc
        .create_for_ip(
            "1.2.3.4",
            AccessRequestKind::Allow,
            Some("blocked.example".into()),
            None,
        )
        .await
        .unwrap();

    let published = events.events.lock().unwrap();
    assert_eq!(published.len(), 1);
    match &published[0] {
        WardnetEvent::AccessRequestCreated {
            request_id,
            device_id,
            kind,
            domain,
            ..
        } => {
            assert_eq!(request_id, &req.id);
            assert_eq!(device_id, DEVICE_ID);
            assert_eq!(*kind, AccessRequestKind::Allow);
            assert_eq!(domain.as_deref(), Some("blocked.example"));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[tokio::test]
async fn create_rejects_empty_domain() {
    let svc = build(true, false);
    let err = svc
        .create_for_ip(
            "1.2.3.4",
            AccessRequestKind::Block,
            Some("   ".into()),
            None,
        )
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::BadRequest(_)));
}

#[tokio::test]
async fn create_unknown_ip_returns_not_found() {
    let svc = build(false, false);
    let err = svc
        .create_for_ip(
            "9.9.9.9",
            AccessRequestKind::Allow,
            Some("x.com".into()),
            None,
        )
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::NotFound(_)));
}

#[tokio::test]
async fn a_private_dns_request_takes_no_domain() {
    let (svc, ..) = Fixture::default().build();
    let req = svc
        .create_for_ip("1.2.3.4", AccessRequestKind::PrivateDns, None, None)
        .await
        .unwrap();
    assert!(req.domain.is_none());

    let err = svc
        .create_for_ip(
            "1.2.3.4",
            AccessRequestKind::PrivateDns,
            Some("a.com".into()),
            None,
        )
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::BadRequest(_)));
}

/// The server-side half of "the Request button only appears once the feature is
/// on": a request the admin could never approve must not be creatable.
#[tokio::test]
async fn a_private_dns_request_is_refused_while_the_feature_is_disabled() {
    let (svc, events, ..) = Fixture {
        private_dns: MockPrivateDns {
            enabled: false,
            ..MockPrivateDns::default()
        },
        ..Fixture::default()
    }
    .build();

    let err = svc
        .create_for_ip("1.2.3.4", AccessRequestKind::PrivateDns, None, None)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)));
    assert!(
        events.events.lock().unwrap().is_empty(),
        "a refused request must not notify admins"
    );
}

/// Rule requests are unaffected by the Private DNS gate.
#[tokio::test]
async fn a_rule_request_is_allowed_while_private_dns_is_disabled() {
    let (svc, ..) = Fixture {
        private_dns: MockPrivateDns {
            enabled: false,
            ..MockPrivateDns::default()
        },
        ..Fixture::default()
    }
    .build();

    svc.create_for_ip(
        "1.2.3.4",
        AccessRequestKind::Allow,
        Some("a.com".into()),
        None,
    )
    .await
    .expect("private DNS being off must not block a rule request");
}

// --- Listing ----------------------------------------------------------------

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

// --- Deciding ---------------------------------------------------------------

#[tokio::test]
async fn decide_rejects_pending_status() {
    let svc = build(true, false);
    let err = auth_context::with_context(admin_ctx(), async {
        svc.decide("req-1", AccessRequestStatus::Pending, None)
            .await
    })
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::BadRequest(_)));
}

#[tokio::test]
async fn decide_unknown_returns_not_found() {
    let (svc, ..) = Fixture {
        repo: MockAccessRequestRepo {
            missing: true,
            ..MockAccessRequestRepo::default()
        },
        ..Fixture::default()
    }
    .build();
    let err = auth_context::with_context(admin_ctx(), async {
        svc.decide("nope", AccessRequestStatus::Approved, None)
            .await
    })
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::NotFound(_)));
}

#[tokio::test]
async fn decide_ok_records_admin_and_status() {
    let svc = build(true, false);
    let req = auth_context::with_context(admin_ctx(), async {
        svc.decide("req-1", AccessRequestStatus::Approved, None)
            .await
    })
    .await
    .unwrap();
    assert_eq!(req.status, AccessRequestStatus::Approved);
    assert!(req.decided_by.is_some());
    assert!(req.decided_at.is_some());
}

/// Re-deciding would rewrite the audit trail and, on approval, run the side
/// effect twice.
#[tokio::test]
async fn decide_refuses_an_already_decided_request() {
    let (svc, ..) = Fixture {
        repo: MockAccessRequestRepo {
            stored_status: Some(AccessRequestStatus::Approved),
            ..MockAccessRequestRepo::default()
        },
        ..Fixture::default()
    }
    .build();
    let err = auth_context::with_context(admin_ctx(), async {
        svc.decide("req-1", AccessRequestStatus::Rejected, None)
            .await
    })
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)));
}

/// `AuthContext::system()` is a `User` with the admin role, so it clears
/// `require_admin` — but a decision needs a human. Letting it through would
/// stamp the nil UUID into `decided_by`, an id with no `users` row.
#[tokio::test]
async fn decide_refuses_the_system_context() {
    let (svc, ..) = Fixture::default().build();
    let err = auth_context::with_context(AuthContext::system(), async {
        svc.decide("req-1", AccessRequestStatus::Approved, None)
            .await
    })
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::Forbidden(_)));
}

// --- The approver registry --------------------------------------------------

#[tokio::test]
async fn approving_a_private_dns_request_mints_the_grant() {
    let (svc, _events, private_dns, _repo) = Fixture {
        repo: MockAccessRequestRepo {
            stored_kind: Some(AccessRequestKind::PrivateDns),
            ..MockAccessRequestRepo::default()
        },
        ..Fixture::default()
    }
    .build();

    auth_context::with_context(admin_ctx(), async {
        svc.decide("req-1", AccessRequestStatus::Approved, None)
            .await
    })
    .await
    .unwrap();

    assert_eq!(private_dns.grants.load(Ordering::SeqCst), 1);
}

/// A kind with no registered approver stays record-only — today's `allow` /
/// `block` behaviour, preserved with no special-casing.
#[tokio::test]
async fn approving_a_rule_request_mints_nothing() {
    let (svc, _events, private_dns, _repo) = Fixture {
        repo: MockAccessRequestRepo {
            stored_kind: Some(AccessRequestKind::Allow),
            ..MockAccessRequestRepo::default()
        },
        ..Fixture::default()
    }
    .build();

    let req = auth_context::with_context(admin_ctx(), async {
        svc.decide("req-1", AccessRequestStatus::Approved, None)
            .await
    })
    .await
    .unwrap();

    assert_eq!(req.status, AccessRequestStatus::Approved);
    assert_eq!(private_dns.grants.load(Ordering::SeqCst), 0);
}

/// Rejecting must never run the approver.
#[tokio::test]
async fn rejecting_a_private_dns_request_mints_nothing() {
    let (svc, _events, private_dns, _repo) = Fixture {
        repo: MockAccessRequestRepo {
            stored_kind: Some(AccessRequestKind::PrivateDns),
            ..MockAccessRequestRepo::default()
        },
        ..Fixture::default()
    }
    .build();

    auth_context::with_context(admin_ctx(), async {
        svc.decide("req-1", AccessRequestStatus::Rejected, None)
            .await
    })
    .await
    .unwrap();

    assert_eq!(private_dns.grants.load(Ordering::SeqCst), 0);
}

/// The admin may have granted from the Remote Access card while the request sat
/// pending. The end state the admin asked for already holds, so approving is a
/// success — answering 409 would make them dismiss a request whose outcome
/// already happened.
#[tokio::test]
async fn approving_an_already_granted_device_succeeds() {
    let (svc, _events, private_dns, _repo) = Fixture {
        repo: MockAccessRequestRepo {
            stored_kind: Some(AccessRequestKind::PrivateDns),
            ..MockAccessRequestRepo::default()
        },
        ..Fixture::default()
    }
    .build();
    private_dns.already_granted.store(true, Ordering::SeqCst);

    let req = auth_context::with_context(admin_ctx(), async {
        svc.decide("req-1", AccessRequestStatus::Approved, None)
            .await
    })
    .await
    .unwrap();
    assert_eq!(req.status, AccessRequestStatus::Approved);
}

/// If the approver fails, the request must stay pending rather than read
/// "approved" against a grant that was never minted.
#[tokio::test]
async fn a_failed_approval_does_not_record_a_decision() {
    let (svc, _events, _private_dns, repo) = Fixture {
        repo: MockAccessRequestRepo {
            stored_kind: Some(AccessRequestKind::PrivateDns),
            ..MockAccessRequestRepo::default()
        },
        private_dns: MockPrivateDns {
            enabled: true,
            grant_fails_disabled: true,
            ..MockPrivateDns::default()
        },
        ..Fixture::default()
    }
    .build();

    let err = auth_context::with_context(admin_ctx(), async {
        svc.decide("req-1", AccessRequestStatus::Approved, None)
            .await
    })
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)));

    // The stored request is untouched — `find_by_id` still reports pending.
    let stored = repo.find_by_id("req-1").await.unwrap().unwrap();
    assert_eq!(stored.status, AccessRequestStatus::Pending);
}

// --- Bus-driven reconciliation ---------------------------------------------

/// Reachable through `AppState::access_request_service()`, so it is gated like
/// every other write — a future handler must not be able to wire an
/// unauthenticated write to the decision audit trail.
#[tokio::test]
async fn resolve_pending_refuses_an_unauthenticated_caller() {
    let (svc, ..) = Fixture::default().build();
    let err = auth_context::with_context(AuthContext::Anonymous, async {
        svc.resolve_pending(
            Uuid::parse_str(DEVICE_ID).unwrap(),
            AccessRequestKind::PrivateDns,
            AccessRequestStatus::Approved,
            Some("admin-1".to_owned()),
        )
        .await
    })
    .await
    .unwrap_err();
    assert!(matches!(err, AppError::Forbidden(_)));
}

/// `AccessRequestListener` has no request context of its own, so it calls this
/// under the system context — the same wrapper the other bus listeners use.
#[tokio::test]
async fn resolve_pending_works_under_the_system_context() {
    let (svc, ..) = Fixture::default().build();
    let out = auth_context::with_context(AuthContext::system(), async {
        svc.resolve_pending(
            Uuid::parse_str(DEVICE_ID).unwrap(),
            AccessRequestKind::PrivateDns,
            AccessRequestStatus::Approved,
            Some("admin-1".to_owned()),
        )
        .await
    })
    .await
    .unwrap()
    .expect("a pending request should resolve");
    assert_eq!(out.status, AccessRequestStatus::Approved);
    assert_eq!(out.decided_by.as_deref(), Some("admin-1"));
}

/// The one phantom-pending row the listener cannot clean up: a request filed
/// *after* the grant has already missed `PrivateDnsGrantCreated`, and no future
/// event will ever name this device. Reachable whenever the PWA's cached
/// `me.granted` is stale.
#[tokio::test]
async fn a_private_dns_request_is_refused_when_the_device_is_already_granted() {
    let (svc, events, private_dns, _repo) = Fixture::default().build();
    private_dns.already_granted.store(true, Ordering::SeqCst);

    let err = svc
        .create_for_ip("1.2.3.4", AccessRequestKind::PrivateDns, None, None)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)));
    assert!(
        events.events.lock().unwrap().is_empty(),
        "a refused request must not notify admins"
    );
}

/// Approval is the moment the member is waiting for, so it pushes — this is
/// what makes "no new client machinery" true rather than aspirational.
#[tokio::test]
async fn approving_a_private_dns_request_notifies_the_device() {
    let (svc, _events, private_dns, _repo) = Fixture {
        repo: MockAccessRequestRepo {
            stored_kind: Some(AccessRequestKind::PrivateDns),
            ..MockAccessRequestRepo::default()
        },
        ..Fixture::default()
    }
    .build();

    auth_context::with_context(admin_ctx(), async {
        svc.decide("req-1", AccessRequestStatus::Approved, None)
            .await
    })
    .await
    .unwrap();

    assert_eq!(private_dns.notifications.load(Ordering::SeqCst), 1);
}

/// The grant is already persisted and working, so a delivery problem must not
/// fail the approval and leave the request pending against a live grant.
#[tokio::test]
async fn a_failed_notification_does_not_fail_the_approval() {
    let (svc, _events, private_dns, _repo) = Fixture {
        repo: MockAccessRequestRepo {
            stored_kind: Some(AccessRequestKind::PrivateDns),
            ..MockAccessRequestRepo::default()
        },
        private_dns: MockPrivateDns {
            enabled: true,
            notify_fails: true,
            ..MockPrivateDns::default()
        },
        ..Fixture::default()
    }
    .build();

    let req = auth_context::with_context(admin_ctx(), async {
        svc.decide("req-1", AccessRequestStatus::Approved, None)
            .await
    })
    .await
    .expect("a push failure must not fail the approval");
    assert_eq!(req.status, AccessRequestStatus::Approved);
    assert_eq!(private_dns.grants.load(Ordering::SeqCst), 1);
}

/// The listener can resolve the request between `decide`'s read and its write.
/// The guarded update then matches nothing, and the admin should see the
/// decision that actually landed rather than a 404.
#[tokio::test]
async fn decide_returns_the_listener_decision_when_it_loses_the_race() {
    let (svc, ..) = Fixture {
        repo: MockAccessRequestRepo {
            stored_kind: Some(AccessRequestKind::PrivateDns),
            // The guarded UPDATE matched nothing — someone else got there.
            decide_returns_none: true,
            ..MockAccessRequestRepo::default()
        },
        ..Fixture::default()
    }
    .build();

    let req = auth_context::with_context(admin_ctx(), async {
        svc.decide("req-1", AccessRequestStatus::Approved, None)
            .await
    })
    .await
    .expect("losing the race is not an error");
    assert_eq!(req.id, "req-1");
}
