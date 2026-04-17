use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::broadcast;
use uuid::Uuid;
use wardnet_common::api::CreateTunnelRequest;
use wardnet_common::event::WardnetEvent;
use wardnet_common::tunnel::{Tunnel, TunnelConfig, TunnelStatus};
use wardnet_common::wireguard_config::WgPeerConfig;

use wardnet_common::auth::AuthContext;

use wardnet_common::device::Device;
use wardnet_common::routing::RoutingRule;

use crate::auth_context;
use crate::error::AppError;
use crate::event::EventPublisher;
use crate::tunnel::interface::{CreateTunnelParams, TunnelInterface, TunnelStats};
use crate::{TunnelService, TunnelServiceImpl};
use wardnetd_data::keys::KeyStore;
use wardnetd_data::repository::tunnel::TunnelRow;
use wardnetd_data::repository::{DeviceRepository, TunnelRepository};

/// Helper to create an admin auth context for tests.
fn admin_ctx() -> AuthContext {
    AuthContext::Admin {
        admin_id: Uuid::new_v4(),
    }
}

// -- Sample WireGuard config text -----------------------------------------

const SAMPLE_CONF: &str = "\
[Interface]
PrivateKey = YNqHbfBQKaGvzefSSbufuZKjTIHQadqIyERi1V562lY=
Address = 10.66.0.2/32
DNS = 1.1.1.1

[Peer]
PublicKey = Uf0bMmMFBJbOQtYp3iByaIT5jlQDGHUBk4bH8WDAiUk=
Endpoint = 198.51.100.1:51820
AllowedIPs = 0.0.0.0/0
PersistentKeepalive = 25
";

// -- Mock TunnelRepository ------------------------------------------------

struct MockTunnelRepo {
    rows: Mutex<Vec<TunnelRow>>,
}

impl MockTunnelRepo {
    fn new() -> Self {
        Self {
            rows: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl TunnelRepository for MockTunnelRepo {
    async fn find_all(&self) -> anyhow::Result<Vec<Tunnel>> {
        let rows = self.rows.lock().unwrap();
        rows.iter().map(row_to_tunnel).collect()
    }

    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<Tunnel>> {
        let rows = self.rows.lock().unwrap();
        let row = rows.iter().find(|r| r.id == id);
        row.map(row_to_tunnel).transpose()
    }

    async fn find_config_by_id(&self, id: &str) -> anyhow::Result<Option<TunnelConfig>> {
        let rows = self.rows.lock().unwrap();
        let row = rows.iter().find(|r| r.id == id);
        match row {
            Some(r) => {
                let address: Vec<String> = serde_json::from_str(&r.address)?;
                let dns: Vec<String> = serde_json::from_str(&r.dns)?;
                let peer: WgPeerConfig = serde_json::from_str(&r.peer_config)?;
                let listen_port = r.listen_port;
                Ok(Some(TunnelConfig {
                    address,
                    dns,
                    listen_port,
                    peer,
                }))
            }
            None => Ok(None),
        }
    }

    async fn insert(&self, row: &TunnelRow) -> anyhow::Result<()> {
        self.rows.lock().unwrap().push(row.clone());
        Ok(())
    }

    async fn update_status(&self, id: &str, status: &str) -> anyhow::Result<()> {
        let mut rows = self.rows.lock().unwrap();
        if let Some(r) = rows.iter_mut().find(|r| r.id == id) {
            r.status = status.to_owned();
        }
        Ok(())
    }

    async fn update_stats(
        &self,
        _id: &str,
        _bytes_tx: i64,
        _bytes_rx: i64,
        _last_handshake: Option<&str>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn delete(&self, id: &str) -> anyhow::Result<()> {
        self.rows.lock().unwrap().retain(|r| r.id != id);
        Ok(())
    }

    async fn next_interface_index(&self) -> anyhow::Result<i64> {
        let rows = self.rows.lock().unwrap();
        let max = rows
            .iter()
            .filter_map(|r| {
                r.interface_name
                    .strip_prefix("wg_ward")
                    .and_then(|s| s.parse::<i64>().ok())
            })
            .max();
        Ok(max.map_or(0, |m| m + 1))
    }

    async fn count(&self) -> anyhow::Result<i64> {
        Ok(i64::try_from(self.rows.lock().unwrap().len()).unwrap())
    }

    async fn count_active(&self) -> anyhow::Result<i64> {
        Ok(0)
    }
}

/// Convert a `TunnelRow` into a `Tunnel` for mock responses.
fn row_to_tunnel(r: &TunnelRow) -> anyhow::Result<Tunnel> {
    let status = match r.status.as_str() {
        "up" => TunnelStatus::Up,
        "connecting" => TunnelStatus::Connecting,
        _ => TunnelStatus::Down,
    };
    Ok(Tunnel {
        id: r.id.parse()?,
        label: r.label.clone(),
        country_code: r.country_code.clone(),
        provider: r.provider.clone(),
        interface_name: r.interface_name.clone(),
        endpoint: r.endpoint.clone(),
        status,
        last_handshake: None,
        bytes_tx: 0,
        bytes_rx: 0,
        created_at: chrono::Utc::now(),
    })
}

// -- Mock KeyStore --------------------------------------------------------

struct MockKeyStore {
    keys: Mutex<Vec<(Uuid, String)>>,
}

impl MockKeyStore {
    fn new() -> Self {
        Self {
            keys: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl KeyStore for MockKeyStore {
    async fn save_key(&self, tunnel_id: &Uuid, private_key: &str) -> anyhow::Result<()> {
        self.keys
            .lock()
            .unwrap()
            .push((*tunnel_id, private_key.to_owned()));
        Ok(())
    }

    async fn load_key(&self, tunnel_id: &Uuid) -> anyhow::Result<String> {
        let keys = self.keys.lock().unwrap();
        keys.iter()
            .find(|(id, _)| id == tunnel_id)
            .map(|(_, k)| k.clone())
            .ok_or_else(|| anyhow::anyhow!("key not found for tunnel {tunnel_id}"))
    }

    async fn delete_key(&self, tunnel_id: &Uuid) -> anyhow::Result<()> {
        self.keys.lock().unwrap().retain(|(id, _)| id != tunnel_id);
        Ok(())
    }
}

// -- Mock TunnelInterface -------------------------------------------------

/// Programmable outcome for `get_stats` calls, used by the stats/health tests.
enum StatsBehavior {
    /// Default: return `Ok(None)` (interface not found).
    None,
    /// Return `Ok(Some(stats))` where stats is cloned from the inner value.
    Some(TunnelStats),
    /// Return `Err(...)`.
    Err,
}

/// Records calls to tunnel interface operations for assertion.
struct MockTunnelInterface {
    created: Mutex<Vec<String>>,
    brought_up: Mutex<Vec<String>>,
    torn_down: Mutex<Vec<String>>,
    removed: Mutex<Vec<String>>,
    stats_behavior: Mutex<StatsBehavior>,
}

impl MockTunnelInterface {
    fn new() -> Self {
        Self {
            created: Mutex::new(Vec::new()),
            brought_up: Mutex::new(Vec::new()),
            torn_down: Mutex::new(Vec::new()),
            removed: Mutex::new(Vec::new()),
            stats_behavior: Mutex::new(StatsBehavior::None),
        }
    }

    fn set_stats(&self, stats: TunnelStats) {
        *self.stats_behavior.lock().unwrap() = StatsBehavior::Some(stats);
    }

    fn set_stats_error(&self) {
        *self.stats_behavior.lock().unwrap() = StatsBehavior::Err;
    }
}

#[async_trait]
impl TunnelInterface for MockTunnelInterface {
    async fn create(&self, params: CreateTunnelParams) -> anyhow::Result<()> {
        self.created.lock().unwrap().push(params.interface_name);
        Ok(())
    }

    async fn bring_up(&self, interface_name: &str) -> anyhow::Result<()> {
        self.brought_up
            .lock()
            .unwrap()
            .push(interface_name.to_owned());
        Ok(())
    }

    async fn tear_down(&self, interface_name: &str) -> anyhow::Result<()> {
        self.torn_down
            .lock()
            .unwrap()
            .push(interface_name.to_owned());
        Ok(())
    }

    async fn remove(&self, interface_name: &str) -> anyhow::Result<()> {
        self.removed.lock().unwrap().push(interface_name.to_owned());
        Ok(())
    }

    async fn get_stats(&self, _interface_name: &str) -> anyhow::Result<Option<TunnelStats>> {
        match &*self.stats_behavior.lock().unwrap() {
            StatsBehavior::None => Ok(None),
            StatsBehavior::Some(s) => Ok(Some(s.clone())),
            StatsBehavior::Err => Err(anyhow::anyhow!("stats unavailable")),
        }
    }

    async fn list(&self) -> anyhow::Result<Vec<String>> {
        Ok(Vec::new())
    }
}

// -- Mock EventPublisher --------------------------------------------------

/// Records published events for assertion.
struct MockEventPublisher {
    events: Mutex<Vec<WardnetEvent>>,
}

impl MockEventPublisher {
    fn new() -> Self {
        Self {
            events: Mutex::new(Vec::new()),
        }
    }

    fn published_events(&self) -> Vec<WardnetEvent> {
        self.events.lock().unwrap().clone()
    }
}

impl EventPublisher for MockEventPublisher {
    fn publish(&self, event: WardnetEvent) {
        self.events.lock().unwrap().push(event);
    }

    fn subscribe(&self) -> broadcast::Receiver<WardnetEvent> {
        let (_, rx) = broadcast::channel(16);
        rx
    }
}

// -- Minimal DeviceRepository mock for TunnelService tests ----------------

struct MockDeviceRepoForTunnel;

/// Device repo mock that returns device IDs when a tunnel's rules are switched.
struct MockDeviceRepoWithSwitchedDevices {
    switched_device_ids: Vec<String>,
}

impl MockDeviceRepoWithSwitchedDevices {
    fn new(device_ids: Vec<String>) -> Self {
        Self {
            switched_device_ids: device_ids,
        }
    }
}

#[async_trait]
impl DeviceRepository for MockDeviceRepoForTunnel {
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
    async fn insert(
        &self,
        _d: &wardnetd_data::repository::device::DeviceRow,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_last_seen_and_ip(&self, _id: &str, _ip: &str, _ts: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_last_seen_batch(&self, _updates: &[(String, String)]) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_hostname(&self, _id: &str, _h: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_name_and_type(
        &self,
        _id: &str,
        _name: Option<&str>,
        _t: &str,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn find_stale(&self, _before: &str) -> anyhow::Result<Vec<Device>> {
        Ok(vec![])
    }
    async fn find_rule_for_device(&self, _id: &str) -> anyhow::Result<Option<RoutingRule>> {
        Ok(None)
    }
    async fn upsert_user_rule(&self, _id: &str, _json: &str, _now: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn switch_tunnel_rules_to_direct(
        &self,
        _tid: &str,
        _now: &str,
    ) -> anyhow::Result<Vec<String>> {
        Ok(vec![])
    }
    async fn update_admin_locked(&self, _id: &str, _locked: bool) -> anyhow::Result<()> {
        Ok(())
    }
    async fn count(&self) -> anyhow::Result<i64> {
        Ok(0)
    }
}

#[async_trait]
impl DeviceRepository for MockDeviceRepoWithSwitchedDevices {
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
    async fn insert(
        &self,
        _d: &wardnetd_data::repository::device::DeviceRow,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_last_seen_and_ip(&self, _id: &str, _ip: &str, _ts: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_last_seen_batch(&self, _updates: &[(String, String)]) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_hostname(&self, _id: &str, _h: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_name_and_type(
        &self,
        _id: &str,
        _name: Option<&str>,
        _t: &str,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn find_stale(&self, _before: &str) -> anyhow::Result<Vec<Device>> {
        Ok(vec![])
    }
    async fn find_rule_for_device(&self, _id: &str) -> anyhow::Result<Option<RoutingRule>> {
        Ok(None)
    }
    async fn upsert_user_rule(&self, _id: &str, _json: &str, _now: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn switch_tunnel_rules_to_direct(
        &self,
        _tid: &str,
        _now: &str,
    ) -> anyhow::Result<Vec<String>> {
        Ok(self.switched_device_ids.clone())
    }
    async fn update_admin_locked(&self, _id: &str, _locked: bool) -> anyhow::Result<()> {
        Ok(())
    }
    async fn count(&self) -> anyhow::Result<i64> {
        Ok(0)
    }
}

// -- Helpers --------------------------------------------------------------

struct TestHarness {
    svc: TunnelServiceImpl,
    tunnel_iface: Arc<MockTunnelInterface>,
    events: Arc<MockEventPublisher>,
    keys: Arc<MockKeyStore>,
}

fn build_harness() -> TestHarness {
    let repo = Arc::new(MockTunnelRepo::new());
    let device_repo: Arc<dyn DeviceRepository> = Arc::new(MockDeviceRepoForTunnel);
    let tunnel_iface = Arc::new(MockTunnelInterface::new());
    let keys = Arc::new(MockKeyStore::new());
    let events = Arc::new(MockEventPublisher::new());

    let svc = TunnelServiceImpl::new(
        repo,
        device_repo,
        tunnel_iface.clone(),
        keys.clone(),
        events.clone(),
    );

    TestHarness {
        svc,
        tunnel_iface,
        events,
        keys,
    }
}

fn build_harness_with_device_repo(device_repo: Arc<dyn DeviceRepository>) -> TestHarness {
    let repo = Arc::new(MockTunnelRepo::new());
    let tunnel_iface = Arc::new(MockTunnelInterface::new());
    let keys = Arc::new(MockKeyStore::new());
    let events = Arc::new(MockEventPublisher::new());

    let svc = TunnelServiceImpl::new(
        repo,
        device_repo,
        tunnel_iface.clone(),
        keys.clone(),
        events.clone(),
    );

    TestHarness {
        svc,
        tunnel_iface,
        events,
        keys,
    }
}

fn sample_request() -> CreateTunnelRequest {
    CreateTunnelRequest {
        label: "Sweden VPN".to_owned(),
        country_code: "SE".to_owned(),
        provider: Some("Mullvad".to_owned()),
        config: SAMPLE_CONF.to_owned(),
    }
}

// -- Tests ----------------------------------------------------------------

#[tokio::test]
async fn import_tunnel_success() {
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();

    assert_eq!(resp.tunnel.label, "Sweden VPN");
    assert_eq!(resp.tunnel.country_code, "SE");
    assert_eq!(resp.tunnel.provider, Some("Mullvad".to_owned()));
    assert_eq!(resp.tunnel.interface_name, "wg_ward0");
    assert_eq!(resp.tunnel.endpoint, "198.51.100.1:51820");
    assert_eq!(resp.tunnel.status, TunnelStatus::Down);
    assert_eq!(resp.tunnel.bytes_tx, 0);
    assert_eq!(resp.tunnel.bytes_rx, 0);
    assert_eq!(resp.message, "tunnel imported successfully");
}

#[tokio::test]
async fn import_tunnel_anonymous_forbidden() {
    let h = build_harness();
    let result = auth_context::with_context(
        AuthContext::Anonymous,
        h.svc.import_tunnel(sample_request()),
    )
    .await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn import_tunnel_invalid_config() {
    let h = build_harness();
    let req = CreateTunnelRequest {
        label: "Bad".to_owned(),
        country_code: "XX".to_owned(),
        provider: None,
        config: "this is not a valid config".to_owned(),
    };

    let result = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(req)).await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), AppError::BadRequest(_)));
}

#[tokio::test]
async fn bring_up_success() {
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    auth_context::with_context(admin_ctx(), h.svc.bring_up(id))
        .await
        .unwrap();

    // Verify WireGuard ops were called.
    {
        let created = h.tunnel_iface.created.lock().unwrap();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0], "wg_ward0");
    }
    {
        let brought_up = h.tunnel_iface.brought_up.lock().unwrap();
        assert_eq!(brought_up.len(), 1);
        assert_eq!(brought_up[0], "wg_ward0");
    }

    // Verify TunnelUp event was published.
    let events = h.events.published_events();
    assert_eq!(events.len(), 1);
    assert!(matches!(&events[0], WardnetEvent::TunnelUp { tunnel_id, .. } if *tunnel_id == id));

    // Verify tunnel status is now up.
    let tunnel = auth_context::with_context(admin_ctx(), h.svc.get_tunnel(id))
        .await
        .unwrap();
    assert_eq!(tunnel.status, TunnelStatus::Up);
}

#[tokio::test]
async fn bring_up_anonymous_forbidden() {
    let h = build_harness();
    let result =
        auth_context::with_context(AuthContext::Anonymous, h.svc.bring_up(Uuid::new_v4())).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn bring_up_not_found() {
    let h = build_harness();
    let result = auth_context::with_context(admin_ctx(), h.svc.bring_up(Uuid::new_v4())).await;

    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), AppError::NotFound(_)));
}

#[tokio::test]
async fn tear_down_success() {
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    auth_context::with_context(admin_ctx(), h.svc.bring_up(id))
        .await
        .unwrap();
    auth_context::with_context(admin_ctx(), h.svc.tear_down(id, "manual"))
        .await
        .unwrap();

    // Verify tear_down and remove_interface were called.
    {
        let torn_down = h.tunnel_iface.torn_down.lock().unwrap();
        assert_eq!(torn_down.len(), 1);
        assert_eq!(torn_down[0], "wg_ward0");
    }
    {
        let removed = h.tunnel_iface.removed.lock().unwrap();
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0], "wg_ward0");
    }

    // Verify TunnelDown event was published.
    let events = h.events.published_events();
    assert!(events.iter().any(|e| matches!(e, WardnetEvent::TunnelDown { tunnel_id, reason, .. } if *tunnel_id == id && reason == "manual")));

    // Verify tunnel status is now down.
    let tunnel = auth_context::with_context(admin_ctx(), h.svc.get_tunnel(id))
        .await
        .unwrap();
    assert_eq!(tunnel.status, TunnelStatus::Down);
}

#[tokio::test]
async fn tear_down_anonymous_forbidden() {
    let h = build_harness();
    let result = auth_context::with_context(
        AuthContext::Anonymous,
        h.svc.tear_down(Uuid::new_v4(), "test"),
    )
    .await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn delete_tunnel_success() {
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    let del = auth_context::with_context(admin_ctx(), h.svc.delete_tunnel(id))
        .await
        .unwrap();
    assert!(del.message.contains("Sweden VPN"));

    // Verify key was deleted.
    let key_result = h.keys.load_key(&id).await;
    assert!(key_result.is_err());

    // Verify tunnel is gone from the repo.
    let get_result = auth_context::with_context(admin_ctx(), h.svc.get_tunnel(id)).await;
    assert!(matches!(get_result, Err(AppError::NotFound(_))));
}

#[tokio::test]
async fn delete_tunnel_anonymous_forbidden() {
    let h = build_harness();
    let result =
        auth_context::with_context(AuthContext::Anonymous, h.svc.delete_tunnel(Uuid::new_v4()))
            .await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn list_tunnels_returns_all() {
    let h = build_harness();
    auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();

    let mut req2 = sample_request();
    req2.label = "Germany VPN".to_owned();
    req2.country_code = "DE".to_owned();
    auth_context::with_context(admin_ctx(), h.svc.import_tunnel(req2))
        .await
        .unwrap();

    let list = auth_context::with_context(admin_ctx(), h.svc.list_tunnels())
        .await
        .unwrap();
    assert_eq!(list.tunnels.len(), 2);
}

#[tokio::test]
async fn list_tunnels_anonymous_forbidden() {
    let h = build_harness();
    let result = auth_context::with_context(AuthContext::Anonymous, h.svc.list_tunnels()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn list_tunnels_as_device_allowed() {
    let h = build_harness();
    let ctx = AuthContext::Device {
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
    };
    let result = auth_context::with_context(ctx, h.svc.list_tunnels()).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn get_tunnel_anonymous_forbidden() {
    let h = build_harness();
    let result =
        auth_context::with_context(AuthContext::Anonymous, h.svc.get_tunnel(Uuid::new_v4())).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

/// `bring_up_internal` bypasses auth, used by the routing engine for on-demand tunnel startup.
#[tokio::test]
async fn bring_up_internal_succeeds_without_admin_context() {
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    // Call bring_up_internal without admin context — should still succeed.
    h.svc.bring_up_internal(id).await.unwrap();

    let created = h.tunnel_iface.created.lock().unwrap();
    assert_eq!(created.len(), 1, "interface should be created");
}

/// `tear_down_internal` bypasses auth, used by the idle tunnel watcher.
#[tokio::test]
async fn tear_down_internal_succeeds_without_admin_context() {
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    // Bring up first so we can tear down.
    auth_context::with_context(admin_ctx(), h.svc.bring_up(id))
        .await
        .unwrap();

    // Call tear_down_internal without admin context — should still succeed.
    h.svc.tear_down_internal(id, "idle timeout").await.unwrap();

    let torn_down = h.tunnel_iface.torn_down.lock().unwrap();
    assert_eq!(torn_down.len(), 1, "interface should be torn down");
    let removed = h.tunnel_iface.removed.lock().unwrap();
    assert_eq!(removed.len(), 1, "interface should be removed");
}

#[tokio::test]
async fn bring_up_already_up_is_noop() {
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    // Bring up the first time.
    auth_context::with_context(admin_ctx(), h.svc.bring_up(id))
        .await
        .unwrap();

    // Clear events from the first bring_up.
    h.events.events.lock().unwrap().clear();

    // Bring up again -- should be a no-op.
    auth_context::with_context(admin_ctx(), h.svc.bring_up(id))
        .await
        .unwrap();

    // No additional WireGuard calls.
    assert_eq!(h.tunnel_iface.created.lock().unwrap().len(), 1);
    assert_eq!(h.tunnel_iface.brought_up.lock().unwrap().len(), 1);

    // No additional events.
    let events = h.events.published_events();
    assert!(events.is_empty(), "no-op bring_up should not emit events");
}

#[tokio::test]
async fn tear_down_already_down_is_noop() {
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    // Tunnel starts down, so tear_down should be a no-op.
    auth_context::with_context(admin_ctx(), h.svc.tear_down(id, "test"))
        .await
        .unwrap();

    // No WireGuard calls.
    assert!(h.tunnel_iface.torn_down.lock().unwrap().is_empty());
    assert!(h.tunnel_iface.removed.lock().unwrap().is_empty());
}

#[tokio::test]
async fn tear_down_not_found() {
    let h = build_harness();
    let result =
        auth_context::with_context(admin_ctx(), h.svc.tear_down(Uuid::new_v4(), "test")).await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), AppError::NotFound(_)));
}

#[tokio::test]
async fn delete_tunnel_tears_down_if_up() {
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    // Bring the tunnel up first.
    auth_context::with_context(admin_ctx(), h.svc.bring_up(id))
        .await
        .unwrap();

    // Delete should tear down before removing.
    let del = auth_context::with_context(admin_ctx(), h.svc.delete_tunnel(id))
        .await
        .unwrap();
    assert!(del.message.contains("Sweden VPN"));

    // Verify tear_down + remove_interface were called.
    assert_eq!(h.tunnel_iface.torn_down.lock().unwrap().len(), 1);
    assert_eq!(h.tunnel_iface.removed.lock().unwrap().len(), 1);

    // Verify key was deleted.
    assert!(h.keys.load_key(&id).await.is_err());

    // Verify tunnel is gone.
    assert!(matches!(
        auth_context::with_context(admin_ctx(), h.svc.get_tunnel(id)).await,
        Err(AppError::NotFound(_))
    ));
}

#[tokio::test]
async fn delete_tunnel_not_found() {
    let h = build_harness();
    let result = auth_context::with_context(admin_ctx(), h.svc.delete_tunnel(Uuid::new_v4())).await;
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), AppError::NotFound(_)));
}

#[tokio::test]
async fn restore_tunnels_succeeds() {
    let h = build_harness();
    // Import a couple of tunnels.
    auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();

    // restore_tunnels should succeed and not bring any tunnels up.
    h.svc.restore_tunnels().await.unwrap();

    // No WireGuard calls should have been made by restore.
    assert!(h.tunnel_iface.created.lock().unwrap().is_empty());
    assert!(h.tunnel_iface.brought_up.lock().unwrap().is_empty());
}

#[tokio::test]
async fn delete_tunnel_switches_device_rules_and_emits_events() {
    let device_id = Uuid::new_v4();
    let device_repo = Arc::new(MockDeviceRepoWithSwitchedDevices::new(vec![
        device_id.to_string(),
    ]));
    let h = build_harness_with_device_repo(device_repo);

    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let tunnel_id = resp.tunnel.id;

    let del = auth_context::with_context(admin_ctx(), h.svc.delete_tunnel(tunnel_id))
        .await
        .unwrap();
    assert!(del.message.contains("Sweden VPN"));

    // Verify RoutingRuleChanged events were emitted for each switched device.
    let events = h.events.published_events();
    let routing_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, WardnetEvent::RoutingRuleChanged { .. }))
        .collect();
    assert_eq!(
        routing_events.len(),
        1,
        "should emit one RoutingRuleChanged event"
    );
    match &routing_events[0] {
        WardnetEvent::RoutingRuleChanged {
            device_id: did,
            target,
            previous_target,
            ..
        } => {
            assert_eq!(*did, device_id);
            assert!(matches!(
                target,
                wardnet_common::routing::RoutingTarget::Direct
            ));
            assert!(matches!(
                previous_target,
                Some(wardnet_common::routing::RoutingTarget::Tunnel { tunnel_id: tid }) if *tid == tunnel_id
            ));
        }
        _ => panic!("expected RoutingRuleChanged event"),
    }
}

#[tokio::test]
async fn delete_tunnel_with_multiple_switched_devices() {
    let d1 = Uuid::new_v4();
    let d2 = Uuid::new_v4();
    let device_repo = Arc::new(MockDeviceRepoWithSwitchedDevices::new(vec![
        d1.to_string(),
        d2.to_string(),
    ]));
    let h = build_harness_with_device_repo(device_repo);

    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let tunnel_id = resp.tunnel.id;

    auth_context::with_context(admin_ctx(), h.svc.delete_tunnel(tunnel_id))
        .await
        .unwrap();

    let events = h.events.published_events();
    let routing_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, WardnetEvent::RoutingRuleChanged { .. }))
        .collect();
    assert_eq!(
        routing_events.len(),
        2,
        "should emit RoutingRuleChanged for each switched device"
    );
}

#[tokio::test]
async fn delete_tunnel_with_invalid_device_id_skips_event() {
    // If switch_tunnel_rules_to_direct returns a non-UUID string, the code
    // should skip it (the `if let Ok(device_id) = ...parse()` guard).
    let device_repo = Arc::new(MockDeviceRepoWithSwitchedDevices::new(vec![
        "not-a-uuid".to_owned(),
        Uuid::new_v4().to_string(),
    ]));
    let h = build_harness_with_device_repo(device_repo);

    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let tunnel_id = resp.tunnel.id;

    auth_context::with_context(admin_ctx(), h.svc.delete_tunnel(tunnel_id))
        .await
        .unwrap();

    let events = h.events.published_events();
    let routing_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, WardnetEvent::RoutingRuleChanged { .. }))
        .collect();
    // Only the valid UUID should produce an event.
    assert_eq!(
        routing_events.len(),
        1,
        "invalid device ID should be skipped"
    );
}

#[tokio::test]
async fn restore_tunnels_empty_db_succeeds() {
    let h = build_harness();
    h.svc.restore_tunnels().await.unwrap();

    assert!(h.tunnel_iface.created.lock().unwrap().is_empty());
    assert!(h.tunnel_iface.brought_up.lock().unwrap().is_empty());
}

#[tokio::test]
async fn collect_stats_updates_stats_and_publishes_event() {
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    // Bring the tunnel up so `collect_stats` picks it up.
    auth_context::with_context(admin_ctx(), h.svc.bring_up(id))
        .await
        .unwrap();
    h.events.events.lock().unwrap().clear();

    // Stub the interface to return real stats on `get_stats`.
    let handshake = chrono::Utc::now() - chrono::Duration::seconds(30);
    h.tunnel_iface.set_stats(TunnelStats {
        bytes_tx: 1024,
        bytes_rx: 2048,
        last_handshake: Some(handshake),
    });

    h.svc.collect_stats().await.unwrap();

    let events = h.events.published_events();
    assert!(
        events.iter().any(|e| matches!(
            e,
            WardnetEvent::TunnelStatsUpdated {
                tunnel_id, bytes_tx, bytes_rx, ..
            } if *tunnel_id == id && *bytes_tx == 1024 && *bytes_rx == 2048
        )),
        "expected TunnelStatsUpdated event"
    );
}

#[tokio::test]
async fn collect_stats_skips_when_stats_none() {
    // Default stats_behavior is `None` → the `continue` branch is hit.
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    auth_context::with_context(admin_ctx(), h.svc.bring_up(id))
        .await
        .unwrap();
    h.events.events.lock().unwrap().clear();

    h.svc.collect_stats().await.unwrap();

    // No stats updates emitted because get_stats returned None.
    let events = h.events.published_events();
    assert!(
        events
            .iter()
            .all(|e| !matches!(e, WardnetEvent::TunnelStatsUpdated { .. })),
        "no stats event expected when get_stats is None"
    );
}

#[tokio::test]
async fn collect_stats_swallows_interface_error() {
    // When `get_stats` returns an error, `collect_stats` logs and continues
    // without failing overall.
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    auth_context::with_context(admin_ctx(), h.svc.bring_up(id))
        .await
        .unwrap();
    h.tunnel_iface.set_stats_error();

    // Should not return an error; the per-tunnel error is logged and skipped.
    h.svc.collect_stats().await.unwrap();
}

#[tokio::test]
async fn collect_stats_ignores_down_tunnels() {
    // Tunnel is never brought up → `collect_stats` filters it out.
    let h = build_harness();
    auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();

    // Should be a no-op: no stats event emitted.
    h.svc.collect_stats().await.unwrap();
    let events = h.events.published_events();
    assert!(
        events
            .iter()
            .all(|e| !matches!(e, WardnetEvent::TunnelStatsUpdated { .. })),
        "down tunnels should be skipped"
    );
}

#[tokio::test]
async fn run_health_check_warns_on_stale_handshake() {
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    auth_context::with_context(admin_ctx(), h.svc.bring_up(id))
        .await
        .unwrap();

    // Simulate a stale handshake (>3 minutes old).
    let stale = chrono::Utc::now() - chrono::Duration::minutes(10);
    h.tunnel_iface.set_stats(TunnelStats {
        bytes_tx: 0,
        bytes_rx: 0,
        last_handshake: Some(stale),
    });

    // Should succeed regardless — the staleness is only logged.
    h.svc.run_health_check().await.unwrap();
}

#[tokio::test]
async fn run_health_check_fresh_handshake_succeeds() {
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    auth_context::with_context(admin_ctx(), h.svc.bring_up(id))
        .await
        .unwrap();

    let fresh = chrono::Utc::now() - chrono::Duration::seconds(10);
    h.tunnel_iface.set_stats(TunnelStats {
        bytes_tx: 0,
        bytes_rx: 0,
        last_handshake: Some(fresh),
    });

    h.svc.run_health_check().await.unwrap();
}

#[tokio::test]
async fn run_health_check_missing_interface_logs_and_returns_ok() {
    // `get_stats` returning None triggers the "interface removed externally"
    // error log path but health check overall returns Ok.
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    auth_context::with_context(admin_ctx(), h.svc.bring_up(id))
        .await
        .unwrap();
    // Default stats_behavior (None) triggers the missing-interface branch.

    h.svc.run_health_check().await.unwrap();
}

#[tokio::test]
async fn run_health_check_stats_error_logs_and_returns_ok() {
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    auth_context::with_context(admin_ctx(), h.svc.bring_up(id))
        .await
        .unwrap();
    h.tunnel_iface.set_stats_error();

    h.svc.run_health_check().await.unwrap();
}

#[tokio::test]
async fn run_health_check_no_up_tunnels_is_noop() {
    let h = build_harness();
    // No tunnels brought up; run_health_check iterates over an empty list.
    h.svc.run_health_check().await.unwrap();
}

const SAMPLE_CONF_WITH_PRESHARED: &str = "\
[Interface]
PrivateKey = YNqHbfBQKaGvzefSSbufuZKjTIHQadqIyERi1V562lY=
Address = 10.66.0.2/32
DNS = 1.1.1.1

[Peer]
PublicKey = Uf0bMmMFBJbOQtYp3iByaIT5jlQDGHUBk4bH8WDAiUk=
PresharedKey = Uf0bMmMFBJbOQtYp3iByaIT5jlQDGHUBk4bH8WDAiUk=
Endpoint = 198.51.100.2:51820
AllowedIPs = 0.0.0.0/0, ::/0
PersistentKeepalive = 25
";

const SAMPLE_CONF_NO_ENDPOINT: &str = "\
[Interface]
PrivateKey = YNqHbfBQKaGvzefSSbufuZKjTIHQadqIyERi1V562lY=
Address = 10.66.0.2/32
ListenPort = 51820
DNS = 1.1.1.1

[Peer]
PublicKey = Uf0bMmMFBJbOQtYp3iByaIT5jlQDGHUBk4bH8WDAiUk=
AllowedIPs = 10.0.0.0/8
";

#[tokio::test]
async fn bring_up_with_no_endpoint_uses_none_branch() {
    // Config without an `Endpoint = ...` line should hit the `None` match
    // arm of peer endpoint parsing in bring_up_core.
    let h = build_harness();
    let req = CreateTunnelRequest {
        label: "Listen-only".to_owned(),
        country_code: "SE".to_owned(),
        provider: None,
        config: SAMPLE_CONF_NO_ENDPOINT.to_owned(),
    };
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(req))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    auth_context::with_context(admin_ctx(), h.svc.bring_up(id))
        .await
        .unwrap();

    assert_eq!(h.tunnel_iface.brought_up.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn bring_up_with_preshared_key_and_multiple_allowed_ips() {
    // Exercises the preshared-key decode branch and multi-allowed-IP parsing.
    let h = build_harness();
    let req = CreateTunnelRequest {
        label: "Preshared".to_owned(),
        country_code: "NO".to_owned(),
        provider: None,
        config: SAMPLE_CONF_WITH_PRESHARED.to_owned(),
    };
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(req))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    auth_context::with_context(admin_ctx(), h.svc.bring_up(id))
        .await
        .unwrap();

    assert_eq!(h.tunnel_iface.created.lock().unwrap().len(), 1);
    assert_eq!(h.tunnel_iface.brought_up.lock().unwrap().len(), 1);
}
