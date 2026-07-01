//! Service-level tests for the push subsystem. Delivery crypto is covered in
//! [`super::sender`]; here we exercise the audience/label mapping, Gone-pruning,
//! subscription ownership, and VAPID idempotency with in-memory collaborators.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;
use wardnet_common::api::{WebPushKeys, WebPushSubscription};
use wardnet_common::auth::AuthContext;
use wardnet_common::device::{Device, DeviceType};
use wardnet_common::event::WardnetEvent;
use wardnet_common::routing::{RoutingRule, RoutingTarget, RuleCreator};
use wardnet_common::tunnel::{Tunnel, TunnelStatus};
use wardnetd_data::repository::push::{
    NewPushSubscription, OWNER_KIND_ADMIN, OWNER_KIND_DEVICE, PushRepository,
    StoredPushSubscription,
};
use wardnetd_data::repository::{DeviceRepository, DeviceRow, TunnelRepository};
use wardnetd_data::secret_store::SecretStore;

use crate::auth_context;
use crate::push::sender::{PushTarget, SendOutcome, VapidKey, WebPushSender};
use crate::push::{PushService, PushServiceImpl};

// ── in-memory push repository ────────────────────────────────────────────────

#[derive(Default)]
struct InMemoryPushRepo {
    rows: Mutex<Vec<StoredPushSubscription>>,
}

#[async_trait]
impl PushRepository for InMemoryPushRepo {
    async fn upsert(&self, sub: NewPushSubscription<'_>) -> anyhow::Result<()> {
        let mut rows = self.rows.lock().unwrap();
        rows.retain(|r| r.endpoint != sub.endpoint);
        rows.push(StoredPushSubscription {
            id: sub.id.to_owned(),
            owner_kind: sub.owner_kind.to_owned(),
            owner_key: sub.owner_key.to_owned(),
            endpoint: sub.endpoint.to_owned(),
            p256dh: sub.p256dh.to_owned(),
            auth: sub.auth.to_owned(),
            created_at: sub.created_at.to_owned(),
        });
        Ok(())
    }

    async fn list_by_owner(
        &self,
        owner_kind: &str,
        owner_key: &str,
    ) -> anyhow::Result<Vec<StoredPushSubscription>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.owner_kind == owner_kind && r.owner_key == owner_key)
            .cloned()
            .collect())
    }

    async fn list_admins(&self) -> anyhow::Result<Vec<StoredPushSubscription>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.owner_kind == OWNER_KIND_ADMIN)
            .cloned()
            .collect())
    }

    async fn delete_by_owner(&self, owner_kind: &str, owner_key: &str) -> anyhow::Result<u64> {
        let mut rows = self.rows.lock().unwrap();
        let before = rows.len();
        rows.retain(|r| !(r.owner_kind == owner_kind && r.owner_key == owner_key));
        Ok((before - rows.len()) as u64)
    }

    async fn delete_by_owner_and_endpoint(
        &self,
        owner_kind: &str,
        owner_key: &str,
        endpoint: &str,
    ) -> anyhow::Result<u64> {
        let mut rows = self.rows.lock().unwrap();
        let before = rows.len();
        rows.retain(|r| {
            !(r.owner_kind == owner_kind && r.owner_key == owner_key && r.endpoint == endpoint)
        });
        Ok((before - rows.len()) as u64)
    }

    async fn delete_by_endpoint(&self, endpoint: &str) -> anyhow::Result<u64> {
        let mut rows = self.rows.lock().unwrap();
        let before = rows.len();
        rows.retain(|r| r.endpoint != endpoint);
        Ok((before - rows.len()) as u64)
    }
}

// ── in-memory secret store + system config ───────────────────────────────────

#[derive(Default)]
struct InMemorySecretStore {
    map: Mutex<HashMap<String, Vec<u8>>>,
}

#[async_trait]
impl SecretStore for InMemorySecretStore {
    async fn put(&self, path: &str, value: &[u8]) -> anyhow::Result<()> {
        self.map
            .lock()
            .unwrap()
            .insert(path.to_owned(), value.to_vec());
        Ok(())
    }
    async fn get(&self, path: &str) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self.map.lock().unwrap().get(path).cloned())
    }
    async fn delete(&self, path: &str) -> anyhow::Result<()> {
        self.map.lock().unwrap().remove(path);
        Ok(())
    }
    async fn list(&self, prefix: &str) -> anyhow::Result<Vec<String>> {
        Ok(self
            .map
            .lock()
            .unwrap()
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect())
    }
}

#[derive(Default)]
struct MapSystemConfig {
    map: Mutex<HashMap<String, String>>,
}

#[async_trait]
impl wardnetd_data::repository::SystemConfigRepository for MapSystemConfig {
    async fn get(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.map.lock().unwrap().get(key).cloned())
    }
    async fn set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        self.map
            .lock()
            .unwrap()
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }
    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.map.lock().unwrap().remove(key);
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
}

// ── device / tunnel stubs: only find_by_id is exercised ──────────────────────

struct StubDeviceRepo {
    devices: Vec<Device>,
}

#[async_trait]
impl DeviceRepository for StubDeviceRepo {
    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<Device>> {
        Ok(self
            .devices
            .iter()
            .find(|d| d.id.to_string() == id)
            .cloned())
    }
    async fn find_by_ip(&self, _ip: &str) -> anyhow::Result<Option<Device>> {
        unimplemented!()
    }
    async fn find_by_mac(&self, _mac: &str) -> anyhow::Result<Option<Device>> {
        unimplemented!()
    }
    async fn find_all(&self) -> anyhow::Result<Vec<Device>> {
        unimplemented!()
    }
    async fn insert(&self, _device: &DeviceRow) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn update_last_seen_and_ip(
        &self,
        _id: &str,
        _ip: &str,
        _last_seen: &str,
    ) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn update_last_seen_batch(&self, _updates: &[(String, String)]) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn update_hostname(&self, _id: &str, _hostname: &str) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn update_name_and_type(
        &self,
        _id: &str,
        _name: Option<&str>,
        _device_type: &str,
    ) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn find_stale(&self, _before: &str) -> anyhow::Result<Vec<Device>> {
        unimplemented!()
    }
    async fn find_rule_for_device(&self, _device_id: &str) -> anyhow::Result<Option<RoutingRule>> {
        unimplemented!()
    }
    async fn find_all_rules(&self) -> anyhow::Result<Vec<RoutingRule>> {
        unimplemented!()
    }
    async fn upsert_user_rule(
        &self,
        _device_id: &str,
        _target_json: &str,
        _now: &str,
    ) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn find_devices_for_tunnel(&self, _tunnel_id: &str) -> anyhow::Result<Vec<Device>> {
        unimplemented!()
    }
    async fn switch_tunnel_rules_to_direct(
        &self,
        _tunnel_id: &str,
        _now: &str,
    ) -> anyhow::Result<Vec<String>> {
        unimplemented!()
    }
    async fn update_admin_locked(&self, _id: &str, _locked: bool) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn count(&self) -> anyhow::Result<i64> {
        unimplemented!()
    }
    async fn update_dns_capture_settings(
        &self,
        _id: &str,
        _enabled: Option<bool>,
        _cap_count: Option<i64>,
        _cap_days: Option<i64>,
    ) -> anyhow::Result<bool> {
        unimplemented!()
    }
    async fn find_all_capture_enabled_ids(&self) -> anyhow::Result<Vec<String>> {
        unimplemented!()
    }
    async fn assign_zone(&self, _device_id: &str, _zone_id: &str) -> anyhow::Result<bool> {
        unimplemented!()
    }
}

struct StubTunnelRepo {
    tunnels: Vec<Tunnel>,
}

#[async_trait]
impl TunnelRepository for StubTunnelRepo {
    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<Tunnel>> {
        Ok(self
            .tunnels
            .iter()
            .find(|t| t.id.to_string() == id)
            .cloned())
    }
    async fn find_all(&self) -> anyhow::Result<Vec<Tunnel>> {
        unimplemented!()
    }
    async fn find_config_by_id(
        &self,
        _id: &str,
    ) -> anyhow::Result<Option<wardnet_common::tunnel::TunnelConfig>> {
        unimplemented!()
    }
    async fn insert(&self, _row: &wardnetd_data::repository::TunnelRow) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn update_status(&self, _id: &str, _status: &str) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn update_dns_override(&self, _id: &str, _value: bool) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn update_stats(
        &self,
        _id: &str,
        _bytes_tx: i64,
        _bytes_rx: i64,
        _last_handshake: Option<&str>,
    ) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn update_endpoint(
        &self,
        _id: &str,
        _endpoint: &str,
        _peer_config_json: &str,
        _server_name: &str,
        _resolved_at: &str,
    ) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn delete(&self, _id: &str) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn next_interface_index(&self) -> anyhow::Result<i64> {
        unimplemented!()
    }
    async fn count(&self) -> anyhow::Result<i64> {
        unimplemented!()
    }
    async fn count_active(&self) -> anyhow::Result<i64> {
        unimplemented!()
    }
}

// ── recording sender ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
struct SentPush {
    endpoint: String,
    payload: String,
}

struct RecordingSender {
    outcome: SendOutcome,
    sent: Mutex<Vec<SentPush>>,
}

impl RecordingSender {
    fn new(outcome: SendOutcome) -> Self {
        Self {
            outcome,
            sent: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl WebPushSender for RecordingSender {
    async fn send(
        &self,
        _vapid: &VapidKey,
        target: PushTarget<'_>,
        payload: Vec<u8>,
    ) -> SendOutcome {
        self.sent.lock().unwrap().push(SentPush {
            endpoint: target.endpoint.to_owned(),
            payload: String::from_utf8(payload).unwrap(),
        });
        self.outcome
    }
}

// ── fixtures ─────────────────────────────────────────────────────────────────

fn test_device(id: Uuid, mac: &str, name: Option<&str>) -> Device {
    Device {
        id,
        mac: mac.to_owned(),
        name: name.map(str::to_owned),
        hostname: None,
        manufacturer: None,
        device_type: DeviceType::Unknown,
        zone_id: Uuid::nil(),
        first_seen: Utc::now(),
        last_seen: Utc::now(),
        last_ip: "192.168.1.10".to_owned(),
        admin_locked: false,
        dns_capture_enabled: false,
        dns_capture_cap_count: 0,
        dns_capture_cap_days: 0,
    }
}

fn test_tunnel(id: Uuid, label: &str) -> Tunnel {
    Tunnel {
        id,
        label: label.to_owned(),
        country_code: "us".to_owned(),
        provider: None,
        interface_name: "wg_ward0".to_owned(),
        endpoint: "1.2.3.4:51820".to_owned(),
        status: TunnelStatus::Up,
        last_handshake: None,
        bytes_tx: 0,
        bytes_rx: 0,
        created_at: Utc::now(),
        override_default_dns: false,
        server_selector: None,
        resolved_server_name: None,
        endpoint_resolved_at: None,
    }
}

struct Harness {
    service: PushServiceImpl,
    push_repo: Arc<InMemoryPushRepo>,
    sender: Arc<RecordingSender>,
    secrets: Arc<InMemorySecretStore>,
}

fn build(devices: Vec<Device>, tunnels: Vec<Tunnel>, outcome: SendOutcome) -> Harness {
    let push_repo = Arc::new(InMemoryPushRepo::default());
    let sender = Arc::new(RecordingSender::new(outcome));
    let secrets = Arc::new(InMemorySecretStore::default());
    let service = PushServiceImpl::new(
        push_repo.clone(),
        Arc::new(StubDeviceRepo { devices }),
        Arc::new(StubTunnelRepo { tunnels }),
        Arc::new(MapSystemConfig::default()),
        secrets.clone(),
        sender.clone(),
    );
    Harness {
        service,
        push_repo,
        sender,
        secrets,
    }
}

/// Dispatch an event the way the daemon listener does: under an admin context.
async fn handle(service: &PushServiceImpl, event: WardnetEvent) {
    auth_context::with_context(
        AuthContext::Admin {
            admin_id: Uuid::nil(),
        },
        service.handle_event(&event),
    )
    .await
    .unwrap();
}

async fn seed(repo: &InMemoryPushRepo, owner_kind: &str, owner_key: &str, endpoint: &str) {
    repo.upsert(NewPushSubscription {
        id: &Uuid::new_v4().to_string(),
        owner_kind,
        owner_key,
        endpoint,
        p256dh: "p256dh",
        auth: "auth",
        created_at: "2026-07-01T00:00:00Z",
    })
    .await
    .unwrap();
}

// ── tests ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn admin_lock_notifies_the_target_device_only() {
    let device_id = Uuid::new_v4();
    let device = test_device(device_id, "aa:bb:cc:00", Some("Kid's iPad"));
    let h = build(vec![device], vec![], SendOutcome::Delivered);
    seed(
        &h.push_repo,
        OWNER_KIND_DEVICE,
        "aa:bb:cc:00",
        "https://push/device",
    )
    .await;
    seed(
        &h.push_repo,
        OWNER_KIND_ADMIN,
        "admin-1",
        "https://push/admin",
    )
    .await;

    handle(
        &h.service,
        WardnetEvent::DeviceAdminLocked {
            device_id,
            locked: true,
            timestamp: Utc::now(),
        },
    )
    .await;

    let sent = h.sender.sent.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].endpoint, "https://push/device");
    assert!(sent[0].payload.contains("Routing locked"));
    assert!(sent[0].payload.contains("locked your routing"));
}

#[tokio::test]
async fn admin_routing_change_targets_device_user_change_targets_admins() {
    let device_id = Uuid::new_v4();
    let tunnel_id = Uuid::new_v4();
    let device = test_device(device_id, "aa:bb:cc:01", Some("Laptop"));
    let tunnel = test_tunnel(tunnel_id, "Sweden #12");
    let h = build(vec![device], vec![tunnel], SendOutcome::Delivered);
    seed(
        &h.push_repo,
        OWNER_KIND_DEVICE,
        "aa:bb:cc:01",
        "https://push/device",
    )
    .await;
    seed(
        &h.push_repo,
        OWNER_KIND_ADMIN,
        "admin-1",
        "https://push/admin",
    )
    .await;

    // Admin-initiated change -> the device is told.
    handle(
        &h.service,
        WardnetEvent::RoutingRuleChanged {
            device_id,
            target: RoutingTarget::Tunnel { tunnel_id },
            previous_target: None,
            changed_by: RuleCreator::Admin,
            timestamp: Utc::now(),
        },
    )
    .await;
    // Device-initiated change -> the admins are told, with the device name.
    handle(
        &h.service,
        WardnetEvent::RoutingRuleChanged {
            device_id,
            target: RoutingTarget::Direct,
            previous_target: None,
            changed_by: RuleCreator::User,
            timestamp: Utc::now(),
        },
    )
    .await;

    let sent = h.sender.sent.lock().unwrap();
    assert_eq!(sent.len(), 2);
    // First push: to the device, mentioning the tunnel label.
    assert_eq!(sent[0].endpoint, "https://push/device");
    assert!(
        sent[0].payload.contains("Sweden #12"),
        "got {}",
        sent[0].payload
    );
    // Second push: to the admin, mentioning the device name + target.
    assert_eq!(sent[1].endpoint, "https://push/admin");
    assert!(
        sent[1].payload.contains("Laptop"),
        "got {}",
        sent[1].payload
    );
    assert!(sent[1].payload.contains("direct (no tunnel)"));
}

#[tokio::test]
async fn tunnel_down_notifies_admins_only_when_interface_absent() {
    let tunnel_id = Uuid::new_v4();
    let tunnel = test_tunnel(tunnel_id, "USA #8");
    let h = build(vec![], vec![tunnel], SendOutcome::Delivered);
    seed(
        &h.push_repo,
        OWNER_KIND_ADMIN,
        "admin-1",
        "https://push/admin",
    )
    .await;

    // A deliberate teardown must NOT notify.
    handle(
        &h.service,
        WardnetEvent::TunnelDown {
            tunnel_id,
            interface_name: "wg_ward0".to_owned(),
            reason: "manual".to_owned(),
            timestamp: Utc::now(),
        },
    )
    .await;
    assert!(h.sender.sent.lock().unwrap().is_empty());

    // The kernel interface vanishing DOES notify.
    handle(
        &h.service,
        WardnetEvent::TunnelDown {
            tunnel_id,
            interface_name: "wg_ward0".to_owned(),
            reason: "interface absent".to_owned(),
            timestamp: Utc::now(),
        },
    )
    .await;

    let sent = h.sender.sent.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].endpoint, "https://push/admin");
    assert!(sent[0].payload.contains("USA #8"));
    assert!(sent[0].payload.contains("went offline"));
}

#[tokio::test]
async fn gone_subscriptions_are_pruned() {
    let tunnel_id = Uuid::new_v4();
    let h = build(vec![], vec![test_tunnel(tunnel_id, "T")], SendOutcome::Gone);
    seed(
        &h.push_repo,
        OWNER_KIND_ADMIN,
        "admin-1",
        "https://push/dead",
    )
    .await;

    handle(
        &h.service,
        WardnetEvent::TunnelStartFailed {
            tunnel_id,
            interface_name: "wg_ward0".to_owned(),
            error: "boom".to_owned(),
            timestamp: Utc::now(),
        },
    )
    .await;

    // The push was attempted, and the dead subscription was removed.
    assert_eq!(h.sender.sent.lock().unwrap().len(), 1);
    assert!(h.push_repo.list_admins().await.unwrap().is_empty());
}

#[tokio::test]
async fn subscribe_picks_owner_from_auth_context() {
    let h = build(vec![], vec![], SendOutcome::Delivered);
    let sub = WebPushSubscription {
        endpoint: "https://push/x".to_owned(),
        keys: WebPushKeys {
            p256dh: "pk".to_owned(),
            auth: "au".to_owned(),
        },
    };

    let admin_id = Uuid::new_v4();
    auth_context::with_context(
        AuthContext::Admin { admin_id },
        h.service.subscribe(sub.clone()),
    )
    .await
    .unwrap();

    let admins = h.push_repo.list_admins().await.unwrap();
    assert_eq!(admins.len(), 1);
    assert_eq!(admins[0].owner_key, admin_id.to_string());

    let device_sub = WebPushSubscription {
        endpoint: "https://push/y".to_owned(),
        keys: WebPushKeys {
            p256dh: "pk".to_owned(),
            auth: "au".to_owned(),
        },
    };
    auth_context::with_context(
        AuthContext::Device {
            mac: "aa:bb:cc:02".to_owned(),
        },
        h.service.subscribe(device_sub),
    )
    .await
    .unwrap();

    let device_subs = h
        .push_repo
        .list_by_owner(OWNER_KIND_DEVICE, "aa:bb:cc:02")
        .await
        .unwrap();
    assert_eq!(device_subs.len(), 1);
}

#[tokio::test]
async fn unsubscribe_by_endpoint_cannot_remove_another_owners_subscription() {
    let h = build(vec![], vec![], SendOutcome::Delivered);
    // An admin owns a subscription.
    seed(
        &h.push_repo,
        OWNER_KIND_ADMIN,
        "admin-1",
        "https://push/admin",
    )
    .await;

    // A device, knowing the admin's endpoint, tries to unsubscribe it.
    auth_context::with_context(
        AuthContext::Device {
            mac: "aa:bb:cc:99".to_owned(),
        },
        h.service.unsubscribe(Some("https://push/admin".to_owned())),
    )
    .await
    .unwrap();

    // The admin's subscription survives — the delete was scoped to the caller.
    let admins = h.push_repo.list_admins().await.unwrap();
    assert_eq!(admins.len(), 1);
    assert_eq!(admins[0].endpoint, "https://push/admin");
}

#[tokio::test]
async fn subscribe_rejects_anonymous_caller() {
    let h = build(vec![], vec![], SendOutcome::Delivered);
    let sub = WebPushSubscription {
        endpoint: "https://push/x".to_owned(),
        keys: WebPushKeys {
            p256dh: "pk".to_owned(),
            auth: "au".to_owned(),
        },
    };
    let result = auth_context::with_context(AuthContext::Anonymous, h.service.subscribe(sub)).await;
    assert!(matches!(result, Err(crate::error::AppError::Forbidden(_))));
}

#[tokio::test]
async fn vapid_public_key_is_generated_once_and_stable() {
    let h = build(vec![], vec![], SendOutcome::Delivered);
    let first = h.service.vapid_public_key().await.unwrap();
    let second = h.service.vapid_public_key().await.unwrap();
    assert_eq!(first, second);
    // Persisted to the secret store, so a fresh service instance over the same
    // store returns the same key rather than minting a new one.
    let reloaded = PushServiceImpl::new(
        Arc::new(InMemoryPushRepo::default()),
        Arc::new(StubDeviceRepo { devices: vec![] }),
        Arc::new(StubTunnelRepo { tunnels: vec![] }),
        Arc::new(MapSystemConfig::default()),
        h.secrets.clone(),
        Arc::new(RecordingSender::new(SendOutcome::Delivered)),
    );
    assert_eq!(reloaded.vapid_public_key().await.unwrap(), first);
}
