use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;
use wardnet_common::device::{Device, DeviceConnectionMode, DeviceType};
use wardnet_common::dns::CustomDnsRecord;
use wardnet_common::event::WardnetEvent;

use crate::auth_context;
use crate::ddns::{DdnsService, DdnsStatus};
use crate::device::service::DeviceService;
use crate::dns_local::DnsLocalService;
use crate::entitlement::Entitlement;
use crate::error::AppError;
use crate::event::{BroadcastEventBus, EventPublisher};
use crate::private_dns::{PrivateDnsService, PrivateDnsServiceImpl, encode_base32, mint_token};
use crate::secret_store::SecretStore;
use crate::tls::{TlsService, TlsStatus};
use wardnetd_data::repository::private_dns::{
    DeviceAlreadyGrantedPrivateDnsError, PrivateDnsGrantRepository, PrivateDnsGrantRow,
};
use wardnetd_data::repository::system_config::SystemConfigRepository;

const DOMAIN: &str = "casa.my.wardnet.services";

fn admin_ctx() -> AuthContext {
    AuthContext::Admin {
        admin_id: Uuid::new_v4(),
    }
}

// -- In-memory SecretStore ------------------------------------------------
//
// Backs the cert/key `device_profile` signs with. Empty by default (no cert
// issued); the profile happy-path test seeds a self-signed pair.
#[derive(Default)]
struct MockSecretStore {
    store: Mutex<HashMap<String, Vec<u8>>>,
}

#[async_trait]
impl SecretStore for MockSecretStore {
    async fn put(&self, path: &str, value: &[u8]) -> anyhow::Result<()> {
        self.store
            .lock()
            .unwrap()
            .insert(path.to_owned(), value.to_vec());
        Ok(())
    }
    async fn get(&self, path: &str) -> anyhow::Result<Option<Vec<u8>>> {
        Ok(self.store.lock().unwrap().get(path).cloned())
    }
    async fn delete(&self, path: &str) -> anyhow::Result<()> {
        self.store.lock().unwrap().remove(path);
        Ok(())
    }
    async fn list(&self, prefix: &str) -> anyhow::Result<Vec<String>> {
        Ok(self
            .store
            .lock()
            .unwrap()
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect())
    }
}

// -- Mock PrivateDnsGrantRepository ---------------------------------------

#[derive(Default)]
struct MockGrantRepo {
    rows: Mutex<Vec<PrivateDnsGrantRow>>,
}

#[async_trait]
impl PrivateDnsGrantRepository for MockGrantRepo {
    async fn insert(&self, row: &PrivateDnsGrantRow) -> anyhow::Result<()> {
        let mut rows = self.rows.lock().unwrap();
        if rows.iter().any(|r| r.device_id == row.device_id) {
            return Err(anyhow::Error::new(DeviceAlreadyGrantedPrivateDnsError));
        }
        rows.push(row.clone());
        Ok(())
    }
    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<PrivateDnsGrantRow>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.id == id)
            .cloned())
    }
    async fn find_by_device_id(
        &self,
        device_id: &str,
    ) -> anyhow::Result<Option<PrivateDnsGrantRow>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.device_id == device_id)
            .cloned())
    }
    async fn find_by_token(&self, token: &str) -> anyhow::Result<Option<PrivateDnsGrantRow>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.token == token)
            .cloned())
    }
    async fn find_all(&self) -> anyhow::Result<Vec<PrivateDnsGrantRow>> {
        Ok(self.rows.lock().unwrap().clone())
    }
    async fn delete(&self, id: &str) -> anyhow::Result<()> {
        self.rows.lock().unwrap().retain(|r| r.id != id);
        Ok(())
    }
}

// -- Mock SystemConfigRepository (HashMap-backed get/set) ------------------

#[derive(Default)]
struct MockSystemConfig {
    data: Mutex<HashMap<String, String>>,
}

#[async_trait]
impl SystemConfigRepository for MockSystemConfig {
    async fn get(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.data.lock().unwrap().get(key).cloned())
    }
    async fn set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        self.data
            .lock()
            .unwrap()
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }
    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.data.lock().unwrap().remove(key);
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

// -- Mock DeviceService (only `get_device` is exercised) ------------------

#[derive(Default)]
struct MockDeviceService {
    devices: Mutex<HashMap<String, Device>>,
}

impl MockDeviceService {
    fn add_device(&self) -> Uuid {
        let id = Uuid::new_v4();
        let device = Device {
            id,
            mac: format!("aa:bb:cc:00:00:{:02x}", self.devices.lock().unwrap().len()),
            name: Some("phone".to_owned()),
            hostname: None,
            manufacturer: None,
            manufacturer_source: None,
            is_randomized: false,
            device_type: DeviceType::Unknown,
            first_seen: Utc::now(),
            last_seen: Utc::now(),
            last_ip: "192.168.1.50".to_owned(),
            admin_locked: false,
            zone_id: Uuid::new_v4(),
            dns_capture_enabled: false,
            dns_capture_cap_count: 0,
            dns_capture_cap_days: 0,
            connection_mode: DeviceConnectionMode::Lan,
        };
        self.devices.lock().unwrap().insert(id.to_string(), device);
        id
    }
}

#[async_trait]
impl DeviceService for MockDeviceService {
    async fn get_device(&self, device_id: &str) -> Result<Option<Device>, AppError> {
        Ok(self.devices.lock().unwrap().get(device_id).cloned())
    }
    async fn clear_remote_connection_mode(&self, _device_id: &str) -> Result<(), AppError> {
        Ok(())
    }
    async fn get_device_for_ip(
        &self,
        _ip: &str,
    ) -> Result<wardnet_common::api::DeviceMeResponse, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn set_rule_for_ip(
        &self,
        _ip: &str,
        _target: wardnet_common::routing::RoutingTarget,
    ) -> Result<wardnet_common::api::SetMyRuleResponse, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn set_rule(
        &self,
        _device_id: &str,
        _target: wardnet_common::routing::RoutingTarget,
    ) -> Result<(), AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn current_rules(
        &self,
    ) -> Result<HashMap<Uuid, wardnet_common::routing::RoutingTarget>, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn get_rule_for_device(
        &self,
        _device_id: &str,
    ) -> Result<Option<wardnet_common::routing::RoutingTarget>, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn update_admin_locked(&self, _device_id: &str, _locked: bool) -> Result<(), AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn get_dns_capture_settings(
        &self,
        _device_id: &str,
    ) -> Result<wardnet_common::api::DnsCaptureSettingsResponse, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn update_dns_capture_settings(
        &self,
        _device_id: &str,
        _enabled: Option<bool>,
        _cap_count: Option<i64>,
        _cap_days: Option<i64>,
    ) -> Result<(), AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn set_my_capture_enabled(
        &self,
        _ip: &str,
        _enabled: bool,
    ) -> Result<wardnet_common::api::DnsCaptureSettingsResponse, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn fetch_pending_dns_events(
        &self,
        _device_id: &str,
        _after_id: i64,
        _limit: i64,
    ) -> Result<Vec<wardnet_common::api::DnsEventItem>, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn ack_dns_events(&self, _device_id: &str, _up_to_id: i64) -> Result<(), AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn list_capture_enabled_device_ids(&self) -> Result<Vec<String>, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn get_device_capture_settings(
        &self,
        _device_id: &str,
    ) -> Result<Option<(bool, i64, i64)>, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
}

// -- Mock DdnsService (only `status` is exercised) -------------------------

struct MockDdnsService {
    status: Mutex<DdnsStatus>,
}

impl MockDdnsService {
    fn wardnet() -> Self {
        Self {
            status: Mutex::new(DdnsStatus {
                provider: Some("wardnet".to_owned()),
                fqdn: Some(DOMAIN.to_owned()),
                last_public_ip: None,
                suspended: false,
            }),
        }
    }

    fn set_provider(&self, provider: Option<&str>, fqdn: Option<&str>) {
        let mut status = self.status.lock().unwrap();
        status.provider = provider.map(str::to_owned);
        status.fqdn = fqdn.map(str::to_owned);
    }
}

#[async_trait]
impl DdnsService for MockDdnsService {
    async fn status(&self) -> Result<DdnsStatus, AppError> {
        Ok(self.status.lock().unwrap().clone())
    }
    async fn request_enrollment_code(&self, _email: String) -> Result<(), AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn enroll(&self, _code: String) -> Result<(), AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn check_slug(&self, _slug: String) -> Result<bool, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn register_network(
        &self,
        _slug: String,
        _display_name: Option<String>,
    ) -> Result<crate::ddns::DdnsRegistration, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn configure_cloudflare(
        &self,
        _token: String,
        _domain: String,
    ) -> Result<crate::ddns::DdnsRegistration, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn refresh_public_ip(&self) -> Result<Option<Ipv4Addr>, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn teardown(&self) -> Result<(), AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn resolution_check(
        &self,
    ) -> Result<wardnet_common::api::DdnsResolutionCheckResponse, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn set_acme_challenge(&self, _values: &[String]) -> Result<(), AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn clear_acme_challenge(&self) -> Result<(), AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
}

// -- Mock TlsService (only `status` is exercised) --------------------------

struct MockTlsService {
    status: Mutex<TlsStatus>,
}

impl MockTlsService {
    fn with_status(status: TlsStatus) -> Self {
        Self {
            status: Mutex::new(status),
        }
    }

    fn issued() -> Self {
        Self::with_status(TlsStatus::UpToDate {
            domain: DOMAIN.to_owned(),
            not_after: Utc::now() + chrono::Duration::days(60),
        })
    }

    fn set_status(&self, status: TlsStatus) {
        *self.status.lock().unwrap() = status;
    }
}

#[async_trait]
impl TlsService for MockTlsService {
    async fn status(&self) -> Result<TlsStatus, AppError> {
        Ok(self.status.lock().unwrap().clone())
    }
    async fn ensure_certificate(&self) -> Result<TlsStatus, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn mark_provisioning_started(&self) -> Result<(), AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn provisioning_status(
        &self,
    ) -> Result<wardnet_common::api::TlsStatusResponse, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn teardown(&self) -> Result<(), AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
}

// -- Mock DnsLocalService (record CRUD subset) -----------------------------

#[derive(Default)]
struct MockDnsLocalService {
    records: Mutex<Vec<CustomDnsRecord>>,
}

impl MockDnsLocalService {
    fn record_domains(&self) -> Vec<String> {
        self.records
            .lock()
            .unwrap()
            .iter()
            .map(|r| r.domain.clone())
            .collect()
    }
}

#[async_trait]
impl DnsLocalService for MockDnsLocalService {
    async fn upsert_record(
        &self,
        req: wardnet_common::api::UpsertRecordRequest,
    ) -> Result<wardnet_common::api::UpsertRecordResponse, AppError> {
        let mut records = self.records.lock().unwrap();
        records.retain(|r| !(r.domain == req.domain && r.record_type == req.record_type));
        let record = CustomDnsRecord {
            id: Uuid::new_v4(),
            zone_id: req.zone_id,
            domain: req.domain,
            record_type: req.record_type,
            value: req.value,
            ttl: req.ttl,
            enabled: req.enabled,
            source: req.source,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        records.push(record.clone());
        Ok(wardnet_common::api::UpsertRecordResponse {
            record,
            message: "ok".to_owned(),
        })
    }
    async fn list_records(&self) -> Result<wardnet_common::api::ListRecordsResponse, AppError> {
        Ok(wardnet_common::api::ListRecordsResponse {
            records: self.records.lock().unwrap().clone(),
        })
    }
    async fn delete_record(
        &self,
        id: Uuid,
    ) -> Result<wardnet_common::api::DeleteRecordResponse, AppError> {
        self.records.lock().unwrap().retain(|r| r.id != id);
        Ok(wardnet_common::api::DeleteRecordResponse {
            message: "ok".to_owned(),
        })
    }
    async fn list_zones(&self) -> Result<wardnet_common::api::ListZonesResponse, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn get_zone(&self, _id: Uuid) -> Result<wardnet_common::api::GetZoneResponse, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn create_zone(
        &self,
        _req: wardnet_common::api::CreateZoneRequest,
    ) -> Result<wardnet_common::api::CreateZoneResponse, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn update_zone(
        &self,
        _id: Uuid,
        _req: wardnet_common::api::UpdateZoneRequest,
    ) -> Result<wardnet_common::api::UpdateZoneResponse, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn delete_zone(
        &self,
        _id: Uuid,
    ) -> Result<wardnet_common::api::DeleteZoneResponse, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn list_zone_records(
        &self,
        _zone_id: Uuid,
    ) -> Result<wardnet_common::api::ListRecordsResponse, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn get_record(
        &self,
        _id: Uuid,
    ) -> Result<wardnet_common::api::GetRecordResponse, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn create_record(
        &self,
        _req: wardnet_common::api::CreateRecordRequest,
    ) -> Result<wardnet_common::api::CreateRecordResponse, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn update_record(
        &self,
        _id: Uuid,
        _req: wardnet_common::api::UpdateRecordRequest,
    ) -> Result<wardnet_common::api::UpdateRecordResponse, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn list_forwarding_rules(
        &self,
    ) -> Result<wardnet_common::api::ListForwardingRulesResponse, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn get_forwarding_rule(
        &self,
        _id: Uuid,
    ) -> Result<wardnet_common::api::GetForwardingRuleResponse, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn create_forwarding_rule(
        &self,
        _req: wardnet_common::api::CreateForwardingRuleRequest,
    ) -> Result<wardnet_common::api::CreateForwardingRuleResponse, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn update_forwarding_rule(
        &self,
        _id: Uuid,
        _req: wardnet_common::api::UpdateForwardingRuleRequest,
    ) -> Result<wardnet_common::api::UpdateForwardingRuleResponse, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn delete_forwarding_rule(
        &self,
        _id: Uuid,
    ) -> Result<wardnet_common::api::DeleteForwardingRuleResponse, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
}

// -- Mock PushService (only `notify_private_dns_granted` is exercised) ------
//
// Records the device ids it was asked to nudge and returns a configurable
// `delivered` result, so the notify-orchestration tests can assert the service
// delegates only after the grant check and passes the delivery flag straight
// back.
#[derive(Default)]
struct MockPushService {
    notified: Mutex<Vec<Uuid>>,
    /// The `delivered` value `notify_private_dns_granted` returns.
    delivered: bool,
}

#[async_trait]
impl crate::push::PushService for MockPushService {
    async fn vapid_public_key(&self) -> Result<String, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn subscribe(
        &self,
        _sub: wardnet_common::api::WebPushSubscription,
    ) -> Result<(), AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn unsubscribe(&self, _endpoint: Option<String>) -> Result<(), AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn handle_event(&self, _event: &WardnetEvent) -> Result<(), AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn recent_notifications(
        &self,
        _limit: u32,
    ) -> Result<Vec<wardnetd_data::repository::StoredNotification>, AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn clear_notifications(&self) -> Result<(), AppError> {
        unimplemented!("not exercised by private-dns tests")
    }
    async fn notify_private_dns_granted(&self, device_id: Uuid) -> Result<bool, AppError> {
        self.notified.lock().unwrap().push(device_id);
        Ok(self.delivered)
    }
}

// -- Harness ---------------------------------------------------------------

struct Harness {
    grants: Arc<MockGrantRepo>,
    system_config: Arc<MockSystemConfig>,
    devices: Arc<MockDeviceService>,
    ddns: Arc<MockDdnsService>,
    tls: Arc<MockTlsService>,
    dns_local: Arc<MockDnsLocalService>,
    entitlement: Arc<Entitlement>,
    events: Arc<BroadcastEventBus>,
    secrets: Arc<MockSecretStore>,
    push: Arc<MockPushService>,
    service: PrivateDnsServiceImpl,
}

fn harness() -> Harness {
    harness_with_delivery(false)
}

/// A harness whose push mock reports the given `delivered` result — the notify
/// tests use this to distinguish the has-subscription and no-subscription paths.
fn harness_with_delivery(delivered: bool) -> Harness {
    let grants = Arc::new(MockGrantRepo::default());
    let system_config = Arc::new(MockSystemConfig::default());
    let devices = Arc::new(MockDeviceService::default());
    let ddns = Arc::new(MockDdnsService::wardnet());
    let tls = Arc::new(MockTlsService::issued());
    let dns_local = Arc::new(MockDnsLocalService::default());
    // Entitled by default — individual tests flip premium off to exercise
    // the gates.
    let entitlement = Entitlement::shared();
    entitlement.set_premium(true);
    let events = Arc::new(BroadcastEventBus::new(16));
    let secrets = Arc::new(MockSecretStore::default());
    let push = Arc::new(MockPushService {
        delivered,
        ..Default::default()
    });

    let service = PrivateDnsServiceImpl::new(
        grants.clone(),
        system_config.clone(),
        devices.clone(),
        ddns.clone(),
        tls.clone(),
        dns_local.clone(),
        entitlement.clone(),
        events.clone(),
        secrets.clone(),
        push.clone(),
        Ipv4Addr::new(192, 168, 1, 1),
    );
    Harness {
        grants,
        system_config,
        devices,
        ddns,
        tls,
        dns_local,
        entitlement,
        events,
        secrets,
        push,
        service,
    }
}

async fn enable(h: &Harness) {
    auth_context::with_context(admin_ctx(), h.service.set_enabled(true))
        .await
        .expect("enable");
}

// -- Enable / disable ------------------------------------------------------

#[tokio::test]
async fn enable_persists_seeds_wildcard_and_publishes() {
    let h = harness();
    let mut rx = h.events.subscribe();

    let status = auth_context::with_context(admin_ctx(), h.service.set_enabled(true))
        .await
        .expect("enable succeeds");

    assert!(status.enabled);
    assert_eq!(status.domain.as_deref(), Some(DOMAIN));
    assert!(h.system_config.get("private_dns_enabled").await.unwrap() == Some("true".to_owned()));
    assert_eq!(h.dns_local.record_domains(), vec![format!("*.{DOMAIN}")]);
    assert!(matches!(
        rx.try_recv(),
        Ok(WardnetEvent::PrivateDnsChanged { enabled: true, .. })
    ));
}

#[tokio::test]
async fn is_enabled_reflects_the_flag_without_a_ddns_round_trip() {
    let h = harness();
    assert!(
        !auth_context::with_context(admin_ctx(), h.service.is_enabled())
            .await
            .expect("is_enabled reads only the config flag")
    );

    enable(&h).await;
    // Break DDNS after enabling: is_enabled must still report true (it never
    // consults the provider), which is the whole point of the accessor — a
    // DDNS hiccup must not make the runner think the feature is off.
    h.ddns.set_provider(None, None);
    assert!(
        auth_context::with_context(admin_ctx(), h.service.is_enabled())
            .await
            .expect("is_enabled")
    );
}

#[tokio::test]
async fn enable_without_entitlement_is_forbidden() {
    let h = harness();
    h.entitlement.set_premium(false);

    let err = auth_context::with_context(admin_ctx(), h.service.set_enabled(true))
        .await
        .expect_err("enable must be rejected");
    assert!(matches!(err, AppError::Forbidden(_)));
    assert!(
        h.system_config
            .get("private_dns_enabled")
            .await
            .unwrap()
            .is_none()
    );
    assert!(h.dns_local.record_domains().is_empty());
}

#[tokio::test]
async fn enable_without_wardnet_provider_conflicts() {
    let h = harness();
    h.ddns
        .set_provider(Some("cloudflare"), Some("byod.example.com"));

    let err = auth_context::with_context(admin_ctx(), h.service.set_enabled(true))
        .await
        .expect_err("enable must be rejected");
    assert!(matches!(err, AppError::Conflict(_)));
}

#[tokio::test]
async fn enable_without_issued_cert_conflicts() {
    let h = harness();
    h.tls.set_status(TlsStatus::Pending {
        domain: DOMAIN.to_owned(),
    });

    let err = auth_context::with_context(admin_ctx(), h.service.set_enabled(true))
        .await
        .expect_err("enable must be rejected");
    assert!(matches!(err, AppError::Conflict(_)));
}

#[tokio::test]
async fn disable_is_allowed_without_entitlement_and_removes_wildcard() {
    let h = harness();
    enable(&h).await;
    h.entitlement.set_premium(false);

    let status = auth_context::with_context(admin_ctx(), h.service.set_enabled(false))
        .await
        .expect("disable always succeeds");

    assert!(!status.enabled);
    assert_eq!(
        h.system_config.get("private_dns_enabled").await.unwrap(),
        Some("false".to_owned())
    );
    assert!(h.dns_local.record_domains().is_empty());
}

// -- Grant / revoke --------------------------------------------------------

#[tokio::test]
async fn grant_mints_a_16_char_base32_token() {
    let h = harness();
    enable(&h).await;
    let device_id = h.devices.add_device();

    let grant = auth_context::with_context(admin_ctx(), h.service.grant_device(device_id))
        .await
        .expect("grant succeeds");

    assert_eq!(grant.device_id, device_id);
    assert_eq!(grant.token.len(), 16);
    assert!(
        grant
            .token
            .chars()
            .all(|c| c.is_ascii_lowercase() || ('2'..='7').contains(&c)),
        "token must be lowercase base-32: {}",
        grant.token
    );
    let stored = h.grants.find_by_token(&grant.token).await.unwrap();
    assert!(stored.is_some(), "grant row must be persisted");
}

#[tokio::test]
async fn grant_requires_the_feature_enabled() {
    let h = harness();
    let device_id = h.devices.add_device();

    let err = auth_context::with_context(admin_ctx(), h.service.grant_device(device_id))
        .await
        .expect_err("grant while disabled must be rejected");
    assert!(matches!(err, AppError::Conflict(_)));
}

#[tokio::test]
async fn grant_without_entitlement_is_forbidden() {
    let h = harness();
    enable(&h).await;
    let device_id = h.devices.add_device();
    h.entitlement.set_premium(false);

    let err = auth_context::with_context(admin_ctx(), h.service.grant_device(device_id))
        .await
        .expect_err("grant must be rejected");
    assert!(matches!(err, AppError::Forbidden(_)));
}

#[tokio::test]
async fn grant_for_unknown_device_is_not_found() {
    let h = harness();
    enable(&h).await;

    let err = auth_context::with_context(admin_ctx(), h.service.grant_device(Uuid::new_v4()))
        .await
        .expect_err("grant must be rejected");
    assert!(matches!(err, AppError::NotFound(_)));
}

#[tokio::test]
async fn second_grant_for_the_same_device_conflicts() {
    let h = harness();
    enable(&h).await;
    let device_id = h.devices.add_device();

    auth_context::with_context(admin_ctx(), h.service.grant_device(device_id))
        .await
        .expect("first grant succeeds");
    let err = auth_context::with_context(admin_ctx(), h.service.grant_device(device_id))
        .await
        .expect_err("second grant must be rejected");
    assert!(matches!(err, AppError::Conflict(_)));
}

#[tokio::test]
async fn revoke_deletes_the_grant_and_cuts_token_resolution() {
    let h = harness();
    enable(&h).await;
    let device_id = h.devices.add_device();
    let grant = auth_context::with_context(admin_ctx(), h.service.grant_device(device_id))
        .await
        .expect("grant");

    let mut rx = h.events.subscribe();
    auth_context::with_context(admin_ctx(), h.service.revoke_grant(grant.id))
        .await
        .expect("revoke succeeds");

    let resolved = auth_context::with_context(admin_ctx(), h.service.resolve_token(&grant.token))
        .await
        .expect("resolve");
    assert!(resolved.is_none(), "revoked token must no longer resolve");

    // A PrivateDnsGrantRevoked(device_id) must fire so the DoT listener can
    // tear down the device's live sessions (not just block reconnects).
    let mut saw_revoked = false;
    while let Ok(event) = rx.try_recv() {
        if let WardnetEvent::PrivateDnsGrantRevoked { device_id: d, .. } = event {
            assert_eq!(d, device_id);
            saw_revoked = true;
        }
    }
    assert!(saw_revoked, "revoke must publish PrivateDnsGrantRevoked");
}

#[tokio::test]
async fn revoke_unknown_grant_is_not_found() {
    let h = harness();

    let err = auth_context::with_context(admin_ctx(), h.service.revoke_grant(Uuid::new_v4()))
        .await
        .expect_err("revoke must be rejected");
    assert!(matches!(err, AppError::NotFound(_)));
}

#[tokio::test]
async fn resolve_token_round_trips_and_rejects_unknown() {
    let h = harness();
    enable(&h).await;
    let device_id = h.devices.add_device();
    let grant = auth_context::with_context(admin_ctx(), h.service.grant_device(device_id))
        .await
        .expect("grant");

    let resolved = auth_context::with_context(admin_ctx(), h.service.resolve_token(&grant.token))
        .await
        .expect("resolve")
        .expect("token must resolve");
    assert_eq!(resolved.device_id, device_id);

    let unknown =
        auth_context::with_context(admin_ctx(), h.service.resolve_token("nosuchtoken12345"))
            .await
            .expect("resolve");
    assert!(unknown.is_none());
}

#[tokio::test]
async fn list_grants_returns_all_rows() {
    let h = harness();
    enable(&h).await;
    let a = h.devices.add_device();
    let b = h.devices.add_device();
    auth_context::with_context(admin_ctx(), h.service.grant_device(a))
        .await
        .expect("grant a");
    auth_context::with_context(admin_ctx(), h.service.grant_device(b))
        .await
        .expect("grant b");

    let grants = auth_context::with_context(admin_ctx(), h.service.list_grants())
        .await
        .expect("list");
    assert_eq!(grants.len(), 2);
}

// -- Device-keyed surface (`/me`, `/me/profile`) ---------------------------

#[tokio::test]
async fn device_grant_returns_only_this_devices_grant() {
    let h = harness();
    enable(&h).await;
    let a = h.devices.add_device();
    let b = h.devices.add_device();
    auth_context::with_context(admin_ctx(), h.service.grant_device(a))
        .await
        .expect("grant a");

    let for_a = auth_context::with_context(admin_ctx(), h.service.device_grant(a))
        .await
        .expect("device_grant a");
    assert_eq!(for_a.map(|g| g.device_id), Some(a));

    let for_b = auth_context::with_context(admin_ctx(), h.service.device_grant(b))
        .await
        .expect("device_grant b");
    assert!(for_b.is_none(), "ungranted device has no grant");
}

#[tokio::test]
async fn device_profile_is_none_when_disabled_or_ungranted() {
    let h = harness();
    let device_id = h.devices.add_device();

    // Feature disabled → no profile.
    let none = auth_context::with_context(admin_ctx(), h.service.device_profile(device_id))
        .await
        .expect("device_profile");
    assert!(none.is_none());

    // Enabled but this device isn't granted → still none.
    enable(&h).await;
    let none = auth_context::with_context(admin_ctx(), h.service.device_profile(device_id))
        .await
        .expect("device_profile");
    assert!(none.is_none());
}

#[tokio::test]
async fn device_profile_signs_when_granted_and_cert_present() {
    let h = harness();
    enable(&h).await;
    let device_id = h.devices.add_device();
    let grant = auth_context::with_context(admin_ctx(), h.service.grant_device(device_id))
        .await
        .expect("grant");

    // Seed a self-signed cert/key at the same secret-store keys the TLS layer
    // uses, so `device_profile` has material to sign with.
    let key_pair = rcgen::KeyPair::generate().unwrap();
    let cert = rcgen::CertificateParams::new(vec![format!("{}.{DOMAIN}", grant.token)])
        .unwrap()
        .self_signed(&key_pair)
        .unwrap();
    h.secrets
        .put(crate::tls::SECRET_CERT_CHAIN, cert.pem().as_bytes())
        .await
        .unwrap();
    h.secrets
        .put(
            crate::tls::SECRET_CERT_KEY,
            key_pair.serialize_pem().as_bytes(),
        )
        .await
        .unwrap();

    let profile = auth_context::with_context(admin_ctx(), h.service.device_profile(device_id))
        .await
        .expect("device_profile")
        .expect("a granted device with a cert gets a signed profile");
    // The signed profile embeds the plist verbatim, so the device's hostname
    // appears in the bytes.
    let hostname = format!("{}.{DOMAIN}", grant.token);
    assert!(
        profile
            .windows(hostname.len())
            .any(|w| w == hostname.as_bytes()),
        "signed profile must carry the device hostname"
    );
}

// -- Reconcile -------------------------------------------------------------

#[tokio::test]
async fn reconcile_persists_disable_when_entitlement_lost() {
    let h = harness();
    enable(&h).await;
    h.entitlement.set_premium(false);

    h.service.reconcile().await.expect("reconcile");

    assert_eq!(
        h.system_config.get("private_dns_enabled").await.unwrap(),
        Some("false".to_owned())
    );
}

#[tokio::test]
async fn reconcile_keeps_an_entitled_box_enabled() {
    let h = harness();
    enable(&h).await;

    h.service.reconcile().await.expect("reconcile");

    assert_eq!(
        h.system_config.get("private_dns_enabled").await.unwrap(),
        Some("true".to_owned())
    );
}

// -- Anonymous callers -----------------------------------------------------

#[tokio::test]
async fn anonymous_callers_are_rejected() {
    let h = harness();
    let err = h.service.status().await.expect_err("must require auth");
    assert!(matches!(
        err,
        AppError::Forbidden(_) | AppError::Unauthorized(_)
    ));
}

// -- Notify (issue #915) ---------------------------------------------------

#[tokio::test]
async fn notify_delegates_to_push_and_returns_delivered() {
    let h = harness_with_delivery(true);
    enable(&h).await;
    let device_id = h.devices.add_device();
    auth_context::with_context(admin_ctx(), h.service.grant_device(device_id))
        .await
        .expect("grant succeeds");

    let delivered = auth_context::with_context(admin_ctx(), h.service.notify_device(device_id))
        .await
        .expect("notify succeeds");

    assert!(delivered, "push reported a targeted subscription");
    assert_eq!(
        *h.push.notified.lock().unwrap(),
        vec![device_id],
        "the granted device UUID is handed to the push service"
    );
}

#[tokio::test]
async fn notify_reports_not_delivered_without_a_subscription() {
    let h = harness_with_delivery(false);
    enable(&h).await;
    let device_id = h.devices.add_device();
    auth_context::with_context(admin_ctx(), h.service.grant_device(device_id))
        .await
        .expect("grant succeeds");

    let delivered = auth_context::with_context(admin_ctx(), h.service.notify_device(device_id))
        .await
        .expect("notify succeeds even with no subscription");

    assert!(
        !delivered,
        "a granted device with no subscription is not an error"
    );
    assert_eq!(*h.push.notified.lock().unwrap(), vec![device_id]);
}

#[tokio::test]
async fn notify_without_a_grant_is_not_found_and_skips_push() {
    let h = harness_with_delivery(true);
    enable(&h).await;
    let device_id = h.devices.add_device();

    let err = auth_context::with_context(admin_ctx(), h.service.notify_device(device_id))
        .await
        .expect_err("an ungranted device must 404");

    assert!(matches!(err, AppError::NotFound(_)));
    assert!(
        h.push.notified.lock().unwrap().is_empty(),
        "push must not be called when there is no grant"
    );
}

#[tokio::test]
async fn notify_requires_admin() {
    let h = harness_with_delivery(true);
    let err = auth_context::with_context(
        AuthContext::Anonymous,
        h.service.notify_device(Uuid::new_v4()),
    )
    .await
    .expect_err("notify must require admin");
    assert!(matches!(
        err,
        AppError::Forbidden(_) | AppError::Unauthorized(_)
    ));
}

// -- Token encoding --------------------------------------------------------

#[test]
fn encode_base32_known_vectors() {
    assert_eq!(encode_base32(&[0u8; 10]), "aaaaaaaaaaaaaaaa");
    assert_eq!(encode_base32(&[0xFF; 10]), "7777777777777777");
    // RFC 4648 test vector "foobar" -> MZXW6YTBOI (lowercased, unpadded).
    assert_eq!(encode_base32(b"foobar"), "mzxw6ytboi");
}

#[test]
fn minted_tokens_are_unique_and_well_formed() {
    let a = mint_token();
    let b = mint_token();
    assert_eq!(a.len(), 16);
    assert_ne!(a, b, "two mints must not collide");
}
