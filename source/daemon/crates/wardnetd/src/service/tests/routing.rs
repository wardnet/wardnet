use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;
use uuid::Uuid;
use wardnet_types::api::CreateTunnelRequest;
use wardnet_types::device::{Device, DeviceType};
use wardnet_types::routing::{RoutingRule, RoutingTarget, RuleCreator};
use wardnet_types::tunnel::{Tunnel, TunnelConfig, TunnelStatus};
use wardnet_types::wireguard_config::WgPeerConfig;

use crate::auth_context;
use crate::error::AppError;
use crate::firewall::FirewallManager;
use crate::policy_router::PolicyRouter;
use crate::repository::device::DeviceRow;
use crate::repository::tunnel::TunnelRow;
use crate::repository::{DeviceRepository, TunnelRepository};
use crate::service::routing::RoutingServiceImpl;
use crate::service::{RoutingService, TunnelService};
use wardnet_types::auth::AuthContext;

/// Run a future under admin auth context (required by all routing service methods).
async fn as_admin<F: std::future::Future>(f: F) -> F::Output {
    auth_context::with_context(
        AuthContext::Admin {
            admin_id: Uuid::nil(),
        },
        f,
    )
    .await
}

// -- Constants ----------------------------------------------------------------

const DEVICE_1_ID: &str = "00000000-0000-0000-0000-000000000001";
const DEVICE_2_ID: &str = "00000000-0000-0000-0000-000000000002";
const TUNNEL_1_ID: &str = "10000000-0000-0000-0000-000000000001";

// -- Mock DeviceRepository ----------------------------------------------------

/// Mock device repository backed by configurable lists and rule map.
struct MockDeviceRepo {
    devices: Vec<Device>,
    rules: HashMap<String, RoutingRule>,
}

#[async_trait]
impl DeviceRepository for MockDeviceRepo {
    async fn find_by_ip(&self, ip: &str) -> anyhow::Result<Option<Device>> {
        Ok(self.devices.iter().find(|d| d.last_ip == ip).cloned())
    }

    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<Device>> {
        Ok(self
            .devices
            .iter()
            .find(|d| d.id.to_string() == id)
            .cloned())
    }

    async fn find_by_mac(&self, mac: &str) -> anyhow::Result<Option<Device>> {
        Ok(self.devices.iter().find(|d| d.mac == mac).cloned())
    }

    async fn find_all(&self) -> anyhow::Result<Vec<Device>> {
        Ok(self.devices.clone())
    }

    async fn insert(&self, _device: &DeviceRow) -> anyhow::Result<()> {
        Ok(())
    }

    async fn update_last_seen_and_ip(
        &self,
        _id: &str,
        _ip: &str,
        _last_seen: &str,
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

    async fn find_rule_for_device(&self, id: &str) -> anyhow::Result<Option<RoutingRule>> {
        Ok(self.rules.get(id).cloned())
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
        #[allow(clippy::cast_possible_wrap)]
        Ok(self.devices.len() as i64)
    }
}

// -- Mock TunnelRepository ----------------------------------------------------

/// Mock tunnel repository that returns a configurable `TunnelConfig`.
struct MockTunnelRepo {
    config: Option<TunnelConfig>,
}

#[async_trait]
impl TunnelRepository for MockTunnelRepo {
    async fn find_all(&self) -> anyhow::Result<Vec<Tunnel>> {
        Ok(vec![])
    }

    async fn find_by_id(&self, _id: &str) -> anyhow::Result<Option<Tunnel>> {
        Ok(None)
    }

    async fn find_config_by_id(&self, _id: &str) -> anyhow::Result<Option<TunnelConfig>> {
        Ok(self.config.clone())
    }

    async fn insert(&self, _row: &TunnelRow) -> anyhow::Result<()> {
        Ok(())
    }

    async fn update_status(&self, _id: &str, _status: &str) -> anyhow::Result<()> {
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

    async fn delete(&self, _id: &str) -> anyhow::Result<()> {
        Ok(())
    }

    async fn next_interface_index(&self) -> anyhow::Result<i64> {
        Ok(0)
    }

    async fn count(&self) -> anyhow::Result<i64> {
        Ok(0)
    }

    async fn count_active(&self) -> anyhow::Result<i64> {
        Ok(0)
    }
}

// -- Mock TunnelService -------------------------------------------------------

/// Mock tunnel service that returns a configurable tunnel and records
/// `bring_up_internal` / `tear_down_internal` calls.
struct MockTunnelService {
    tunnel: Option<Tunnel>,
    bring_ups: Arc<Mutex<Vec<Uuid>>>,
    tear_downs: Arc<Mutex<Vec<Uuid>>>,
}

#[async_trait]
impl TunnelService for MockTunnelService {
    async fn import_tunnel(
        &self,
        _req: CreateTunnelRequest,
    ) -> Result<wardnet_types::api::CreateTunnelResponse, AppError> {
        unimplemented!("not used in routing tests")
    }

    async fn list_tunnels(&self) -> Result<wardnet_types::api::ListTunnelsResponse, AppError> {
        unimplemented!("not used in routing tests")
    }

    async fn get_tunnel(&self, _id: Uuid) -> Result<Tunnel, AppError> {
        self.tunnel
            .clone()
            .ok_or_else(|| AppError::NotFound("tunnel not found".to_owned()))
    }

    async fn bring_up(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!("not used in routing tests")
    }

    async fn tear_down(&self, _id: Uuid, _reason: &str) -> Result<(), AppError> {
        unimplemented!("not used in routing tests")
    }

    async fn delete_tunnel(
        &self,
        _id: Uuid,
    ) -> Result<wardnet_types::api::DeleteTunnelResponse, AppError> {
        unimplemented!("not used in routing tests")
    }

    async fn bring_up_internal(&self, id: Uuid) -> Result<(), AppError> {
        self.bring_ups.lock().await.push(id);
        Ok(())
    }

    async fn tear_down_internal(&self, id: Uuid, _reason: &str) -> Result<(), AppError> {
        self.tear_downs.lock().await.push(id);
        Ok(())
    }

    async fn restore_tunnels(&self) -> Result<(), AppError> {
        Ok(())
    }
}

// -- Mock PolicyRouter --------------------------------------------------------

/// Records all netlink calls for assertion.
struct MockNetlink {
    calls: Arc<Mutex<Vec<String>>>,
    wardnet_rules: Vec<(String, u32)>,
    /// Number of times `add_route_table` should fail with "not up" before
    /// succeeding. Used to test the retry logic in `ensure_tunnel_table`.
    route_add_failures_remaining: Arc<Mutex<u32>>,
}

#[async_trait]
impl PolicyRouter for MockNetlink {
    async fn enable_ip_forwarding(&self) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push("enable_ip_forwarding".to_owned());
        Ok(())
    }

    async fn add_route_table(&self, interface: &str, table: u32) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push(format!("add_route_table:{interface}:{table}"));
        let mut remaining = self.route_add_failures_remaining.lock().await;
        if *remaining > 0 {
            *remaining -= 1;
            anyhow::bail!("Error: Device for nexthop is not up.");
        }
        Ok(())
    }

    async fn remove_route_table(&self, table: u32) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push(format!("remove_route_table:{table}"));
        Ok(())
    }

    async fn has_route_table(&self, _table: u32) -> anyhow::Result<bool> {
        Ok(false)
    }

    async fn add_ip_rule(&self, src_ip: &str, table: u32) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push(format!("add_ip_rule:{src_ip}:{table}"));
        Ok(())
    }

    async fn remove_ip_rule(&self, src_ip: &str, table: u32) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push(format!("remove_ip_rule:{src_ip}:{table}"));
        Ok(())
    }

    async fn list_wardnet_rules(&self) -> anyhow::Result<Vec<(String, u32)>> {
        self.calls
            .lock()
            .await
            .push("list_wardnet_rules".to_owned());
        Ok(self.wardnet_rules.clone())
    }

    async fn flush_conntrack(&self, src_ip: &str) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push(format!("flush_conntrack:{src_ip}"));
        Ok(())
    }

    async fn flush_route_cache(&self) -> anyhow::Result<()> {
        self.calls.lock().await.push("flush_route_cache".to_owned());
        Ok(())
    }

    async fn check_tools_available(&self) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push("check_tools_available".to_owned());
        Ok(())
    }
}

// -- Mock FirewallManager -----------------------------------------------------

/// Records all nftables calls for assertion.
struct MockNftables {
    calls: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl FirewallManager for MockNftables {
    async fn init_wardnet_table(&self) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push("init_wardnet_table".to_owned());
        Ok(())
    }

    async fn flush_wardnet_table(&self) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push("flush_wardnet_table".to_owned());
        Ok(())
    }

    async fn add_masquerade(&self, interface: &str) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push(format!("add_masquerade:{interface}"));
        Ok(())
    }

    async fn remove_masquerade(&self, interface: &str) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push(format!("remove_masquerade:{interface}"));
        Ok(())
    }

    async fn add_dns_redirect(&self, device_ip: &str, dns_ip: &str) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push(format!("add_dns_redirect:{device_ip}:{dns_ip}"));
        Ok(())
    }

    async fn remove_dns_redirect(&self, device_ip: &str) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push(format!("remove_dns_redirect:{device_ip}"));
        Ok(())
    }

    async fn check_tools_available(&self) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push("check_tools_available".to_owned());
        Ok(())
    }

    async fn destroy_wardnet_table(&self) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push("destroy_wardnet_table".to_owned());
        Ok(())
    }
}

// -- Helpers ------------------------------------------------------------------

/// Aggregate test state with access to recorded mock calls.
#[allow(dead_code)]
struct TestSetup {
    routing: RoutingServiceImpl,
    netlink_calls: Arc<Mutex<Vec<String>>>,
    nftables_calls: Arc<Mutex<Vec<String>>>,
    bring_ups: Arc<Mutex<Vec<Uuid>>>,
    tear_downs: Arc<Mutex<Vec<Uuid>>>,
}

fn device_id_1() -> Uuid {
    DEVICE_1_ID.parse().unwrap()
}

fn device_id_2() -> Uuid {
    DEVICE_2_ID.parse().unwrap()
}

fn tunnel_id_1() -> Uuid {
    TUNNEL_1_ID.parse().unwrap()
}

/// Create a sample device with a given ID and IP.
fn sample_device(id: Uuid, ip: &str) -> Device {
    Device {
        id,
        mac: format!("AA:BB:CC:DD:EE:{:02X}", id.as_bytes()[15]),
        name: Some("Test Device".to_owned()),
        hostname: None,
        manufacturer: None,
        device_type: DeviceType::Phone,
        first_seen: "2026-03-07T00:00:00Z".parse().unwrap(),
        last_seen: "2026-03-07T00:00:00Z".parse().unwrap(),
        last_ip: ip.to_owned(),
        admin_locked: false,
    }
}

/// Create a sample tunnel with configurable status.
fn sample_tunnel(id: Uuid, interface_name: &str, status: TunnelStatus) -> Tunnel {
    Tunnel {
        id,
        label: "Test Tunnel".to_owned(),
        country_code: "US".to_owned(),
        provider: None,
        interface_name: interface_name.to_owned(),
        endpoint: "1.2.3.4:51820".to_owned(),
        status,
        last_handshake: None,
        bytes_tx: 0,
        bytes_rx: 0,
        created_at: chrono::Utc::now(),
    }
}

/// Create a sample `TunnelConfig` with optional DNS servers.
fn sample_tunnel_config(dns: Vec<String>) -> TunnelConfig {
    TunnelConfig {
        address: vec!["10.66.0.2/32".to_owned()],
        dns,
        listen_port: None,
        peer: WgPeerConfig {
            public_key: "Uf0bMmMFBJbOQtYp3iByaIT5jlQDGHUBk4bH8WDAiUk=".to_owned(),
            endpoint: Some("1.2.3.4:51820".to_owned()),
            allowed_ips: vec!["0.0.0.0/0".to_owned()],
            preshared_key: None,
            persistent_keepalive: Some(25),
        },
    }
}

/// Build a `TestSetup` with no devices and a tunnel that is `Up`.
fn setup() -> TestSetup {
    setup_with_devices_and_tunnel(
        vec![],
        HashMap::new(),
        Some(sample_tunnel(tunnel_id_1(), "wg_ward0", TunnelStatus::Up)),
        Some(sample_tunnel_config(vec!["1.1.1.1".to_owned()])),
        "direct".to_owned(),
    )
}

/// Build a `TestSetup` with configurable devices, rules, and default policy.
fn setup_with_devices(devices: Vec<Device>, rules: HashMap<String, RoutingRule>) -> TestSetup {
    setup_with_devices_and_tunnel(
        devices,
        rules,
        Some(sample_tunnel(tunnel_id_1(), "wg_ward0", TunnelStatus::Up)),
        Some(sample_tunnel_config(vec!["1.1.1.1".to_owned()])),
        "direct".to_owned(),
    )
}

/// Full builder for `TestSetup` with all configurable parts.
fn setup_with_devices_and_tunnel(
    devices: Vec<Device>,
    rules: HashMap<String, RoutingRule>,
    tunnel: Option<Tunnel>,
    tunnel_config: Option<TunnelConfig>,
    default_policy: String,
) -> TestSetup {
    let netlink_calls = Arc::new(Mutex::new(Vec::new()));
    let nftables_calls = Arc::new(Mutex::new(Vec::new()));
    let bring_ups = Arc::new(Mutex::new(Vec::new()));
    let tear_downs = Arc::new(Mutex::new(Vec::new()));

    let device_repo: Arc<dyn DeviceRepository> = Arc::new(MockDeviceRepo { devices, rules });
    let tunnel_repo: Arc<dyn TunnelRepository> = Arc::new(MockTunnelRepo {
        config: tunnel_config,
    });
    let tunnel_svc: Arc<dyn TunnelService> = Arc::new(MockTunnelService {
        tunnel,
        bring_ups: bring_ups.clone(),
        tear_downs: tear_downs.clone(),
    });
    let netlink: Arc<dyn PolicyRouter> = Arc::new(MockNetlink {
        calls: netlink_calls.clone(),
        wardnet_rules: vec![],
        route_add_failures_remaining: Arc::new(Mutex::new(0)),
    });
    let nftables: Arc<dyn FirewallManager> = Arc::new(MockNftables {
        calls: nftables_calls.clone(),
    });

    let routing = RoutingServiceImpl::new(
        device_repo,
        tunnel_repo,
        tunnel_svc,
        netlink,
        nftables,
        default_policy,
        "eth0".to_owned(),
    );

    TestSetup {
        routing,
        netlink_calls,
        nftables_calls,
        bring_ups,
        tear_downs,
    }
}

/// Build a `TestSetup` with configurable orphaned kernel rules for reconcile tests.
fn setup_with_orphaned_rules(
    devices: Vec<Device>,
    rules: HashMap<String, RoutingRule>,
    tunnel: Option<Tunnel>,
    tunnel_config: Option<TunnelConfig>,
    kernel_rules: Vec<(String, u32)>,
) -> TestSetup {
    let netlink_calls = Arc::new(Mutex::new(Vec::new()));
    let nftables_calls = Arc::new(Mutex::new(Vec::new()));
    let bring_ups = Arc::new(Mutex::new(Vec::new()));
    let tear_downs = Arc::new(Mutex::new(Vec::new()));

    let device_repo: Arc<dyn DeviceRepository> = Arc::new(MockDeviceRepo { devices, rules });
    let tunnel_repo: Arc<dyn TunnelRepository> = Arc::new(MockTunnelRepo {
        config: tunnel_config,
    });
    let tunnel_svc: Arc<dyn TunnelService> = Arc::new(MockTunnelService {
        tunnel,
        bring_ups: bring_ups.clone(),
        tear_downs: tear_downs.clone(),
    });
    let netlink: Arc<dyn PolicyRouter> = Arc::new(MockNetlink {
        calls: netlink_calls.clone(),
        wardnet_rules: kernel_rules,
        route_add_failures_remaining: Arc::new(Mutex::new(0)),
    });
    let nftables: Arc<dyn FirewallManager> = Arc::new(MockNftables {
        calls: nftables_calls.clone(),
    });

    let routing = RoutingServiceImpl::new(
        device_repo,
        tunnel_repo,
        tunnel_svc,
        netlink,
        nftables,
        "direct".to_owned(),
        "eth0".to_owned(),
    );

    TestSetup {
        routing,
        netlink_calls,
        nftables_calls,
        bring_ups,
        tear_downs,
    }
}

/// Build a `TestSetup` where `add_route_table` fails the first N calls with a
/// "not up" error before succeeding. Used to test the retry logic.
fn setup_with_route_add_failures(failures: u32) -> TestSetup {
    let netlink_calls = Arc::new(Mutex::new(Vec::new()));
    let nftables_calls = Arc::new(Mutex::new(Vec::new()));
    let bring_ups = Arc::new(Mutex::new(Vec::new()));
    let tear_downs = Arc::new(Mutex::new(Vec::new()));

    let device_repo: Arc<dyn DeviceRepository> = Arc::new(MockDeviceRepo {
        devices: vec![],
        rules: HashMap::new(),
    });
    let tunnel_repo: Arc<dyn TunnelRepository> = Arc::new(MockTunnelRepo {
        config: Some(sample_tunnel_config(vec!["1.1.1.1".to_owned()])),
    });
    let tunnel_svc: Arc<dyn TunnelService> = Arc::new(MockTunnelService {
        tunnel: Some(sample_tunnel(tunnel_id_1(), "wg_ward0", TunnelStatus::Up)),
        bring_ups: bring_ups.clone(),
        tear_downs: tear_downs.clone(),
    });
    let netlink: Arc<dyn PolicyRouter> = Arc::new(MockNetlink {
        calls: netlink_calls.clone(),
        wardnet_rules: vec![],
        route_add_failures_remaining: Arc::new(Mutex::new(failures)),
    });
    let nftables: Arc<dyn FirewallManager> = Arc::new(MockNftables {
        calls: nftables_calls.clone(),
    });

    let routing = RoutingServiceImpl::new(
        device_repo,
        tunnel_repo,
        tunnel_svc,
        netlink,
        nftables,
        "direct".to_owned(),
        "eth0".to_owned(),
    );

    TestSetup {
        routing,
        netlink_calls,
        nftables_calls,
        bring_ups,
        tear_downs,
    }
}

// -- Tests: auth context failures ---------------------------------------------

#[tokio::test]
async fn apply_rule_without_auth_context_fails() {
    let ts = setup();

    // Call without wrapping in as_admin — should fail with Forbidden.
    let result = auth_context::with_context(
        AuthContext::Anonymous,
        ts.routing
            .apply_rule(device_id_1(), "192.168.1.10", &RoutingTarget::Direct),
    )
    .await;

    assert!(
        matches!(result, Err(AppError::Forbidden(_))),
        "apply_rule without admin context should return Forbidden, got: {result:?}"
    );
}

#[tokio::test]
async fn remove_device_routes_without_auth_context_fails() {
    let ts = setup();

    let result = auth_context::with_context(
        AuthContext::Anonymous,
        ts.routing
            .remove_device_routes(device_id_1(), "192.168.1.10"),
    )
    .await;

    assert!(
        matches!(result, Err(AppError::Forbidden(_))),
        "remove_device_routes without admin context should return Forbidden, got: {result:?}"
    );
}

#[tokio::test]
async fn handle_ip_change_without_auth_context_fails() {
    let ts = setup();

    let result = auth_context::with_context(
        AuthContext::Anonymous,
        ts.routing
            .handle_ip_change(device_id_1(), "192.168.1.10", "192.168.1.20"),
    )
    .await;

    assert!(
        matches!(result, Err(AppError::Forbidden(_))),
        "handle_ip_change without admin context should return Forbidden, got: {result:?}"
    );
}

#[tokio::test]
async fn handle_tunnel_down_without_auth_context_fails() {
    let ts = setup();

    let result = auth_context::with_context(
        AuthContext::Anonymous,
        ts.routing.handle_tunnel_down(tunnel_id_1()),
    )
    .await;

    assert!(
        matches!(result, Err(AppError::Forbidden(_))),
        "handle_tunnel_down without admin context should return Forbidden, got: {result:?}"
    );
}

#[tokio::test]
async fn handle_tunnel_up_without_auth_context_fails() {
    let ts = setup();

    let result = auth_context::with_context(
        AuthContext::Anonymous,
        ts.routing.handle_tunnel_up(tunnel_id_1()),
    )
    .await;

    assert!(
        matches!(result, Err(AppError::Forbidden(_))),
        "handle_tunnel_up without admin context should return Forbidden, got: {result:?}"
    );
}

#[tokio::test]
async fn reconcile_without_auth_context_fails() {
    let ts = setup();

    let result = auth_context::with_context(AuthContext::Anonymous, ts.routing.reconcile()).await;

    assert!(
        matches!(result, Err(AppError::Forbidden(_))),
        "reconcile without admin context should return Forbidden, got: {result:?}"
    );
}

#[tokio::test]
async fn devices_using_tunnel_without_auth_context_fails() {
    let ts = setup();

    let result = auth_context::with_context(
        AuthContext::Anonymous,
        ts.routing.devices_using_tunnel(tunnel_id_1()),
    )
    .await;

    assert!(
        matches!(result, Err(AppError::Forbidden(_))),
        "devices_using_tunnel without admin context should return Forbidden, got: {result:?}"
    );
}

// -- Tests: apply_rule basics -------------------------------------------------

#[tokio::test]
async fn apply_rule_direct_is_noop_for_kernel() {
    let ts = setup();

    as_admin(
        ts.routing
            .apply_rule(device_id_1(), "192.168.1.10", &RoutingTarget::Direct),
    )
    .await
    .unwrap();

    let nl = ts.netlink_calls.lock().await;
    let nf = ts.nftables_calls.lock().await;

    // Direct routing adds no ip rules or nftables rules — only the conntrack
    // and route-cache flushes that always run after a policy change so old
    // flows don't stay pinned to a previous tunnel's masquerade state.
    assert_eq!(
        *nl,
        vec![
            "flush_conntrack:192.168.1.10".to_owned(),
            "flush_route_cache".to_owned(),
        ],
        "direct routing should only flush conntrack + route cache: {nl:?}"
    );
    assert!(
        nf.is_empty(),
        "no nftables calls expected for direct: {nf:?}"
    );
}

#[tokio::test]
async fn apply_rule_tunnel_adds_ip_rule_and_masquerade() {
    let ts = setup();
    let target = RoutingTarget::Tunnel {
        tunnel_id: tunnel_id_1(),
    };

    as_admin(
        ts.routing
            .apply_rule(device_id_1(), "192.168.1.10", &target),
    )
    .await
    .unwrap();

    let nl = ts.netlink_calls.lock().await;
    let nf = ts.nftables_calls.lock().await;

    // wg_ward0 -> index 0 -> table 100
    assert!(
        nl.contains(&"add_route_table:wg_ward0:100".to_owned()),
        "expected add_route_table call: {nl:?}"
    );
    assert!(
        nl.contains(&"add_ip_rule:192.168.1.10:100".to_owned()),
        "expected add_ip_rule call: {nl:?}"
    );
    assert!(
        nf.contains(&"add_masquerade:wg_ward0".to_owned()),
        "expected add_masquerade call: {nf:?}"
    );
}

#[tokio::test]
async fn apply_rule_tunnel_brings_up_down_tunnel() {
    // Tunnel starts Down, so bring_up_internal should be called.
    let ts = setup_with_devices_and_tunnel(
        vec![],
        HashMap::new(),
        Some(sample_tunnel(tunnel_id_1(), "wg_ward0", TunnelStatus::Down)),
        Some(sample_tunnel_config(vec!["1.1.1.1".to_owned()])),
        "direct".to_owned(),
    );
    let target = RoutingTarget::Tunnel {
        tunnel_id: tunnel_id_1(),
    };

    as_admin(
        ts.routing
            .apply_rule(device_id_1(), "192.168.1.10", &target),
    )
    .await
    .unwrap();

    let ups = ts.bring_ups.lock().await;
    assert_eq!(ups.len(), 1, "bring_up_internal should be called once");
    assert_eq!(ups[0], tunnel_id_1());
}

#[tokio::test]
async fn apply_rule_same_rule_is_noop() {
    let ts = setup();
    let target = RoutingTarget::Tunnel {
        tunnel_id: tunnel_id_1(),
    };

    // First apply.
    as_admin(
        ts.routing
            .apply_rule(device_id_1(), "192.168.1.10", &target),
    )
    .await
    .unwrap();

    // Record call counts after first apply.
    let netlink_count_1 = ts.netlink_calls.lock().await.len();
    let nftables_count_1 = ts.nftables_calls.lock().await.len();

    // Second apply with same target and IP should be a no-op.
    as_admin(
        ts.routing
            .apply_rule(device_id_1(), "192.168.1.10", &target),
    )
    .await
    .unwrap();

    let netlink_count_2 = ts.netlink_calls.lock().await.len();
    let nftables_count_2 = ts.nftables_calls.lock().await.len();

    assert_eq!(
        netlink_count_1, netlink_count_2,
        "no new netlink calls expected on duplicate apply"
    );
    assert_eq!(
        nftables_count_1, nftables_count_2,
        "no new nftables calls expected on duplicate apply"
    );
}

#[tokio::test]
async fn apply_rule_tunnel_with_dns_adds_redirect() {
    let ts = setup_with_devices_and_tunnel(
        vec![],
        HashMap::new(),
        Some(sample_tunnel(tunnel_id_1(), "wg_ward0", TunnelStatus::Up)),
        Some(sample_tunnel_config(vec!["9.9.9.9".to_owned()])),
        "direct".to_owned(),
    );
    let target = RoutingTarget::Tunnel {
        tunnel_id: tunnel_id_1(),
    };

    as_admin(
        ts.routing
            .apply_rule(device_id_1(), "192.168.1.10", &target),
    )
    .await
    .unwrap();

    let nf = ts.nftables_calls.lock().await;
    assert!(
        nf.contains(&"add_dns_redirect:192.168.1.10:9.9.9.9".to_owned()),
        "expected DNS redirect call: {nf:?}"
    );
}

#[tokio::test]
async fn apply_rule_tunnel_not_found_falls_back_to_direct() {
    // No tunnel configured in the mock.
    let ts = setup_with_devices_and_tunnel(vec![], HashMap::new(), None, None, "direct".to_owned());

    let target = RoutingTarget::Tunnel {
        tunnel_id: tunnel_id_1(),
    };

    // Should not error — falls back to direct silently.
    as_admin(
        ts.routing
            .apply_rule(device_id_1(), "192.168.1.10", &target),
    )
    .await
    .unwrap();

    let nl = ts.netlink_calls.lock().await;
    // No ip rule should be added since we fell back to direct.
    let has_add_ip = nl.iter().any(|c| c.starts_with("add_ip_rule"));
    assert!(
        !has_add_ip,
        "no ip rule expected when tunnel not found: {nl:?}"
    );
}

// -- Tests: default resolution ------------------------------------------------

#[tokio::test]
async fn apply_rule_default_resolves_to_direct() {
    let ts = setup();

    // RoutingTarget::Default with default_policy="direct" should resolve to Direct.
    as_admin(
        ts.routing
            .apply_rule(device_id_1(), "192.168.1.10", &RoutingTarget::Default),
    )
    .await
    .unwrap();

    let nl = ts.netlink_calls.lock().await;
    let nf = ts.nftables_calls.lock().await;

    // Direct routing adds no ip rules — only the conntrack and route-cache
    // flushes that always run after a policy change.
    assert_eq!(
        *nl,
        vec![
            "flush_conntrack:192.168.1.10".to_owned(),
            "flush_route_cache".to_owned(),
        ],
        "default->direct should only flush conntrack + route cache: {nl:?}"
    );
    assert!(
        nf.is_empty(),
        "default->direct should not add nftables rules: {nf:?}"
    );
}

// -- Tests: remove_device_routes ----------------------------------------------

#[tokio::test]
async fn remove_device_routes_cleans_up_kernel_state() {
    let ts = setup();
    let target = RoutingTarget::Tunnel {
        tunnel_id: tunnel_id_1(),
    };

    // Apply a tunnel rule first.
    as_admin(
        ts.routing
            .apply_rule(device_id_1(), "192.168.1.10", &target),
    )
    .await
    .unwrap();

    // Clear recorded calls so we only check the remove calls.
    ts.netlink_calls.lock().await.clear();
    ts.nftables_calls.lock().await.clear();

    // Remove routes.
    as_admin(
        ts.routing
            .remove_device_routes(device_id_1(), "192.168.1.10"),
    )
    .await
    .unwrap();

    let nl = ts.netlink_calls.lock().await;
    let nf = ts.nftables_calls.lock().await;

    assert!(
        nl.contains(&"remove_ip_rule:192.168.1.10:100".to_owned()),
        "expected remove_ip_rule call: {nl:?}"
    );
    assert!(
        nf.contains(&"remove_dns_redirect:192.168.1.10".to_owned()),
        "expected remove_dns_redirect call: {nf:?}"
    );
}

// -- Tests: handle_ip_change --------------------------------------------------

#[tokio::test]
async fn handle_ip_change_re_applies_rule() {
    let ts = setup();
    let target = RoutingTarget::Tunnel {
        tunnel_id: tunnel_id_1(),
    };

    // Apply initial rule.
    as_admin(
        ts.routing
            .apply_rule(device_id_1(), "192.168.1.10", &target),
    )
    .await
    .unwrap();

    // Clear calls.
    ts.netlink_calls.lock().await.clear();
    ts.nftables_calls.lock().await.clear();

    // Handle IP change.
    as_admin(
        ts.routing
            .handle_ip_change(device_id_1(), "192.168.1.10", "192.168.1.20"),
    )
    .await
    .unwrap();

    let nl = ts.netlink_calls.lock().await;

    // Old rule should be removed.
    assert!(
        nl.contains(&"remove_ip_rule:192.168.1.10:100".to_owned()),
        "expected old ip rule removal: {nl:?}"
    );
    // New rule should be added.
    assert!(
        nl.contains(&"add_ip_rule:192.168.1.20:100".to_owned()),
        "expected new ip rule addition: {nl:?}"
    );
}

// -- Tests: handle_tunnel_down ------------------------------------------------

#[tokio::test]
async fn handle_tunnel_down_removes_affected_device_rules() {
    let ts = setup();
    let target = RoutingTarget::Tunnel {
        tunnel_id: tunnel_id_1(),
    };

    // Apply rules for two devices.
    as_admin(
        ts.routing
            .apply_rule(device_id_1(), "192.168.1.10", &target),
    )
    .await
    .unwrap();
    as_admin(
        ts.routing
            .apply_rule(device_id_2(), "192.168.1.11", &target),
    )
    .await
    .unwrap();

    // Clear calls.
    ts.netlink_calls.lock().await.clear();
    ts.nftables_calls.lock().await.clear();

    // Tunnel goes down.
    as_admin(ts.routing.handle_tunnel_down(tunnel_id_1()))
        .await
        .unwrap();

    let nl = ts.netlink_calls.lock().await;
    let nf = ts.nftables_calls.lock().await;

    // Both devices' ip rules should be removed.
    assert!(
        nl.contains(&"remove_ip_rule:192.168.1.10:100".to_owned()),
        "expected device 1 ip rule removal: {nl:?}"
    );
    assert!(
        nl.contains(&"remove_ip_rule:192.168.1.11:100".to_owned()),
        "expected device 2 ip rule removal: {nl:?}"
    );

    // DNS redirects should be removed.
    assert!(
        nf.contains(&"remove_dns_redirect:192.168.1.10".to_owned()),
        "expected device 1 dns redirect removal: {nf:?}"
    );
    assert!(
        nf.contains(&"remove_dns_redirect:192.168.1.11".to_owned()),
        "expected device 2 dns redirect removal: {nf:?}"
    );

    // Route table should also be cleaned up.
    assert!(
        nl.contains(&"remove_route_table:100".to_owned()),
        "expected tunnel route table removal: {nl:?}"
    );
}

// -- Tests: handle_tunnel_up --------------------------------------------------

#[tokio::test]
async fn handle_tunnel_up_re_applies_rules() {
    let tid = tunnel_id_1();
    let d1 = sample_device(device_id_1(), "192.168.1.10");
    let d2 = sample_device(device_id_2(), "192.168.1.11");

    let mut rules = HashMap::new();
    rules.insert(
        DEVICE_1_ID.to_owned(),
        RoutingRule {
            device_id: device_id_1(),
            target: RoutingTarget::Tunnel { tunnel_id: tid },
            created_by: RuleCreator::User,
        },
    );
    rules.insert(
        DEVICE_2_ID.to_owned(),
        RoutingRule {
            device_id: device_id_2(),
            target: RoutingTarget::Tunnel { tunnel_id: tid },
            created_by: RuleCreator::User,
        },
    );

    let ts = setup_with_devices(vec![d1, d2], rules);

    // Handle tunnel up — should apply routing rules for both devices from the DB.
    as_admin(ts.routing.handle_tunnel_up(tid)).await.unwrap();

    let nl = ts.netlink_calls.lock().await;

    // Both devices should get ip rules.
    assert!(
        nl.contains(&"add_ip_rule:192.168.1.10:100".to_owned()),
        "expected device 1 ip rule: {nl:?}"
    );
    assert!(
        nl.contains(&"add_ip_rule:192.168.1.11:100".to_owned()),
        "expected device 2 ip rule: {nl:?}"
    );
}

// -- Tests: reconcile ---------------------------------------------------------

#[tokio::test]
async fn reconcile_enables_forwarding_and_inits_nftables() {
    let ts = setup();

    as_admin(ts.routing.reconcile()).await.unwrap();

    let nl = ts.netlink_calls.lock().await;
    let nf = ts.nftables_calls.lock().await;

    assert!(
        nl.contains(&"check_tools_available".to_owned()),
        "expected netlink check_tools_available: {nl:?}"
    );
    assert!(
        nl.contains(&"enable_ip_forwarding".to_owned()),
        "expected enable_ip_forwarding: {nl:?}"
    );
    assert!(
        nf.contains(&"check_tools_available".to_owned()),
        "expected nftables check_tools_available: {nf:?}"
    );
    assert!(
        nf.contains(&"init_wardnet_table".to_owned()),
        "expected init_wardnet_table: {nf:?}"
    );
    assert!(
        nf.contains(&"flush_wardnet_table".to_owned()),
        "expected flush_wardnet_table: {nf:?}"
    );
}

#[tokio::test]
async fn reconcile_applies_stored_rules() {
    let tid = tunnel_id_1();
    let d1 = sample_device(device_id_1(), "192.168.1.10");

    let mut rules = HashMap::new();
    rules.insert(
        DEVICE_1_ID.to_owned(),
        RoutingRule {
            device_id: device_id_1(),
            target: RoutingTarget::Tunnel { tunnel_id: tid },
            created_by: RuleCreator::User,
        },
    );

    let ts = setup_with_devices(vec![d1], rules);

    as_admin(ts.routing.reconcile()).await.unwrap();

    let nl = ts.netlink_calls.lock().await;

    // The device's tunnel rule should be applied during reconcile.
    assert!(
        nl.contains(&"add_ip_rule:192.168.1.10:100".to_owned()),
        "expected reconciled ip rule: {nl:?}"
    );
}

#[tokio::test]
async fn reconcile_cleans_up_orphaned_rules() {
    // Kernel has a rule for 192.168.1.99:100 but no device in the DB has that.
    let orphaned_rules = vec![("192.168.1.99".to_owned(), 100)];

    let ts = setup_with_orphaned_rules(
        vec![],
        HashMap::new(),
        Some(sample_tunnel(tunnel_id_1(), "wg_ward0", TunnelStatus::Up)),
        Some(sample_tunnel_config(vec!["1.1.1.1".to_owned()])),
        orphaned_rules,
    );

    as_admin(ts.routing.reconcile()).await.unwrap();

    let nl = ts.netlink_calls.lock().await;

    // Orphaned rule should be removed.
    assert!(
        nl.contains(&"remove_ip_rule:192.168.1.99:100".to_owned()),
        "expected orphaned rule removal: {nl:?}"
    );
}

// -- Tests: devices_using_tunnel ----------------------------------------------

#[tokio::test]
async fn devices_using_tunnel_returns_correct_list() {
    let ts = setup();
    let tid = tunnel_id_1();
    let target = RoutingTarget::Tunnel { tunnel_id: tid };

    // Apply tunnel rule for device 1 only.
    as_admin(
        ts.routing
            .apply_rule(device_id_1(), "192.168.1.10", &target),
    )
    .await
    .unwrap();

    // Apply direct rule for device 2.
    as_admin(
        ts.routing
            .apply_rule(device_id_2(), "192.168.1.11", &RoutingTarget::Direct),
    )
    .await
    .unwrap();

    let using = as_admin(ts.routing.devices_using_tunnel(tid))
        .await
        .unwrap();
    assert_eq!(using.len(), 1, "only device 1 uses the tunnel");
    assert_eq!(using[0], device_id_1());

    // Unknown tunnel should return empty list.
    let empty = as_admin(ts.routing.devices_using_tunnel(Uuid::new_v4()))
        .await
        .unwrap();
    assert!(empty.is_empty(), "no devices should use unknown tunnel");
}

// -- Tests: ensure_tunnel_table retry logic -----------------------------------

#[tokio::test]
async fn ensure_tunnel_table_retries_on_interface_not_up() {
    // Fail twice with "not up", then succeed on the third attempt.
    let ts = setup_with_route_add_failures(2);
    let target = RoutingTarget::Tunnel {
        tunnel_id: tunnel_id_1(),
    };

    as_admin(
        ts.routing
            .apply_rule(device_id_1(), "192.168.1.10", &target),
    )
    .await
    .unwrap();

    let nl = ts.netlink_calls.lock().await;

    // add_route_table should have been called 3 times (2 failures + 1 success).
    let route_add_count = nl
        .iter()
        .filter(|c| c.starts_with("add_route_table:"))
        .count();
    assert_eq!(
        route_add_count, 3,
        "expected 3 add_route_table calls (2 retries + 1 success): {nl:?}"
    );

    // The ip rule should still be added after the retry succeeds.
    assert!(
        nl.contains(&"add_ip_rule:192.168.1.10:100".to_owned()),
        "expected add_ip_rule after successful retry: {nl:?}"
    );
}

#[tokio::test]
async fn ensure_tunnel_table_falls_back_to_direct_after_max_retries() {
    // Fail more times than the retry limit (max retries is 5, so 6+ failures
    // means it exhausts all attempts). We use 10 to be safely above the limit.
    let ts = setup_with_route_add_failures(10);
    let target = RoutingTarget::Tunnel {
        tunnel_id: tunnel_id_1(),
    };

    // Should not return an error — the apply_rule method catches
    // ensure_tunnel_table failures and falls back to direct.
    as_admin(
        ts.routing
            .apply_rule(device_id_1(), "192.168.1.10", &target),
    )
    .await
    .unwrap();

    let nl = ts.netlink_calls.lock().await;

    // add_route_table should have been called max_retries + 1 times (6 total).
    let route_add_count = nl
        .iter()
        .filter(|c| c.starts_with("add_route_table:"))
        .count();
    assert_eq!(
        route_add_count, 6,
        "expected 6 add_route_table calls (initial + 5 retries): {nl:?}"
    );

    // No ip rule should be added since we fell back to direct.
    let has_add_ip = nl.iter().any(|c| c.starts_with("add_ip_rule"));
    assert!(
        !has_add_ip,
        "no ip rule expected after exhausted retries: {nl:?}"
    );
}
