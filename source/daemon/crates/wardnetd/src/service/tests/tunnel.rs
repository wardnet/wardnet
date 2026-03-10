use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use tokio::sync::broadcast;
use uuid::Uuid;
use wardnet_types::api::CreateTunnelRequest;
use wardnet_types::event::WardnetEvent;
use wardnet_types::tunnel::{Tunnel, TunnelConfig, TunnelStatus};
use wardnet_types::wireguard_config::WgPeerConfig;

use wardnet_types::auth::AuthContext;

use crate::auth_context;
use crate::error::AppError;
use crate::event::EventPublisher;
use crate::keys::KeyStore;
use crate::repository::TunnelRepository;
use crate::repository::tunnel::TunnelRow;
use crate::service::{TunnelService, TunnelServiceImpl};
use crate::wireguard::{CreateInterfaceParams, WgInterfaceStats, WireGuardOps};

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

// -- Mock WireGuardOps ----------------------------------------------------

/// Records calls to `WireGuard` operations for assertion.
struct MockWireGuardOps {
    created: Mutex<Vec<String>>,
    brought_up: Mutex<Vec<String>>,
    torn_down: Mutex<Vec<String>>,
    removed: Mutex<Vec<String>>,
}

impl MockWireGuardOps {
    fn new() -> Self {
        Self {
            created: Mutex::new(Vec::new()),
            brought_up: Mutex::new(Vec::new()),
            torn_down: Mutex::new(Vec::new()),
            removed: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl WireGuardOps for MockWireGuardOps {
    async fn create_interface(&self, params: CreateInterfaceParams) -> anyhow::Result<()> {
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

    async fn remove_interface(&self, interface_name: &str) -> anyhow::Result<()> {
        self.removed.lock().unwrap().push(interface_name.to_owned());
        Ok(())
    }

    async fn get_stats(&self, _interface_name: &str) -> anyhow::Result<Option<WgInterfaceStats>> {
        Ok(None)
    }

    async fn list_interfaces(&self) -> anyhow::Result<Vec<String>> {
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

// -- Helpers --------------------------------------------------------------

struct TestHarness {
    svc: TunnelServiceImpl,
    wireguard: Arc<MockWireGuardOps>,
    events: Arc<MockEventPublisher>,
    keys: Arc<MockKeyStore>,
}

fn build_harness() -> TestHarness {
    let repo = Arc::new(MockTunnelRepo::new());
    let wireguard = Arc::new(MockWireGuardOps::new());
    let keys = Arc::new(MockKeyStore::new());
    let events = Arc::new(MockEventPublisher::new());

    let svc = TunnelServiceImpl::new(repo, wireguard.clone(), keys.clone(), events.clone());

    TestHarness {
        svc,
        wireguard,
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
        let created = h.wireguard.created.lock().unwrap();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0], "wg_ward0");
    }
    {
        let brought_up = h.wireguard.brought_up.lock().unwrap();
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
        let torn_down = h.wireguard.torn_down.lock().unwrap();
        assert_eq!(torn_down.len(), 1);
        assert_eq!(torn_down[0], "wg_ward0");
    }
    {
        let removed = h.wireguard.removed.lock().unwrap();
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
    assert_eq!(h.wireguard.created.lock().unwrap().len(), 1);
    assert_eq!(h.wireguard.brought_up.lock().unwrap().len(), 1);

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
    assert!(h.wireguard.torn_down.lock().unwrap().is_empty());
    assert!(h.wireguard.removed.lock().unwrap().is_empty());
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
    assert_eq!(h.wireguard.torn_down.lock().unwrap().len(), 1);
    assert_eq!(h.wireguard.removed.lock().unwrap().len(), 1);

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
    assert!(h.wireguard.created.lock().unwrap().is_empty());
    assert!(h.wireguard.brought_up.lock().unwrap().is_empty());
}
