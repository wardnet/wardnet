use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;

use wardnet_common::device::{Device, DeviceConnectionMode, DeviceType};

use crate::auth_context;
use crate::device::service::DeviceService;
use crate::error::AppError;
use crate::inbound_wg::interface::{
    InboundWgInterface, InboundWgPeerConfig, InboundWgPeerStats, InboundWgServerConfig,
};
use crate::inbound_wg::key_store::ServerKeyStore;
use crate::inbound_wg::service::{INBOUND_WG_INTERFACE, InboundWgService, InboundWgServiceImpl};
use crate::routing::firewall::{FirewallManager, ZoneIsolationRules, ZoneRules};
use wardnetd_data::repository::inbound_wg::{InboundWgPeerRepository, InboundWgPeerRow};
use wardnetd_data::repository::system_config::SystemConfigRepository;

// -- Mock DeviceService (only `get_device` is exercised) ------------------

#[derive(Default)]
struct MockDeviceService {
    devices: Mutex<HashMap<String, Device>>,
}

impl MockDeviceService {
    /// Register a device with the given display name and return its id.
    fn add_device(&self, name: &str) -> Uuid {
        let id = Uuid::new_v4();
        let device = Device {
            id,
            mac: format!("aa:bb:cc:00:00:{:02x}", self.devices.lock().unwrap().len()),
            name: Some(name.to_owned()),
            hostname: None,
            manufacturer: None,
            device_type: DeviceType::Unknown,
            first_seen: chrono::Utc::now(),
            last_seen: chrono::Utc::now(),
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

    async fn clear_remote_connection_mode(&self, device_id: &str) -> Result<(), AppError> {
        if let Some(device) = self.devices.lock().unwrap().get_mut(device_id)
            && device.connection_mode == DeviceConnectionMode::Remote
        {
            device.connection_mode = DeviceConnectionMode::Lan;
        }
        Ok(())
    }

    async fn get_device_for_ip(
        &self,
        _ip: &str,
    ) -> Result<wardnet_common::api::DeviceMeResponse, AppError> {
        unimplemented!("not exercised by inbound-wg tests")
    }
    async fn set_rule_for_ip(
        &self,
        _ip: &str,
        _target: wardnet_common::routing::RoutingTarget,
    ) -> Result<wardnet_common::api::SetMyRuleResponse, AppError> {
        unimplemented!("not exercised by inbound-wg tests")
    }
    async fn set_rule(
        &self,
        _device_id: &str,
        _target: wardnet_common::routing::RoutingTarget,
    ) -> Result<(), AppError> {
        unimplemented!("not exercised by inbound-wg tests")
    }
    async fn current_rules(
        &self,
    ) -> Result<HashMap<Uuid, wardnet_common::routing::RoutingTarget>, AppError> {
        unimplemented!("not exercised by inbound-wg tests")
    }
    async fn update_admin_locked(&self, _device_id: &str, _locked: bool) -> Result<(), AppError> {
        unimplemented!("not exercised by inbound-wg tests")
    }
    async fn get_dns_capture_settings(
        &self,
        _device_id: &str,
    ) -> Result<wardnet_common::api::DnsCaptureSettingsResponse, AppError> {
        unimplemented!("not exercised by inbound-wg tests")
    }
    async fn update_dns_capture_settings(
        &self,
        _device_id: &str,
        _enabled: Option<bool>,
        _cap_count: Option<i64>,
        _cap_days: Option<i64>,
    ) -> Result<(), AppError> {
        unimplemented!("not exercised by inbound-wg tests")
    }
    async fn set_my_capture_enabled(
        &self,
        _ip: &str,
        _enabled: bool,
    ) -> Result<wardnet_common::api::DnsCaptureSettingsResponse, AppError> {
        unimplemented!("not exercised by inbound-wg tests")
    }
    async fn fetch_pending_dns_events(
        &self,
        _device_id: &str,
        _after_id: i64,
        _limit: i64,
    ) -> Result<Vec<wardnet_common::api::DnsEventItem>, AppError> {
        unimplemented!("not exercised by inbound-wg tests")
    }
    async fn mark_dns_events_synced(
        &self,
        _device_id: &str,
        _up_to_id: i64,
    ) -> Result<(), AppError> {
        unimplemented!("not exercised by inbound-wg tests")
    }
    async fn ack_dns_events(&self, _device_id: &str, _up_to_id: i64) -> Result<(), AppError> {
        unimplemented!("not exercised by inbound-wg tests")
    }
    async fn list_capture_enabled_device_ids(&self) -> Result<Vec<String>, AppError> {
        unimplemented!("not exercised by inbound-wg tests")
    }
    async fn get_device_capture_settings(
        &self,
        _device_id: &str,
    ) -> Result<Option<(bool, i64, i64)>, AppError> {
        unimplemented!("not exercised by inbound-wg tests")
    }
}

fn admin_ctx() -> AuthContext {
    AuthContext::Admin {
        admin_id: Uuid::new_v4(),
    }
}

// -- Mock InboundWgPeerRepository -----------------------------------------

#[derive(Default)]
struct MockPeerRepo {
    rows: Mutex<Vec<InboundWgPeerRow>>,
}

#[async_trait]
impl InboundWgPeerRepository for MockPeerRepo {
    async fn insert(&self, row: &InboundWgPeerRow) -> anyhow::Result<()> {
        self.rows.lock().unwrap().push(row.clone());
        Ok(())
    }
    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<InboundWgPeerRow>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.id == id)
            .cloned())
    }
    async fn find_by_device_id(&self, device_id: &str) -> anyhow::Result<Option<InboundWgPeerRow>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .find(|r| r.device_id.as_deref() == Some(device_id))
            .cloned())
    }
    async fn find_all(&self) -> anyhow::Result<Vec<InboundWgPeerRow>> {
        Ok(self.rows.lock().unwrap().clone())
    }
    async fn find_enabled(&self) -> anyhow::Result<Vec<InboundWgPeerRow>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.enabled)
            .cloned()
            .collect())
    }
    async fn delete(&self, id: &str) -> anyhow::Result<()> {
        self.rows.lock().unwrap().retain(|r| r.id != id);
        Ok(())
    }
    async fn set_enabled(&self, id: &str, enabled: bool) -> anyhow::Result<()> {
        if let Some(row) = self.rows.lock().unwrap().iter_mut().find(|r| r.id == id) {
            row.enabled = enabled;
        }
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

// -- Mock ServerKeyStore --------------------------------------------------

#[derive(Default)]
struct MockKeyStore {
    key: Mutex<Option<String>>,
}

#[async_trait]
impl ServerKeyStore for MockKeyStore {
    async fn save_key(&self, private_key: &str) -> anyhow::Result<()> {
        *self.key.lock().unwrap() = Some(private_key.to_owned());
        Ok(())
    }
    async fn load_key(&self) -> anyhow::Result<Option<String>> {
        Ok(self.key.lock().unwrap().clone())
    }
    async fn delete_key(&self) -> anyhow::Result<()> {
        *self.key.lock().unwrap() = None;
        Ok(())
    }
}

// -- Mock InboundWgInterface (records calls) ------------------------------

#[derive(Default)]
struct MockInterface {
    ensure_calls: Mutex<Vec<InboundWgServerConfig>>,
    added_peers: Mutex<Vec<InboundWgPeerConfig>>,
    removed_peers: Mutex<Vec<[u8; 32]>>,
    tear_downs: Mutex<Vec<String>>,
    /// When set, `add_peer` returns an error instead of recording the peer —
    /// used to exercise the service's row-rollback compensating action.
    fail_add_peer: AtomicBool,
}

#[async_trait]
impl InboundWgInterface for MockInterface {
    async fn ensure_server(&self, config: InboundWgServerConfig) -> anyhow::Result<()> {
        self.ensure_calls.lock().unwrap().push(config);
        Ok(())
    }
    async fn tear_down_server(&self, interface_name: &str) -> anyhow::Result<()> {
        self.tear_downs
            .lock()
            .unwrap()
            .push(interface_name.to_owned());
        Ok(())
    }
    async fn add_peer(
        &self,
        _interface_name: &str,
        peer: InboundWgPeerConfig,
    ) -> anyhow::Result<()> {
        if self.fail_add_peer.load(Ordering::SeqCst) {
            anyhow::bail!("mock interface: add_peer failure");
        }
        self.added_peers.lock().unwrap().push(peer);
        Ok(())
    }
    async fn remove_peer(&self, _interface_name: &str, public_key: [u8; 32]) -> anyhow::Result<()> {
        self.removed_peers.lock().unwrap().push(public_key);
        Ok(())
    }
    async fn peer_stats(&self, _interface_name: &str) -> anyhow::Result<Vec<InboundWgPeerStats>> {
        Ok(Vec::new())
    }
}

// -- Mock FirewallManager (records the calls we care about) ---------------

#[derive(Default)]
struct MockFirewall {
    masquerade_added: Mutex<Vec<String>>,
    masquerade_removed: Mutex<Vec<String>>,
    accept_added: Mutex<Vec<u16>>,
    accept_removed: Mutex<u32>,
}

#[async_trait]
impl FirewallManager for MockFirewall {
    async fn init_wardnet_table(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn flush_wardnet_table(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn add_masquerade(&self, interface: &str) -> anyhow::Result<()> {
        self.masquerade_added
            .lock()
            .unwrap()
            .push(interface.to_owned());
        Ok(())
    }
    async fn remove_masquerade(&self, interface: &str) -> anyhow::Result<()> {
        self.masquerade_removed
            .lock()
            .unwrap()
            .push(interface.to_owned());
        Ok(())
    }
    async fn add_inbound_wg_accept(&self, port: u16) -> anyhow::Result<()> {
        self.accept_added.lock().unwrap().push(port);
        Ok(())
    }
    async fn remove_inbound_wg_accept(&self) -> anyhow::Result<()> {
        *self.accept_removed.lock().unwrap() += 1;
        Ok(())
    }
    async fn cleanup_legacy_dns_redirects(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn add_tcp_reset_reject(&self, _device_ip: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn remove_tcp_reset_reject(&self, _device_ip: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn apply_zone_rules(
        &self,
        _device_ip: &str,
        _rules: ZoneRules,
        _lan_interface: &str,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn remove_zone_rules(&self, _device_ip: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn list_zone_rule_ips(&self) -> anyhow::Result<Vec<String>> {
        Ok(Vec::new())
    }
    async fn apply_zone_isolation(&self, _rules: ZoneIsolationRules) -> anyhow::Result<()> {
        Ok(())
    }
    async fn check_tools_available(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn destroy_wardnet_table(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

// -- Harness --------------------------------------------------------------

struct Harness {
    peers: Arc<MockPeerRepo>,
    system_config: Arc<MockSystemConfig>,
    keys: Arc<MockKeyStore>,
    interface: Arc<MockInterface>,
    firewall: Arc<MockFirewall>,
    devices: Arc<MockDeviceService>,
    service: InboundWgServiceImpl,
}

impl Harness {
    /// Register a device and return its id, for use as `add_peer`'s argument.
    fn add_device(&self, name: &str) -> Uuid {
        self.devices.add_device(name)
    }
}

fn harness() -> Harness {
    let peers = Arc::new(MockPeerRepo::default());
    let system_config = Arc::new(MockSystemConfig::default());
    let keys = Arc::new(MockKeyStore::default());
    let interface = Arc::new(MockInterface::default());
    let firewall = Arc::new(MockFirewall::default());
    let devices = Arc::new(MockDeviceService::default());
    let service = InboundWgServiceImpl::with_key_store(
        peers.clone(),
        system_config.clone(),
        keys.clone(),
        interface.clone(),
        firewall.clone(),
        devices.clone(),
    );
    Harness {
        peers,
        system_config,
        keys,
        interface,
        firewall,
        devices,
        service,
    }
}

#[tokio::test]
async fn enable_persists_config_and_calls_backends() {
    let h = harness();
    let resp = auth_context::with_context(admin_ctx(), h.service.set_config(true, 51821))
        .await
        .expect("enable succeeds");

    assert!(resp.enabled);
    assert_eq!(resp.listen_port, 51821);
    assert!(
        resp.server_public_key.is_some(),
        "pubkey generated on enable"
    );

    // Config persisted.
    assert!(h.system_config.inbound_wg_enabled().await.unwrap());
    assert_eq!(
        h.system_config.inbound_wg_listen_port().await.unwrap(),
        51821
    );
    assert!(
        h.system_config
            .inbound_wg_server_pubkey()
            .await
            .unwrap()
            .is_some()
    );
    // Server keypair persisted in the key store.
    assert!(h.keys.load_key().await.unwrap().is_some());

    // Interface stood up with the right name + port.
    let ensures = h.interface.ensure_calls.lock().unwrap();
    assert_eq!(ensures.len(), 1);
    assert_eq!(ensures[0].interface_name, INBOUND_WG_INTERFACE);
    assert_eq!(ensures[0].listen_port, 51821);

    // Firewall rules installed on the right interface/port.
    assert_eq!(
        *h.firewall.masquerade_added.lock().unwrap(),
        vec![INBOUND_WG_INTERFACE.to_owned()]
    );
    assert_eq!(*h.firewall.accept_added.lock().unwrap(), vec![51821]);
}

#[tokio::test]
async fn get_config_reads_without_mutating() {
    let h = harness();
    let before = auth_context::with_context(admin_ctx(), h.service.get_config())
        .await
        .unwrap();
    assert!(!before.enabled);
    assert!(before.server_public_key.is_none());
    // A read must not stand up the interface or touch the firewall.
    assert!(h.interface.ensure_calls.lock().unwrap().is_empty());

    auth_context::with_context(admin_ctx(), h.service.set_config(true, 51821))
        .await
        .unwrap();
    let after = auth_context::with_context(admin_ctx(), h.service.get_config())
        .await
        .unwrap();
    assert!(after.enabled);
    assert_eq!(after.listen_port, 51821);
    assert!(after.server_public_key.is_some());
}

#[tokio::test]
async fn add_peer_rejected_when_disabled() {
    let h = harness();
    let device_id = h.add_device("laptop");
    let result = auth_context::with_context(admin_ctx(), h.service.add_peer(device_id)).await;
    assert!(result.is_err(), "add_peer must fail when server disabled");
    // Nothing was persisted or pushed to the interface.
    assert!(h.peers.find_all().await.unwrap().is_empty());
    assert!(h.interface.added_peers.lock().unwrap().is_empty());
}

#[tokio::test]
async fn add_peer_generates_private_key_never_persisted() {
    let h = harness();
    auth_context::with_context(admin_ctx(), h.service.set_config(true, 51821))
        .await
        .unwrap();

    let device_id = h.add_device("phone");
    let resp = auth_context::with_context(admin_ctx(), h.service.add_peer(device_id))
        .await
        .expect("add_peer succeeds when enabled");

    assert_eq!(resp.name, "phone");
    assert!(!resp.private_key.is_empty());
    assert_ne!(resp.private_key, resp.public_key);
    assert_eq!(resp.allowed_ip, "10.100.64.2/32", "first peer gets .2");

    // The row stores only the public key + allocated IP — never the private key.
    let rows = h.peers.find_all().await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].public_key, resp.public_key);
    assert_eq!(rows[0].allowed_ip, resp.allowed_ip);
    let serialized = format!("{:?}", rows[0]);
    assert!(
        !serialized.contains(&resp.private_key),
        "private key must not be persisted anywhere on the row"
    );

    // The peer was pushed to the interface.
    assert_eq!(h.interface.added_peers.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn add_peer_rolls_back_row_when_interface_fails() {
    let h = harness();
    auth_context::with_context(admin_ctx(), h.service.set_config(true, 51821))
        .await
        .unwrap();

    // Force the interface to reject the peer after the row is inserted.
    h.interface.fail_add_peer.store(true, Ordering::SeqCst);

    let device_id = h.add_device("laptop");
    let result = auth_context::with_context(admin_ctx(), h.service.add_peer(device_id)).await;
    assert!(
        result.is_err(),
        "add_peer must fail when the interface rejects the peer"
    );

    // Compensating action: the just-inserted row must not survive a failed
    // interface add — no orphaned IP allocation or public key left behind.
    assert!(
        h.peers.find_all().await.unwrap().is_empty(),
        "a failed interface add_peer must leave no orphaned peer row"
    );
    // And nothing was recorded as added on the interface either.
    assert!(h.interface.added_peers.lock().unwrap().is_empty());
}

#[tokio::test]
async fn second_peer_gets_next_free_ip() {
    let h = harness();
    auth_context::with_context(admin_ctx(), h.service.set_config(true, 51821))
        .await
        .unwrap();
    let id_a = h.add_device("a");
    let id_b = h.add_device("b");
    let a = auth_context::with_context(admin_ctx(), h.service.add_peer(id_a))
        .await
        .unwrap();
    let b = auth_context::with_context(admin_ctx(), h.service.add_peer(id_b))
        .await
        .unwrap();
    assert_eq!(a.allowed_ip, "10.100.64.2/32");
    assert_eq!(b.allowed_ip, "10.100.64.3/32");
}

#[tokio::test]
async fn remove_peer_deletes_row() {
    let h = harness();
    auth_context::with_context(admin_ctx(), h.service.set_config(true, 51821))
        .await
        .unwrap();
    let device_id = h.add_device("x");
    let peer = auth_context::with_context(admin_ctx(), h.service.add_peer(device_id))
        .await
        .unwrap();
    assert_eq!(h.peers.find_all().await.unwrap().len(), 1);

    auth_context::with_context(admin_ctx(), h.service.remove_peer(peer.id))
        .await
        .expect("remove succeeds");
    assert!(h.peers.find_all().await.unwrap().is_empty());
    assert_eq!(h.interface.removed_peers.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn disable_does_not_delete_peer_rows() {
    let h = harness();
    auth_context::with_context(admin_ctx(), h.service.set_config(true, 51821))
        .await
        .unwrap();
    let device_id = h.add_device("keep");
    auth_context::with_context(admin_ctx(), h.service.add_peer(device_id))
        .await
        .unwrap();

    auth_context::with_context(admin_ctx(), h.service.set_config(false, 51821))
        .await
        .expect("disable succeeds");

    // Peer rows survive a disable.
    assert_eq!(h.peers.find_all().await.unwrap().len(), 1);
    assert!(!h.system_config.inbound_wg_enabled().await.unwrap());
    // Firewall + interface torn down.
    assert_eq!(
        *h.firewall.masquerade_removed.lock().unwrap(),
        vec![INBOUND_WG_INTERFACE.to_owned()]
    );
    assert_eq!(*h.firewall.accept_removed.lock().unwrap(), 1);
    assert_eq!(
        *h.interface.tear_downs.lock().unwrap(),
        vec![INBOUND_WG_INTERFACE.to_owned()]
    );
}

#[tokio::test]
async fn set_peer_enabled_disable_removes_from_interface_but_keeps_row() {
    let h = harness();
    auth_context::with_context(admin_ctx(), h.service.set_config(true, 51821))
        .await
        .unwrap();
    let device_id = h.add_device("laptop");
    let peer = auth_context::with_context(admin_ctx(), h.service.add_peer(device_id))
        .await
        .unwrap();
    assert_eq!(h.interface.added_peers.lock().unwrap().len(), 1);

    let summary =
        auth_context::with_context(admin_ctx(), h.service.set_peer_enabled(peer.id, false))
            .await
            .expect("disable succeeds");

    assert!(!summary.enabled);
    // Row survives — distinct from `remove_peer`.
    assert_eq!(h.peers.find_all().await.unwrap().len(), 1);
    assert!(
        !h.peers
            .find_by_id(&peer.id.to_string())
            .await
            .unwrap()
            .unwrap()
            .enabled
    );
    // Best-effort removed from the live interface.
    assert_eq!(h.interface.removed_peers.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn set_peer_enabled_reenable_readmits_same_credential() {
    let h = harness();
    auth_context::with_context(admin_ctx(), h.service.set_config(true, 51821))
        .await
        .unwrap();
    let device_id = h.add_device("laptop");
    let peer = auth_context::with_context(admin_ctx(), h.service.add_peer(device_id))
        .await
        .unwrap();
    auth_context::with_context(admin_ctx(), h.service.set_peer_enabled(peer.id, false))
        .await
        .unwrap();

    let summary =
        auth_context::with_context(admin_ctx(), h.service.set_peer_enabled(peer.id, true))
            .await
            .expect("re-enable succeeds");

    assert!(summary.enabled);
    assert_eq!(
        summary.public_key, peer.public_key,
        "same credential, no new keypair"
    );
    // add_peer once (initial grant) + re-admit once on re-enable.
    assert_eq!(h.interface.added_peers.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn set_peer_enabled_not_found_returns_404() {
    let h = harness();
    let result = auth_context::with_context(
        admin_ctx(),
        h.service.set_peer_enabled(Uuid::new_v4(), false),
    )
    .await;
    assert!(result.is_err(), "unknown peer id must 404");
}

#[tokio::test]
async fn set_peer_enabled_noop_when_already_in_requested_state() {
    let h = harness();
    auth_context::with_context(admin_ctx(), h.service.set_config(true, 51821))
        .await
        .unwrap();
    let device_id = h.add_device("laptop");
    let peer = auth_context::with_context(admin_ctx(), h.service.add_peer(device_id))
        .await
        .unwrap();

    auth_context::with_context(admin_ctx(), h.service.set_peer_enabled(peer.id, true))
        .await
        .expect("no-op enable succeeds");

    // No extra interface calls beyond the initial grant.
    assert_eq!(h.interface.added_peers.lock().unwrap().len(), 1);
    assert!(h.interface.removed_peers.lock().unwrap().is_empty());
}

#[tokio::test]
async fn requires_admin() {
    let h = harness();
    // No auth context — must be rejected.
    let result = h.service.list_peers().await;
    assert!(result.is_err(), "list_peers requires admin");
}

#[tokio::test]
async fn reconcile_reenables_peers_when_enabled() {
    let h = harness();
    auth_context::with_context(admin_ctx(), h.service.set_config(true, 51821))
        .await
        .unwrap();
    let device_id = h.add_device("p");
    auth_context::with_context(admin_ctx(), h.service.add_peer(device_id))
        .await
        .unwrap();

    // Clear the recorded interface calls, then reconcile.
    h.interface.ensure_calls.lock().unwrap().clear();
    h.interface.added_peers.lock().unwrap().clear();

    h.service.reconcile().await.expect("reconcile succeeds");

    assert_eq!(h.interface.ensure_calls.lock().unwrap().len(), 1);
    assert_eq!(
        h.interface.added_peers.lock().unwrap().len(),
        1,
        "enabled peer re-added on reconcile"
    );
}
