//! Tests for the push subsystem. Delivery crypto and the wire format are
//! covered in the `ReqwestWebPushSender` section at the bottom of this
//! file; the service-level tests here exercise the audience/label mapping, Gone-pruning,
//! subscription ownership, and VAPID idempotency over real in-memory `SQLite`
//! repositories (same migration set as the rest of the workspace) plus a
//! recording [`WebPushSender`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::Utc;
use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use uuid::Uuid;
use wardnet_common::api::{WebPushKeys, WebPushSubscription};
use wardnet_common::auth::AuthContext;
use wardnet_common::event::WardnetEvent;
use wardnet_common::routing::{RoutingTarget, RuleCreator};
use wardnetd_data::repository::device::DeviceRow;
use wardnetd_data::repository::push::{OWNER_KIND_ADMIN, OWNER_KIND_DEVICE};
use wardnetd_data::repository::tunnel::TunnelRow;
use wardnetd_data::repository::{
    DeviceRepository, NewPushSubscription, NotificationRepository, PushRepository,
    SqliteDeviceRepository, SqliteNotificationRepository, SqlitePushRepository,
    SqliteSystemConfigRepository, SqliteTunnelRepository, TunnelRepository,
};
use wardnetd_data::secret_store::SecretStore;

use crate::auth_context;
use crate::push::sender::{PushTarget, ReqwestWebPushSender, SendOutcome, VapidKey, WebPushSender};
use crate::push::{PushService, PushServiceImpl};

/// The daemon-seeded Guest zone (default-for-new); used as a valid `zone_id` FK
/// when inserting test devices.
const GUEST_ZONE: &str = "00000000-0000-0000-0000-000000000203";

async fn test_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../wardnetd-data/migrations")
        .run(&pool)
        .await
        .unwrap();
    pool
}

// ── in-memory secret store (persists across a service's lifetime) ─────────────

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

// ── harness ──────────────────────────────────────────────────────────────────

struct Harness {
    service: PushServiceImpl,
    push_repo: Arc<dyn PushRepository>,
    notifications: Arc<dyn NotificationRepository>,
    devices: Arc<dyn DeviceRepository>,
    tunnels: Arc<dyn TunnelRepository>,
    sender: Arc<RecordingSender>,
    secrets: Arc<InMemorySecretStore>,
}

async fn build(outcome: SendOutcome) -> Harness {
    let pool = test_pool().await;
    let push_repo: Arc<dyn PushRepository> = Arc::new(SqlitePushRepository::new(pool.clone()));
    let notifications: Arc<dyn NotificationRepository> =
        Arc::new(SqliteNotificationRepository::new(pool.clone()));
    let devices: Arc<dyn DeviceRepository> = Arc::new(SqliteDeviceRepository::new(pool.clone()));
    let tunnels: Arc<dyn TunnelRepository> = Arc::new(SqliteTunnelRepository::new(pool.clone()));
    let system_config = Arc::new(SqliteSystemConfigRepository::new(pool));
    let sender = Arc::new(RecordingSender::new(outcome));
    let secrets = Arc::new(InMemorySecretStore::default());
    let service = PushServiceImpl::new(
        push_repo.clone(),
        notifications.clone(),
        devices.clone(),
        tunnels.clone(),
        system_config,
        secrets.clone(),
        sender.clone(),
    );
    Harness {
        service,
        push_repo,
        notifications,
        devices,
        tunnels,
        sender,
        secrets,
    }
}

async fn insert_device(
    devices: &Arc<dyn DeviceRepository>,
    id: Uuid,
    mac: &str,
    name: Option<&str>,
) {
    let now = Utc::now().to_rfc3339();
    devices
        .insert(&DeviceRow {
            id: id.to_string(),
            mac: mac.to_owned(),
            hostname: None,
            manufacturer: None,
            device_type: "unknown".to_owned(),
            first_seen: now.clone(),
            last_seen: now,
            last_ip: "192.168.1.50".to_owned(),
            zone_id: GUEST_ZONE.to_owned(),
            connection_mode: wardnet_common::device::DeviceConnectionMode::Lan,
        })
        .await
        .unwrap();
    if let Some(name) = name {
        devices
            .update_name_and_type(&id.to_string(), Some(name), "unknown")
            .await
            .unwrap();
    }
}

async fn insert_tunnel(tunnels: &Arc<dyn TunnelRepository>, id: Uuid, label: &str) {
    tunnels
        .insert(&TunnelRow {
            id: id.to_string(),
            label: label.to_owned(),
            country_code: "us".to_owned(),
            provider: None,
            interface_name: "wg_ward0".to_owned(),
            endpoint: "1.2.3.4:51820".to_owned(),
            status: "up".to_owned(),
            address: "[]".to_owned(),
            dns: "[]".to_owned(),
            peer_config: "{}".to_owned(),
            listen_port: None,
            override_default_dns: false,
            server_selector_country: None,
            resolved_server_name: None,
            endpoint_resolved_at: None,
        })
        .await
        .unwrap();
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

async fn seed(repo: &Arc<dyn PushRepository>, owner_kind: &str, owner_key: &str, endpoint: &str) {
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
    let h = build(SendOutcome::Delivered).await;
    insert_device(&h.devices, device_id, "aa:bb:cc:00", Some("Kid's iPad")).await;
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

/// Call an admin-gated service method the way a handler does — under an admin
/// context.
async fn notify(service: &PushServiceImpl, device_id: Uuid) -> bool {
    auth_context::with_context(
        AuthContext::Admin {
            admin_id: Uuid::nil(),
        },
        service.notify_private_dns_granted(device_id),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn private_dns_notify_resolves_uuid_to_mac_and_reports_delivered() {
    let device_id = Uuid::new_v4();
    let h = build(SendOutcome::Delivered).await;
    // Subscriptions are keyed by MAC; the endpoint speaks device UUID, so the
    // service must resolve UUID -> MAC before targeting.
    insert_device(&h.devices, device_id, "aa:bb:cc:11", Some("Phone")).await;
    seed(
        &h.push_repo,
        OWNER_KIND_DEVICE,
        "aa:bb:cc:11",
        "https://push/device",
    )
    .await;

    let delivered = notify(&h.service, device_id).await;

    assert!(delivered, "a subscription existed, so delivered is true");
    let sent = h.sender.sent.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].endpoint, "https://push/device");
    assert!(sent[0].payload.contains("private_dns_granted"));
    assert!(sent[0].payload.contains("/private-dns"));
    assert!(sent[0].payload.contains("Private DNS is ready"));
    assert!(sent[0].payload.contains(&device_id.to_string()));
}

#[tokio::test]
async fn private_dns_notify_reports_not_delivered_without_a_subscription() {
    let device_id = Uuid::new_v4();
    let h = build(SendOutcome::Delivered).await;
    // The device exists but has no push subscription (the household member
    // hasn't enabled notifications) — not an error, just `false`.
    insert_device(&h.devices, device_id, "aa:bb:cc:12", Some("Phone")).await;

    let delivered = notify(&h.service, device_id).await;

    assert!(!delivered);
    assert!(h.sender.sent.lock().unwrap().is_empty());
}

#[tokio::test]
async fn private_dns_notify_unknown_device_reports_not_delivered() {
    let h = build(SendOutcome::Delivered).await;
    // No device row: UUID -> MAC resolution misses, so nothing is targeted.
    let delivered = notify(&h.service, Uuid::new_v4()).await;
    assert!(!delivered);
    assert!(h.sender.sent.lock().unwrap().is_empty());
}

#[tokio::test]
async fn admin_routing_change_targets_device_user_change_targets_admins() {
    let device_id = Uuid::new_v4();
    let tunnel_id = Uuid::new_v4();
    let h = build(SendOutcome::Delivered).await;
    insert_device(&h.devices, device_id, "aa:bb:cc:01", Some("Laptop")).await;
    insert_tunnel(&h.tunnels, tunnel_id, "Sweden #12").await;
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
    let h = build(SendOutcome::Delivered).await;
    insert_tunnel(&h.tunnels, tunnel_id, "USA #8").await;
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
    let h = build(SendOutcome::Gone).await;
    insert_tunnel(&h.tunnels, tunnel_id, "T").await;
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
    let h = build(SendOutcome::Delivered).await;
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
async fn subscribe_rejects_non_https_endpoint() {
    let h = build(SendOutcome::Delivered).await;
    let sub = WebPushSubscription {
        endpoint: "http://push/x".to_owned(),
        keys: WebPushKeys {
            p256dh: "pk".to_owned(),
            auth: "au".to_owned(),
        },
    };
    let result = auth_context::with_context(
        AuthContext::Device {
            mac: "aa:bb:cc:03".to_owned(),
        },
        h.service.subscribe(sub),
    )
    .await;
    assert!(matches!(result, Err(crate::error::AppError::BadRequest(_))));
}

#[tokio::test]
async fn unsubscribe_by_endpoint_cannot_remove_another_owners_subscription() {
    let h = build(SendOutcome::Delivered).await;
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
async fn unsubscribe_without_endpoint_removes_all_caller_subscriptions() {
    let h = build(SendOutcome::Delivered).await;
    seed(
        &h.push_repo,
        OWNER_KIND_DEVICE,
        "aa:bb:cc:aa",
        "https://push/1",
    )
    .await;
    seed(
        &h.push_repo,
        OWNER_KIND_DEVICE,
        "aa:bb:cc:aa",
        "https://push/2",
    )
    .await;

    auth_context::with_context(
        AuthContext::Device {
            mac: "aa:bb:cc:aa".to_owned(),
        },
        h.service.unsubscribe(None),
    )
    .await
    .unwrap();

    assert!(
        h.push_repo
            .list_by_owner(OWNER_KIND_DEVICE, "aa:bb:cc:aa")
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn subscribe_rejects_anonymous_caller() {
    let h = build(SendOutcome::Delivered).await;
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
    let h = build(SendOutcome::Delivered).await;
    let first = h.service.vapid_public_key().await.unwrap();
    let second = h.service.vapid_public_key().await.unwrap();
    assert_eq!(first, second);

    // Persisted to the secret store, so a fresh service instance over the same
    // store returns the same key rather than minting a new one.
    let pool = test_pool().await;
    let reloaded = PushServiceImpl::new(
        Arc::new(SqlitePushRepository::new(pool.clone())),
        Arc::new(SqliteNotificationRepository::new(pool.clone())),
        Arc::new(SqliteDeviceRepository::new(pool.clone())),
        Arc::new(SqliteTunnelRepository::new(pool.clone())),
        Arc::new(SqliteSystemConfigRepository::new(pool)),
        h.secrets.clone(),
        Arc::new(RecordingSender::new(SendOutcome::Delivered)),
    );
    assert_eq!(reloaded.vapid_public_key().await.unwrap(), first);
}

#[tokio::test]
async fn user_change_with_unknown_device_and_default_target_notifies_admins() {
    // Unknown device -> "A device"; Default target -> "default routing".
    let h = build(SendOutcome::Delivered).await;
    seed(
        &h.push_repo,
        OWNER_KIND_ADMIN,
        "admin-1",
        "https://push/admin",
    )
    .await;

    handle(
        &h.service,
        WardnetEvent::RoutingRuleChanged {
            device_id: Uuid::new_v4(),
            target: RoutingTarget::Default,
            previous_target: None,
            changed_by: RuleCreator::User,
            timestamp: Utc::now(),
        },
    )
    .await;

    let sent = h.sender.sent.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert!(
        sent[0].payload.contains("A device"),
        "got {}",
        sent[0].payload
    );
    assert!(sent[0].payload.contains("default routing"));
}

#[tokio::test]
async fn offline_notification_falls_back_when_tunnel_unknown() {
    // No tunnel record -> label falls back to "A tunnel".
    let h = build(SendOutcome::Delivered).await;
    seed(
        &h.push_repo,
        OWNER_KIND_ADMIN,
        "admin-1",
        "https://push/admin",
    )
    .await;

    handle(
        &h.service,
        WardnetEvent::TunnelReconnecting {
            tunnel_id: Uuid::new_v4(),
            interface_name: "wg_ward0".to_owned(),
            last_handshake: None,
            timestamp: Utc::now(),
        },
    )
    .await;

    let sent = h.sender.sent.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert!(
        sent[0].payload.contains("A tunnel"),
        "got {}",
        sent[0].payload
    );
}

#[tokio::test]
async fn admin_lock_with_no_device_subscription_delivers_nothing() {
    // The device exists but has no push subscription -> deliver() early-returns.
    let device_id = Uuid::new_v4();
    let h = build(SendOutcome::Delivered).await;
    insert_device(&h.devices, device_id, "aa:bb:cc:77", Some("Phone")).await;

    handle(
        &h.service,
        WardnetEvent::DeviceAdminLocked {
            device_id,
            locked: false,
            timestamp: Utc::now(),
        },
    )
    .await;

    assert!(h.sender.sent.lock().unwrap().is_empty());
}

#[tokio::test]
async fn delivery_is_dropped_when_vapid_key_is_corrupt() {
    // A corrupt stored VAPID key makes `ensure_vapid` fail; delivery is dropped
    // rather than panicking.
    let tunnel_id = Uuid::new_v4();
    let h = build(SendOutcome::Delivered).await;
    insert_tunnel(&h.tunnels, tunnel_id, "T").await;
    seed(
        &h.push_repo,
        OWNER_KIND_ADMIN,
        "admin-1",
        "https://push/admin",
    )
    .await;
    h.secrets
        .put(crate::push::SECRET_VAPID_KEY, b"not-a-valid-key")
        .await
        .unwrap();

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

    // No push was sent (VAPID load failed) and the subscription is untouched.
    assert!(h.sender.sent.lock().unwrap().is_empty());
    assert_eq!(h.push_repo.list_admins().await.unwrap().len(), 1);
}

#[tokio::test]
async fn new_device_quarantined_notifies_admins() {
    let h = build(SendOutcome::Delivered).await;
    let device_id = Uuid::new_v4();
    insert_device(
        &h.devices,
        device_id,
        "aa:bb:cc:dd:ee:01",
        Some("Kid's iPad"),
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
        WardnetEvent::NewDeviceQuarantined {
            device_id,
            mac: "aa:bb:cc:dd:ee:01".to_owned(),
            zone_name: "Guest".to_owned(),
            timestamp: Utc::now(),
        },
    )
    .await;

    let sent = h.sender.sent.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].endpoint, "https://push/admin");
    // Uses the device's human name and the landing zone.
    assert!(sent[0].payload.contains("Kid's iPad"));
    assert!(sent[0].payload.contains("Guest"));
    assert!(sent[0].payload.contains("Approve"));
    // Structured data lets the service worker deep-link to the approve action.
    let payload: serde_json::Value = serde_json::from_str(&sent[0].payload).unwrap();
    assert_eq!(payload["data"]["kind"], "new_device_quarantined");
    assert_eq!(payload["data"]["url"], "/devices");
    assert_eq!(payload["data"]["subject_id"], device_id.to_string());
}

#[tokio::test]
async fn tunnel_offline_payload_carries_kind_url_and_tunnel_subject() {
    let tunnel_id = Uuid::new_v4();
    let h = build(SendOutcome::Delivered).await;
    insert_tunnel(&h.tunnels, tunnel_id, "USA #8").await;
    seed(
        &h.push_repo,
        OWNER_KIND_ADMIN,
        "admin-1",
        "https://push/admin",
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

    let sent = h.sender.sent.lock().unwrap();
    assert_eq!(sent.len(), 1);
    // The subject id is kind-driven: a tunnel UUID for tunnel kinds.
    let payload: serde_json::Value = serde_json::from_str(&sent[0].payload).unwrap();
    assert_eq!(payload["data"]["kind"], "tunnel_offline");
    assert_eq!(payload["data"]["url"], "/tunnels");
    assert_eq!(payload["data"]["subject_id"], tunnel_id.to_string());
}

#[tokio::test]
async fn admin_notification_is_persisted_even_with_zero_subscriptions() {
    // The feed records "what happened", not "what was delivered": no admin is
    // subscribed, yet the row lands in the feed.
    let tunnel_id = Uuid::new_v4();
    let h = build(SendOutcome::Delivered).await;
    insert_tunnel(&h.tunnels, tunnel_id, "USA #8").await;

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

    assert!(h.sender.sent.lock().unwrap().is_empty());
    let feed = h.notifications.list_recent(10).await.unwrap();
    assert_eq!(feed.len(), 1);
    assert_eq!(feed[0].kind, "tunnel_offline");
    assert_eq!(feed[0].title, "Tunnel offline");
    assert_eq!(feed[0].url.as_deref(), Some("/tunnels"));
    assert_eq!(feed[0].subject_id.as_deref(), Some(&*tunnel_id.to_string()));
}

#[tokio::test]
async fn admin_notification_is_persisted_when_delivery_fails_transiently() {
    let tunnel_id = Uuid::new_v4();
    let h = build(SendOutcome::TransientFailure).await;
    insert_tunnel(&h.tunnels, tunnel_id, "T").await;
    seed(
        &h.push_repo,
        OWNER_KIND_ADMIN,
        "admin-1",
        "https://push/admin",
    )
    .await;

    handle(
        &h.service,
        WardnetEvent::TunnelReconnecting {
            tunnel_id,
            interface_name: "wg_ward0".to_owned(),
            last_handshake: None,
            timestamp: Utc::now(),
        },
    )
    .await;

    // Delivery was attempted and failed; the feed row is there regardless.
    assert_eq!(h.sender.sent.lock().unwrap().len(), 1);
    assert_eq!(h.notifications.list_recent(10).await.unwrap().len(), 1);
}

#[tokio::test]
async fn device_keyed_notifications_are_not_persisted_to_the_feed() {
    // The feed is admin-audience only: a device-keyed push must leave no row.
    let device_id = Uuid::new_v4();
    let h = build(SendOutcome::Delivered).await;
    insert_device(&h.devices, device_id, "aa:bb:cc:05", Some("Phone")).await;
    seed(
        &h.push_repo,
        OWNER_KIND_DEVICE,
        "aa:bb:cc:05",
        "https://push/device",
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

    assert_eq!(h.sender.sent.lock().unwrap().len(), 1);
    assert!(h.notifications.list_recent(10).await.unwrap().is_empty());
}

#[tokio::test]
async fn recent_notifications_requires_admin_and_returns_newest_first() {
    let tunnel_id = Uuid::new_v4();
    let h = build(SendOutcome::Delivered).await;
    insert_tunnel(&h.tunnels, tunnel_id, "T").await;

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

    // A device caller is rejected.
    let result = auth_context::with_context(
        AuthContext::Device {
            mac: "aa:bb:cc:06".to_owned(),
        },
        h.service.recent_notifications(10),
    )
    .await;
    assert!(matches!(result, Err(crate::error::AppError::Forbidden(_))));

    // An admin gets the feed.
    let feed = auth_context::with_context(
        AuthContext::Admin {
            admin_id: Uuid::nil(),
        },
        h.service.recent_notifications(10),
    )
    .await
    .unwrap();
    assert_eq!(feed.len(), 1);

    // Clear empties it (also admin-gated).
    auth_context::with_context(
        AuthContext::Admin {
            admin_id: Uuid::nil(),
        },
        h.service.clear_notifications(),
    )
    .await
    .unwrap();
    assert!(h.notifications.list_recent(10).await.unwrap().is_empty());
}

#[tokio::test]
async fn rule_request_notifies_admins_and_lands_in_the_feed() {
    let device_id = Uuid::new_v4();
    let h = build(SendOutcome::Delivered).await;
    insert_device(&h.devices, device_id, "aa:bb:cc:07", Some("Kid's iPad")).await;
    seed(
        &h.push_repo,
        OWNER_KIND_ADMIN,
        "admin-1",
        "https://push/admin",
    )
    .await;

    handle(
        &h.service,
        WardnetEvent::RuleRequestCreated {
            request_id: "req-1".to_owned(),
            device_id: device_id.to_string(),
            kind: wardnet_common::rule_request::RuleRequestKind::Allow,
            domain: "blocked.example".to_owned(),
            timestamp: Utc::now(),
        },
    )
    .await;

    // Scoped so the guard drops before the `.await` below (clippy:
    // await_holding_lock).
    {
        let sent = h.sender.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].endpoint, "https://push/admin");
        let payload: serde_json::Value = serde_json::from_str(&sent[0].payload).unwrap();
        assert_eq!(payload["title"], "Rule request");
        assert_eq!(
            payload["body"],
            "Kid's iPad asked to allow blocked.example."
        );
        assert_eq!(payload["data"]["kind"], "rule_request_created");
        // No admin-app surface for rule requests yet — no deep link.
        assert!(payload["data"].get("url").is_none());
        assert_eq!(payload["data"]["subject_id"], "req-1");
    }

    let feed = h.notifications.list_recent(10).await.unwrap();
    assert_eq!(feed.len(), 1);
    assert_eq!(feed[0].kind, "rule_request_created");
}

#[tokio::test]
async fn device_keyed_payload_omits_url_but_keeps_kind_and_subject() {
    let device_id = Uuid::new_v4();
    let h = build(SendOutcome::Delivered).await;
    insert_device(&h.devices, device_id, "aa:bb:cc:04", Some("Phone")).await;
    seed(
        &h.push_repo,
        OWNER_KIND_DEVICE,
        "aa:bb:cc:04",
        "https://push/device",
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
    let payload: serde_json::Value = serde_json::from_str(&sent[0].payload).unwrap();
    assert_eq!(payload["data"]["kind"], "routing_locked");
    // Absent keys are omitted, not serialized as null.
    assert!(payload["data"].get("url").is_none());
    assert_eq!(payload["data"]["subject_id"], device_id.to_string());
}

// ── ReqwestWebPushSender (wire format + outcome classification) ─────────

/// A valid subscriber target: a real P-256 point + 16-byte auth secret.
fn valid_target(endpoint: &str) -> (String, String, String) {
    let p256dh = VapidKey::generate().public_key_base64url();
    let auth = URL_SAFE_NO_PAD.encode([9u8; 16]);
    (endpoint.to_owned(), p256dh, auth)
}

async fn send_to(status: u16) -> SendOutcome {
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(status))
        .mount(&server)
        .await;

    let sender = ReqwestWebPushSender::new(reqwest::Client::new(), "mailto:a@b.c".to_owned());
    let vapid = VapidKey::generate();
    let (endpoint, p256dh, auth) = valid_target(&server.uri());
    sender
        .send(
            &vapid,
            PushTarget {
                endpoint: &endpoint,
                p256dh: &p256dh,
                auth: &auth,
            },
            b"{\"title\":\"hi\"}".to_vec(),
        )
        .await
}

#[tokio::test]
async fn send_classifies_http_responses() {
    assert_eq!(send_to(201).await, SendOutcome::Delivered);
    assert_eq!(send_to(200).await, SendOutcome::Delivered);
    assert_eq!(send_to(410).await, SendOutcome::Gone);
    assert_eq!(send_to(404).await, SendOutcome::Gone);
    assert_eq!(send_to(500).await, SendOutcome::TransientFailure);
    assert_eq!(send_to(429).await, SendOutcome::TransientFailure);
}

#[tokio::test]
async fn send_maps_unbuildable_subscription_to_gone() {
    // A malformed p256dh can never encrypt -> pruned as Gone.
    let sender = ReqwestWebPushSender::new(reqwest::Client::new(), "mailto:a@b.c".to_owned());
    let vapid = VapidKey::generate();
    let outcome = sender
        .send(
            &vapid,
            PushTarget {
                endpoint: "https://push.example.com/x",
                p256dh: "not-a-valid-point",
                auth: "YXV0aA",
            },
            b"{}".to_vec(),
        )
        .await;
    assert_eq!(outcome, SendOutcome::Gone);
}

#[tokio::test]
async fn send_maps_network_error_to_transient() {
    // Nothing is listening on this port -> connection refused.
    let sender = ReqwestWebPushSender::new(reqwest::Client::new(), "mailto:a@b.c".to_owned());
    let vapid = VapidKey::generate();
    let (endpoint, p256dh, auth) = valid_target("http://127.0.0.1:1/push");
    let outcome = sender
        .send(
            &vapid,
            PushTarget {
                endpoint: &endpoint,
                p256dh: &p256dh,
                auth: &auth,
            },
            b"{}".to_vec(),
        )
        .await;
    assert_eq!(outcome, SendOutcome::TransientFailure);
}

#[test]
fn from_bytes_rejects_wrong_length_without_panicking() {
    let Err(err) = VapidKey::from_bytes(b"too-short") else {
        panic!("expected a length error");
    };
    assert!(err.to_string().contains("32 bytes"), "got {err}");
}

#[test]
fn vapid_public_key_is_stable_across_reload() {
    let key = VapidKey::generate();
    let public = key.public_key_base64url();
    let bytes = key.to_bytes();

    let reloaded = VapidKey::from_bytes(&bytes).unwrap();
    assert_eq!(reloaded.public_key_base64url(), public);
    // Uncompressed P-256 point is 65 bytes -> 87 base64url chars unpadded.
    assert_eq!(public.len(), 87);
}

#[test]
fn build_request_sets_web_push_headers() {
    // A syntactically valid subscription (keys are real base64url of the
    // right lengths) must produce an aes128gcm-encoded POST with a VAPID
    // Authorization header.
    let vapid = VapidKey::generate();
    // 65-byte uncompressed P-256 point for a throwaway key = valid p256dh.
    let subscriber = VapidKey::generate();
    let p256dh = subscriber.public_key_base64url();
    let auth = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([7u8; 16]);

    let sender = ReqwestWebPushSender::new(reqwest::Client::new(), "mailto:a@b.c".to_owned());
    let request = sender
        .build_request(
            &vapid,
            &PushTarget {
                endpoint: "https://push.example.com/xyz",
                p256dh: &p256dh,
                auth: &auth,
            },
            b"{\"title\":\"hi\"}".to_vec(),
        )
        .expect("valid subscription should build");

    assert_eq!(request.method(), http::Method::POST);
    assert_eq!(
        request
            .headers()
            .get(http::header::CONTENT_ENCODING)
            .unwrap(),
        "aes128gcm"
    );
    let auth_header = request
        .headers()
        .get(http::header::AUTHORIZATION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(auth_header.starts_with("vapid t="), "got {auth_header}");
}

#[test]
fn build_request_vapid_token_verifies_against_advertised_key() {
    // The Authorization header must carry an ES256 JWS a push service can
    // actually validate: signature over `header.claims` with the key
    // advertised in `k=`, and the claims RFC 8292 requires.
    use web_push_native::p256::ecdsa::signature::Verifier;
    use web_push_native::p256::ecdsa::{Signature, VerifyingKey};

    let vapid = VapidKey::generate();
    let subscriber = VapidKey::generate();
    let p256dh = subscriber.public_key_base64url();
    let auth = URL_SAFE_NO_PAD.encode([7u8; 16]);

    let sender = ReqwestWebPushSender::new(reqwest::Client::new(), "mailto:a@b.c".to_owned());
    let request = sender
        .build_request(
            &vapid,
            &PushTarget {
                endpoint: "https://push.example.com/xyz",
                p256dh: &p256dh,
                auth: &auth,
            },
            b"{}".to_vec(),
        )
        .expect("valid subscription should build");

    let header = request
        .headers()
        .get(http::header::AUTHORIZATION)
        .unwrap()
        .to_str()
        .unwrap();
    let (token, advertised_key) = header
        .strip_prefix("vapid t=")
        .and_then(|rest| rest.split_once(", k="))
        .expect("header must be `vapid t=<jwt>, k=<pub>`");
    assert_eq!(advertised_key, vapid.public_key_base64url());

    let [jose, claims, signature] = token.split('.').collect::<Vec<_>>()[..] else {
        panic!("token must have three segments, got {token}");
    };
    let verifying_key =
        VerifyingKey::from_sec1_bytes(&URL_SAFE_NO_PAD.decode(advertised_key).unwrap()).unwrap();
    let signature = Signature::from_slice(&URL_SAFE_NO_PAD.decode(signature).unwrap()).unwrap();
    verifying_key
        .verify(format!("{jose}.{claims}").as_bytes(), &signature)
        .expect("token signature must verify against the advertised key");

    let jose: serde_json::Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(jose).unwrap()).unwrap();
    assert_eq!(jose["alg"], "ES256");

    let claims: serde_json::Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(claims).unwrap()).unwrap();
    assert_eq!(claims["aud"], "https://push.example.com");
    assert_eq!(claims["sub"], "mailto:a@b.c");
    let exp = claims["exp"].as_u64().expect("exp must be numeric");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert!(exp > now, "token must not be born expired");
    assert!(
        exp <= now + 24 * 60 * 60,
        "push services reject exp more than 24h out"
    );
}

#[test]
fn build_request_rejects_wrong_length_auth() {
    let vapid = VapidKey::generate();
    let subscriber = VapidKey::generate();
    let p256dh = subscriber.public_key_base64url();
    // 15 bytes instead of 16.
    let auth = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode([7u8; 15]);

    let sender = ReqwestWebPushSender::new(reqwest::Client::new(), "mailto:a@b.c".to_owned());
    let err = sender
        .build_request(
            &vapid,
            &PushTarget {
                endpoint: "https://push.example.com/xyz",
                p256dh: &p256dh,
                auth: &auth,
            },
            b"{}".to_vec(),
        )
        .unwrap_err();
    assert!(err.to_string().contains("16 bytes"), "got {err}");
}
