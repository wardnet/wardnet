use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::broadcast;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;
use wardnet_common::device::{Device, DeviceType};
use wardnet_common::event::WardnetEvent;
use wardnet_common::network_zone::{AllowedTargetKind, NetworkZone, ZoneProvenance, ZoneStance};
use wardnet_common::routing::{RoutingRule, RoutingTarget, RuleCreator};

use crate::auth_context;
use crate::event::EventPublisher;
use crate::{DeviceService, DeviceServiceImpl};
use wardnetd_data::repository::device::DeviceRow;
use wardnetd_data::repository::dns_events::{DnsCaptureStats, DnsEventRow, DnsEventsRepository};
use wardnetd_data::repository::{DeviceRepository, NetworkZoneRepository, SystemConfigRepository};

// -- Mock repository ------------------------------------------------------

struct MockDeviceRepo {
    device: Option<Device>,
    rule: Option<RoutingRule>,
    /// Rules returned by the batched `find_all_rules` lookup.
    all_rules: Vec<RoutingRule>,
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
    async fn clear_last_ip(&self, _id: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_last_seen_and_ip(
        &self,
        _id: &str,
        _ip: &str,
        _last_seen: &str,
        _mode: wardnet_common::device::DeviceConnectionMode,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_connection_mode(
        &self,
        _id: &str,
        _mode: wardnet_common::device::DeviceConnectionMode,
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
    async fn find_all_rules(&self) -> anyhow::Result<Vec<RoutingRule>> {
        Ok(self.all_rules.clone())
    }
    async fn upsert_user_rule(&self, _id: &str, _json: &str, _now: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_admin_locked(&self, _id: &str, _locked: bool) -> anyhow::Result<()> {
        Ok(())
    }
    async fn assign_zone(&self, _device_id: &str, _zone_id: &str) -> anyhow::Result<bool> {
        Ok(true)
    }
    async fn find_devices_for_tunnel(
        &self,
        _tid: &str,
    ) -> anyhow::Result<Vec<wardnet_common::device::Device>> {
        Ok(vec![])
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
    async fn update_dns_capture_settings(
        &self,
        _id: &str,
        _enabled: Option<bool>,
        _cap_count: Option<i64>,
        _cap_days: Option<i64>,
    ) -> anyhow::Result<bool> {
        Ok(true)
    }
    async fn find_all_capture_enabled_ids(&self) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
}

struct MockDnsEventsRepo;

#[async_trait]
impl DnsEventsRepository for MockDnsEventsRepo {
    async fn insert(
        &self,
        _device_id: &str,
        _domain: &str,
        _status: &str,
        _captured_at: &str,
    ) -> anyhow::Result<i64> {
        Ok(1)
    }
    async fn stats_for_device(&self, _device_id: &str) -> anyhow::Result<DnsCaptureStats> {
        Ok(DnsCaptureStats {
            row_count: 0,
            size_bytes: 0,
        })
    }
    async fn prune_for_device(
        &self,
        _device_id: &str,
        _cap_count: i64,
        _cap_days: i64,
    ) -> anyhow::Result<u64> {
        Ok(0)
    }
    async fn delete_all_for_device(&self, _device_id: &str) -> anyhow::Result<u64> {
        Ok(0)
    }
    async fn find_device_ids_with_data(&self) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
    async fn fetch_pending(
        &self,
        _device_id: &str,
        _after_id: i64,
        _limit: i64,
    ) -> anyhow::Result<Vec<wardnetd_data::repository::DnsEventRow>> {
        Ok(vec![])
    }
    async fn mark_synced_up_to(&self, _device_id: &str, _up_to_id: i64) -> anyhow::Result<u64> {
        Ok(0)
    }
    async fn delete_up_to(&self, _device_id: &str, _up_to_id: i64) -> anyhow::Result<u64> {
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

// -- Mock network-zone repository -----------------------------------------

/// The Trusted zone UUID seeded by the schema.
const TRUSTED_ZONE_ID: &str = "00000000-0000-0000-0000-000000000201";

/// A permissive zone that allows both `Direct` and `Tunnel` targets, so
/// existing routing tests never hit the zone `Conflict` guard.
fn permissive_zone() -> NetworkZone {
    NetworkZone {
        id: Uuid::parse_str(TRUSTED_ZONE_ID).unwrap(),
        name: "Trusted".to_owned(),
        provenance: ZoneProvenance::System,
        isolation_stance: ZoneStance::SharedSubnet,
        allowed_targets: vec![AllowedTargetKind::Direct, AllowedTargetKind::Tunnel],
        member_isolation: false,
        subnet: None,
        admin_ui_reachable: false,
        is_default: false,
        is_default_for_new: false,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

/// Permissive zone repo: every lookup resolves to [`permissive_zone`].
struct MockNetworkZoneRepo;

#[async_trait]
impl NetworkZoneRepository for MockNetworkZoneRepo {
    async fn find_all(&self) -> anyhow::Result<Vec<NetworkZone>> {
        Ok(vec![permissive_zone()])
    }
    async fn find_by_id(&self, _id: &str) -> anyhow::Result<Option<NetworkZone>> {
        Ok(Some(permissive_zone()))
    }
    async fn find_default(&self) -> anyhow::Result<NetworkZone> {
        Ok(permissive_zone())
    }
    async fn find_default_for_new(&self) -> anyhow::Result<NetworkZone> {
        Ok(permissive_zone())
    }
    async fn insert(&self, _zone: &NetworkZone) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update(&self, _zone: &NetworkZone) -> anyhow::Result<()> {
        Ok(())
    }
    async fn delete(&self, _id: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn set_default(&self, _id: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn set_default_for_new(&self, _id: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn count_members(&self, _zone_id: &str) -> anyhow::Result<i64> {
        Ok(0)
    }
    async fn member_counts(&self) -> anyhow::Result<std::collections::HashMap<String, i64>> {
        Ok(std::collections::HashMap::new())
    }
}

// -- Mock system-config repository ----------------------------------------

/// Minimal system-config repo whose `get_default_policy` resolves to
/// `"direct"`, matching the routing behaviour these tests assume.
struct MockSystemConfigRepo;

#[async_trait]
impl SystemConfigRepository for MockSystemConfigRepo {
    async fn get(&self, _key: &str) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
    async fn set(&self, _key: &str, _value: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn delete(&self, _key: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn device_count(&self) -> anyhow::Result<i64> {
        Ok(0)
    }
    async fn tunnel_count(&self) -> anyhow::Result<i64> {
        Ok(0)
    }
    async fn db_size_bytes(&self) -> anyhow::Result<u64> {
        Ok(0)
    }
    async fn get_default_policy(&self) -> anyhow::Result<Option<String>> {
        Ok(Some("direct".to_owned()))
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
        zone_id: TRUSTED_ZONE_ID.parse().unwrap(),
        dns_capture_enabled: false,
        dns_capture_cap_count: 1000,
        dns_capture_cap_days: 7,
        connection_mode: wardnet_common::device::DeviceConnectionMode::Lan,
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
            all_rules: vec![],
        }),
        Arc::new(MockDnsEventsRepo),
        Arc::new(MockNetworkZoneRepo),
        Arc::new(MockSystemConfigRepo),
        Arc::new(MockEventPublisher),
    )
}

fn make_svc_no_device() -> DeviceServiceImpl {
    DeviceServiceImpl::new(
        Arc::new(MockDeviceRepo {
            device: None,
            rule: None,
            all_rules: vec![],
        }),
        Arc::new(MockDnsEventsRepo),
        Arc::new(MockNetworkZoneRepo),
        Arc::new(MockSystemConfigRepo),
        Arc::new(MockEventPublisher),
    )
}

fn make_svc_with_rules(all_rules: Vec<RoutingRule>) -> DeviceServiceImpl {
    DeviceServiceImpl::new(
        Arc::new(MockDeviceRepo {
            device: None,
            rule: None,
            all_rules,
        }),
        Arc::new(MockDnsEventsRepo),
        Arc::new(MockNetworkZoneRepo),
        Arc::new(MockSystemConfigRepo),
        Arc::new(MockEventPublisher),
    )
}

fn rule_for(device_id: &str, target: RoutingTarget) -> RoutingRule {
    RoutingRule {
        device_id: Uuid::parse_str(device_id).unwrap(),
        target,
        created_by: RuleCreator::User,
    }
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

// -- Capturing event publisher --------------------------------------------

/// Event publisher that records every published event for later inspection.
struct CapturingEventPublisher {
    events: std::sync::Mutex<Vec<WardnetEvent>>,
}

impl CapturingEventPublisher {
    fn new() -> Self {
        Self {
            events: std::sync::Mutex::new(vec![]),
        }
    }

    fn take_events(&self) -> Vec<WardnetEvent> {
        self.events.lock().unwrap().drain(..).collect()
    }
}

impl EventPublisher for CapturingEventPublisher {
    fn publish(&self, event: WardnetEvent) {
        self.events.lock().unwrap().push(event);
    }
    fn subscribe(&self) -> broadcast::Receiver<WardnetEvent> {
        let (tx, rx) = broadcast::channel(1);
        drop(tx);
        rx
    }
}

// -- Tests: current_rules ------------------------------------------------

#[tokio::test]
async fn current_rules_maps_each_device_to_its_target() {
    const DEV1: &str = "00000000-0000-0000-0000-000000000001";
    const DEV2: &str = "00000000-0000-0000-0000-000000000002";
    const DEV3: &str = "00000000-0000-0000-0000-000000000003";
    let tunnel_id = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();

    let svc = make_svc_with_rules(vec![
        rule_for(DEV1, RoutingTarget::Tunnel { tunnel_id }),
        rule_for(DEV2, RoutingTarget::Direct),
        rule_for(DEV3, RoutingTarget::Default),
    ]);

    let map = auth_context::with_context(admin_ctx(), svc.current_rules())
        .await
        .unwrap();

    assert_eq!(map.len(), 3);
    assert_eq!(
        map.get(&Uuid::parse_str(DEV1).unwrap()),
        Some(&RoutingTarget::Tunnel { tunnel_id })
    );
    assert_eq!(
        map.get(&Uuid::parse_str(DEV2).unwrap()),
        Some(&RoutingTarget::Direct)
    );
    assert_eq!(
        map.get(&Uuid::parse_str(DEV3).unwrap()),
        Some(&RoutingTarget::Default)
    );
}

#[tokio::test]
async fn current_rules_empty_when_no_rules() {
    let svc = make_svc_with_rules(vec![]);

    let map = auth_context::with_context(admin_ctx(), svc.current_rules())
        .await
        .unwrap();

    assert!(map.is_empty());
}

#[tokio::test]
async fn current_rules_anonymous_forbidden() {
    let svc = make_svc_with_rules(vec![rule_for(
        "00000000-0000-0000-0000-000000000001",
        RoutingTarget::Direct,
    )]);

    let result = auth_context::with_context(AuthContext::Anonymous, svc.current_rules()).await;

    assert!(result.is_err());
}

// -- Tests: get_dns_capture_settings -------------------------------------

#[tokio::test]
async fn get_dns_capture_settings_returns_404_for_unknown() {
    let svc = make_svc_no_device();
    let unknown_id = "00000000-0000-0000-0000-000000000099";

    let result = auth_context::with_context(admin_ctx(), async {
        svc.get_dns_capture_settings(unknown_id).await
    })
    .await;

    assert!(matches!(result, Err(crate::error::AppError::NotFound(_))));
}

#[tokio::test]
async fn get_dns_capture_settings_returns_defaults() {
    let svc = make_svc(false, None);
    let device_id = "00000000-0000-0000-0000-000000000001";

    let resp = auth_context::with_context(admin_ctx(), async {
        svc.get_dns_capture_settings(device_id).await
    })
    .await
    .unwrap();

    assert!(!resp.enabled);
    assert_eq!(resp.cap_count, 1000);
    assert_eq!(resp.cap_days, 7);
    assert_eq!(resp.row_count, 0);
    assert_eq!(resp.size_bytes, 0);
}

// -- Tests: update_dns_capture_settings ----------------------------------

#[tokio::test]
async fn update_dns_capture_settings_publishes_event() {
    let publisher = Arc::new(CapturingEventPublisher::new());
    let svc = DeviceServiceImpl::new(
        Arc::new(MockDeviceRepo {
            device: Some(sample_device(false)),
            rule: None,
            all_rules: vec![],
        }),
        Arc::new(MockDnsEventsRepo),
        Arc::new(MockNetworkZoneRepo),
        Arc::new(MockSystemConfigRepo),
        Arc::clone(&publisher) as Arc<dyn crate::event::EventPublisher>,
    );
    let device_id = "00000000-0000-0000-0000-000000000001";

    auth_context::with_context(admin_ctx(), async {
        svc.update_dns_capture_settings(device_id, Some(true), None, None)
            .await
    })
    .await
    .unwrap();

    let events = publisher.take_events();
    assert_eq!(events.len(), 1);
    match &events[0] {
        WardnetEvent::DeviceCaptureSettingsChanged {
            device_id: id,
            enabled,
            ..
        } => {
            assert_eq!(
                *id,
                Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap()
            );
            assert!(*enabled);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn update_dns_capture_settings_returns_404_for_unknown() {
    // Repo returns false from update_dns_capture_settings when the row is not found.
    struct NotFoundDeviceRepo;

    #[async_trait]
    impl DeviceRepository for NotFoundDeviceRepo {
        async fn find_by_ip(&self, _ip: &str) -> anyhow::Result<Option<Device>> {
            Ok(None)
        }
        async fn find_by_id(&self, _id: &str) -> anyhow::Result<Option<Device>> {
            Ok(None)
        }
        async fn find_by_mac(&self, _mac: &str) -> anyhow::Result<Option<Device>> {
            Ok(None)
        }
        async fn find_all(&self) -> anyhow::Result<Vec<Device>> {
            Ok(vec![])
        }
        async fn insert(&self, _device: &DeviceRow) -> anyhow::Result<()> {
            Ok(())
        }
        async fn clear_last_ip(&self, _id: &str) -> anyhow::Result<()> {
            Ok(())
        }
        async fn update_last_seen_and_ip(
            &self,
            _id: &str,
            _ip: &str,
            _last_seen: &str,
            _mode: wardnet_common::device::DeviceConnectionMode,
        ) -> anyhow::Result<()> {
            Ok(())
        }
        async fn update_connection_mode(
            &self,
            _id: &str,
            _mode: wardnet_common::device::DeviceConnectionMode,
        ) -> anyhow::Result<()> {
            Ok(())
        }
        async fn update_last_seen_batch(
            &self,
            _updates: &[(String, String)],
        ) -> anyhow::Result<()> {
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
            Ok(None)
        }
        async fn find_all_rules(&self) -> anyhow::Result<Vec<RoutingRule>> {
            Ok(vec![])
        }
        async fn upsert_user_rule(&self, _id: &str, _json: &str, _now: &str) -> anyhow::Result<()> {
            Ok(())
        }
        async fn update_admin_locked(&self, _id: &str, _locked: bool) -> anyhow::Result<()> {
            Ok(())
        }
        async fn assign_zone(&self, _device_id: &str, _zone_id: &str) -> anyhow::Result<bool> {
            Ok(true)
        }
        async fn find_devices_for_tunnel(&self, _tid: &str) -> anyhow::Result<Vec<Device>> {
            Ok(vec![])
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
        async fn update_dns_capture_settings(
            &self,
            _id: &str,
            _enabled: Option<bool>,
            _cap_count: Option<i64>,
            _cap_days: Option<i64>,
        ) -> anyhow::Result<bool> {
            Ok(false) // signals "not found"
        }
        async fn find_all_capture_enabled_ids(&self) -> anyhow::Result<Vec<String>> {
            Ok(vec![])
        }
    }

    let svc = DeviceServiceImpl::new(
        Arc::new(NotFoundDeviceRepo),
        Arc::new(MockDnsEventsRepo),
        Arc::new(MockNetworkZoneRepo),
        Arc::new(MockSystemConfigRepo),
        Arc::new(MockEventPublisher),
    );
    let unknown_id = "00000000-0000-0000-0000-000000000099";

    let result = auth_context::with_context(admin_ctx(), async {
        svc.update_dns_capture_settings(unknown_id, Some(true), None, None)
            .await
    })
    .await;

    assert!(matches!(result, Err(crate::error::AppError::NotFound(_))));
}

#[tokio::test]
async fn update_dns_capture_settings_with_enabled_none_reads_db_value() {
    // When `enabled = None`, the service re-reads the device from DB to
    // resolve the actual enabled state before publishing the event.
    let publisher = Arc::new(CapturingEventPublisher::new());
    let svc = DeviceServiceImpl::new(
        Arc::new(MockDeviceRepo {
            // sample_device sets dns_capture_enabled = false
            device: Some(sample_device(false)),
            rule: None,
            all_rules: vec![],
        }),
        Arc::new(MockDnsEventsRepo),
        Arc::new(MockNetworkZoneRepo),
        Arc::new(MockSystemConfigRepo),
        Arc::clone(&publisher) as Arc<dyn crate::event::EventPublisher>,
    );
    let device_id = "00000000-0000-0000-0000-000000000001";

    auth_context::with_context(admin_ctx(), async {
        svc.update_dns_capture_settings(device_id, None, Some(500), Some(3))
            .await
    })
    .await
    .unwrap();

    let events = publisher.take_events();
    assert_eq!(events.len(), 1);
    match &events[0] {
        WardnetEvent::DeviceCaptureSettingsChanged { enabled, .. } => {
            // The mock device has dns_capture_enabled = false, so the
            // DB-resolved value should also be false.
            assert!(!(*enabled), "expected enabled=false from DB read");
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// fetch_pending_dns_events / ack_dns_events
// ---------------------------------------------------------------------------

/// Mock DNS events repo that returns a fixed list of rows from `fetch_pending`.
struct RowsDnsEventsRepo {
    rows: Vec<DnsEventRow>,
}

#[async_trait]
impl DnsEventsRepository for RowsDnsEventsRepo {
    async fn insert(
        &self,
        _device_id: &str,
        _domain: &str,
        _status: &str,
        _captured_at: &str,
    ) -> anyhow::Result<i64> {
        Ok(1)
    }
    async fn stats_for_device(&self, _device_id: &str) -> anyhow::Result<DnsCaptureStats> {
        Ok(DnsCaptureStats {
            row_count: 0,
            size_bytes: 0,
        })
    }
    async fn prune_for_device(
        &self,
        _device_id: &str,
        _cap_count: i64,
        _cap_days: i64,
    ) -> anyhow::Result<u64> {
        Ok(0)
    }
    async fn delete_all_for_device(&self, _device_id: &str) -> anyhow::Result<u64> {
        Ok(0)
    }
    async fn find_device_ids_with_data(&self) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
    async fn fetch_pending(
        &self,
        _device_id: &str,
        _after_id: i64,
        _limit: i64,
    ) -> anyhow::Result<Vec<DnsEventRow>> {
        Ok(self.rows.clone())
    }
    async fn mark_synced_up_to(&self, _device_id: &str, _up_to_id: i64) -> anyhow::Result<u64> {
        Ok(0)
    }
    async fn delete_up_to(&self, _device_id: &str, _up_to_id: i64) -> anyhow::Result<u64> {
        Ok(1)
    }
}

#[tokio::test]
async fn fetch_pending_maps_rows_to_items() {
    let rows = vec![
        DnsEventRow {
            id: 1,
            domain: "ads.tracker.io".to_owned(),
            status: "blocked".to_owned(),
            captured_at: "2026-06-12T00:00:00Z".to_owned(),
        },
        DnsEventRow {
            id: 2,
            domain: "example.com".to_owned(),
            status: "allowed".to_owned(),
            captured_at: "2026-06-12T00:01:00Z".to_owned(),
        },
    ];
    let svc = DeviceServiceImpl::new(
        Arc::new(MockDeviceRepo {
            device: Some(sample_device(false)),
            rule: None,
            all_rules: vec![],
        }),
        Arc::new(RowsDnsEventsRepo { rows }),
        Arc::new(MockNetworkZoneRepo),
        Arc::new(MockSystemConfigRepo),
        Arc::new(MockEventPublisher),
    );

    crate::auth_context::with_context(
        wardnet_common::auth::AuthContext::Admin {
            admin_id: Uuid::nil(),
        },
        async {
            let items = svc
                .fetch_pending_dns_events("00000000-0000-0000-0000-000000000001", 0, 100)
                .await
                .unwrap();
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].id, 1);
            assert_eq!(items[0].domain, "ads.tracker.io");
            assert_eq!(items[1].id, 2);
            assert_eq!(items[1].status, "allowed");
        },
    )
    .await;
}

#[tokio::test]
async fn ack_dns_events_delegates_to_repo() {
    let svc = DeviceServiceImpl::new(
        Arc::new(MockDeviceRepo {
            device: Some(sample_device(false)),
            rule: None,
            all_rules: vec![],
        }),
        Arc::new(RowsDnsEventsRepo { rows: vec![] }),
        Arc::new(MockNetworkZoneRepo),
        Arc::new(MockSystemConfigRepo),
        Arc::new(MockEventPublisher),
    );

    crate::auth_context::with_context(
        wardnet_common::auth::AuthContext::Admin {
            admin_id: Uuid::nil(),
        },
        async {
            svc.ack_dns_events("00000000-0000-0000-0000-000000000001", 42)
                .await
                .expect("ack should succeed when repo succeeds");
        },
    )
    .await;
}

#[tokio::test]
async fn list_capture_enabled_device_ids_delegates_to_repo() {
    let svc = DeviceServiceImpl::new(
        Arc::new(MockDeviceRepo {
            device: None,
            rule: None,
            all_rules: vec![],
        }),
        Arc::new(MockDnsEventsRepo),
        Arc::new(MockNetworkZoneRepo),
        Arc::new(MockSystemConfigRepo),
        Arc::new(MockEventPublisher),
    );
    let ids = svc
        .list_capture_enabled_device_ids()
        .await
        .expect("should succeed");
    assert!(ids.is_empty()); // MockDeviceRepo returns empty list
}

#[tokio::test]
async fn get_device_capture_settings_returns_settings_when_device_found() {
    let mut device = sample_device(false);
    device.dns_capture_enabled = true;
    let (cap_count, cap_days) = (device.dns_capture_cap_count, device.dns_capture_cap_days);
    let svc = DeviceServiceImpl::new(
        Arc::new(MockDeviceRepo {
            device: Some(device),
            rule: None,
            all_rules: vec![],
        }),
        Arc::new(MockDnsEventsRepo),
        Arc::new(MockNetworkZoneRepo),
        Arc::new(MockSystemConfigRepo),
        Arc::new(MockEventPublisher),
    );
    let result = svc
        .get_device_capture_settings("any-id")
        .await
        .expect("should succeed");
    let (enabled, c, d) = result.expect("device should be found");
    assert!(enabled);
    assert_eq!(c, cap_count);
    assert_eq!(d, cap_days);
}

#[tokio::test]
async fn get_device_capture_settings_returns_none_for_unknown_device() {
    let svc = DeviceServiceImpl::new(
        Arc::new(MockDeviceRepo {
            device: None,
            rule: None,
            all_rules: vec![],
        }),
        Arc::new(MockDnsEventsRepo),
        Arc::new(MockNetworkZoneRepo),
        Arc::new(MockSystemConfigRepo),
        Arc::new(MockEventPublisher),
    );
    let result = svc
        .get_device_capture_settings("unknown-id")
        .await
        .expect("should succeed");
    assert!(result.is_none());
}
