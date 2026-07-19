use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;
use uuid::Uuid;
use wardnet_common::api::CreateTunnelRequest;
use wardnet_common::device::{Device, DeviceType};
use wardnet_common::routing::{RoutingRule, RoutingTarget, RuleCreator};
use wardnet_common::tunnel::{Tunnel, TunnelConfig, TunnelStatus};
use wardnet_common::wireguard_config::WgPeerConfig;

use crate::auth_context;
use crate::error::AppError;
use crate::routing::firewall::FirewallManager;
use crate::routing::policy_router::PolicyRouter;
use crate::routing::service::RoutingServiceImpl;
use crate::{RoutingService, TunnelService};
use wardnet_common::auth::AuthContext;
use wardnetd_data::repository::device::DeviceRow;
use wardnetd_data::repository::tunnel::TunnelRow;
use wardnetd_data::repository::{DeviceRepository, SystemConfigRepository, TunnelRepository};

use crate::tests::mac_from;

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

    async fn find_rule_for_device(&self, id: &str) -> anyhow::Result<Option<RoutingRule>> {
        Ok(self.rules.get(id).cloned())
    }
    async fn find_all_rules(&self) -> anyhow::Result<Vec<RoutingRule>> {
        Ok(self.rules.values().cloned().collect())
    }

    async fn upsert_user_rule(&self, _id: &str, _json: &str, _now: &str) -> anyhow::Result<()> {
        Ok(())
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

    async fn update_admin_locked(&self, _id: &str, _locked: bool) -> anyhow::Result<()> {
        Ok(())
    }

    async fn assign_zone(&self, _device_id: &str, _zone_id: &str) -> anyhow::Result<bool> {
        Ok(true)
    }

    async fn count(&self) -> anyhow::Result<i64> {
        #[allow(clippy::cast_possible_wrap)]
        Ok(self.devices.len() as i64)
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

    async fn update_dns_override(&self, _id: &str, _value: bool) -> anyhow::Result<()> {
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

    async fn update_endpoint(
        &self,
        _id: &str,
        _endpoint: &str,
        _peer_config_json: &str,
        _server_name: &str,
        _resolved_at: &str,
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

// -- Mock SystemConfigRepository ----------------------------------------------

/// In-memory key-value mock used by routing tests to back
/// `set_default_policy` / `get_default_policy`.
struct MockSystemConfigRepo {
    store: Mutex<HashMap<String, String>>,
}

impl MockSystemConfigRepo {
    fn new(initial: &[(&str, &str)]) -> Self {
        let mut map = HashMap::new();
        for (k, v) in initial {
            map.insert((*k).to_owned(), (*v).to_owned());
        }
        Self {
            store: Mutex::new(map),
        }
    }
}

#[async_trait]
impl SystemConfigRepository for MockSystemConfigRepo {
    async fn get(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.store.lock().await.get(key).cloned())
    }

    async fn set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        self.store
            .lock()
            .await
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }

    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.store.lock().await.remove(key);
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
    ) -> Result<wardnet_common::api::CreateTunnelResponse, AppError> {
        unimplemented!("not used in routing tests")
    }

    async fn list_tunnels(&self) -> Result<wardnet_common::api::ListTunnelsResponse, AppError> {
        unimplemented!("not used in routing tests")
    }

    async fn get_tunnel(&self, _id: Uuid) -> Result<Tunnel, AppError> {
        self.tunnel
            .clone()
            .ok_or_else(|| AppError::NotFound("tunnel not found".to_owned()))
    }

    async fn test_tunnel(
        &self,
        _id: Uuid,
    ) -> Result<wardnet_common::api::TunnelTestResult, AppError> {
        unimplemented!("not used in routing tests")
    }

    async fn list_tunnel_devices(
        &self,
        _id: Uuid,
    ) -> Result<wardnet_common::api::TunnelDevicesResponse, AppError> {
        unimplemented!("not used in routing tests")
    }

    async fn set_dns_override(&self, _id: Uuid, _value: bool) -> Result<Tunnel, AppError> {
        unimplemented!("not used in routing tests")
    }

    async fn rebuild(&self, _id: Uuid) -> Result<(), AppError> {
        unimplemented!("not used in routing tests")
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
    ) -> Result<wardnet_common::api::DeleteTunnelResponse, AppError> {
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

    async fn collect_stats(&self) -> Result<(), AppError> {
        Ok(())
    }

    async fn run_health_check(&self) -> Result<(), AppError> {
        Ok(())
    }

    async fn probe_latencies(&self) -> Result<(), AppError> {
        Ok(())
    }

    async fn start_speed_test(
        self: Arc<Self>,
        _id: Uuid,
    ) -> Result<wardnet_common::jobs::JobDispatchedResponse, AppError> {
        unimplemented!("not used in routing tests")
    }

    async fn list_speed_tests(
        &self,
        _id: Uuid,
    ) -> Result<wardnet_common::speed_test::TunnelSpeedTestHistoryResponse, AppError> {
        unimplemented!("not used in routing tests")
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
    /// What `has_route_table` returns. Defaults to `true`.
    has_route_table_result: Arc<Mutex<bool>>,
    /// When `true`, `has_route_table` returns an error instead of the result.
    has_route_table_error: Arc<Mutex<bool>>,
    /// Tracks the number of ip rules installed per (ip, table).
    /// Incremented by `add_ip_rule`, decremented by `remove_ip_rule`.
    /// Pre-seeded from `wardnet_rules` to simulate pre-existing kernel state.
    /// Returns an error when count reaches 0 (no such rule).
    rule_counts: Arc<Mutex<HashMap<(String, u32), u32>>>,
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

    async fn has_route_table(&self, table: u32) -> anyhow::Result<bool> {
        self.calls
            .lock()
            .await
            .push(format!("has_route_table:{table}"));
        if *self.has_route_table_error.lock().await {
            anyhow::bail!("mock: has_route_table forced error");
        }
        Ok(*self.has_route_table_result.lock().await)
    }

    async fn add_ip_rule(&self, src_ip: &str, table: u32) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push(format!("add_ip_rule:{src_ip}:{table}"));
        *self
            .rule_counts
            .lock()
            .await
            .entry((src_ip.to_owned(), table))
            .or_insert(0) += 1;
        Ok(())
    }

    async fn remove_ip_rule(&self, src_ip: &str, table: u32) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push(format!("remove_ip_rule:{src_ip}:{table}"));
        let mut counts = self.rule_counts.lock().await;
        let count = counts.entry((src_ip.to_owned(), table)).or_insert(0);
        if *count == 0 {
            anyhow::bail!("RTNETLINK answers: No such process");
        }
        *count -= 1;
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

    async fn add_interface_alias(&self, i: &str, ip: &str, p: u8) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push(format!("add_interface_alias:{ip}/{p}:{i}"));
        Ok(())
    }

    async fn remove_interface_alias(&self, i: &str, ip: &str, p: u8) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push(format!("remove_interface_alias:{ip}/{p}:{i}"));
        Ok(())
    }

    async fn list_interface_aliases(&self, i: &str) -> anyhow::Result<Vec<(String, u8)>> {
        self.calls
            .lock()
            .await
            .push(format!("list_interface_aliases:{i}"));
        Ok(Vec::new())
    }

    async fn set_proxy_arp(&self, i: &str, e: bool) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push(format!("set_proxy_arp:{i}:{e}"));
        Ok(())
    }

    async fn add_host_route(&self, ip: &str, i: &str) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push(format!("add_host_route:{ip}:{i}"));
        Ok(())
    }

    async fn remove_host_route(&self, ip: &str, i: &str) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push(format!("remove_host_route:{ip}:{i}"));
        Ok(())
    }
}

// -- Mock FirewallManager -----------------------------------------------------

/// Records all nftables calls for assertion.
struct MockNftables {
    calls: Arc<Mutex<Vec<String>>>,
    /// When `true`, `add_tcp_reset_reject` returns an error.
    add_tcp_reset_reject_fail: Arc<Mutex<bool>>,
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

    async fn add_inbound_wg_accept(&self, port: u16) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push(format!("add_inbound_wg_accept:{port}"));
        Ok(())
    }

    async fn remove_inbound_wg_accept(&self) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push("remove_inbound_wg_accept".to_owned());
        Ok(())
    }

    async fn cleanup_legacy_dns_redirects(&self) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push("cleanup_legacy_dns_redirects".to_owned());
        Ok(())
    }

    async fn add_tcp_reset_reject(&self, device_ip: &str) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push(format!("add_tcp_reset_reject:{device_ip}"));
        if *self.add_tcp_reset_reject_fail.lock().await {
            anyhow::bail!("mock: add_tcp_reset_reject forced error");
        }
        Ok(())
    }

    async fn remove_tcp_reset_reject(&self, device_ip: &str) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push(format!("remove_tcp_reset_reject:{device_ip}"));
        Ok(())
    }

    async fn apply_zone_rules(
        &self,
        device_ip: &str,
        rules: crate::routing::firewall::ZoneRules,
        lan_interface: &str,
    ) -> anyhow::Result<()> {
        self.calls.lock().await.push(format!(
            "apply_zone_rules:{device_ip}:direct={}:tunnel={}:adminui={}:lan={lan_interface}",
            rules.allow_direct, rules.allow_tunnel, rules.admin_ui_reachable
        ));
        Ok(())
    }

    async fn remove_zone_rules(&self, device_ip: &str) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push(format!("remove_zone_rules:{device_ip}"));
        Ok(())
    }

    async fn list_zone_rule_ips(&self) -> anyhow::Result<Vec<String>> {
        self.calls
            .lock()
            .await
            .push("list_zone_rule_ips".to_owned());
        Ok(Vec::new())
    }

    async fn apply_zone_isolation(
        &self,
        rules: crate::routing::firewall::ZoneIsolationRules,
    ) -> anyhow::Result<()> {
        self.calls.lock().await.push(format!(
            "apply_zone_isolation:allows={}:denies={}:members={}",
            rules.allows.len(),
            rules.deny_pairs.len(),
            rules.member_isolation_subnets.len()
        ));
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
    /// Exposed so tests can persist a policy change behind the routing
    /// service's back, simulating the tunnel-deletion write path.
    system_config: Arc<MockSystemConfigRepo>,
    netlink_calls: Arc<Mutex<Vec<String>>>,
    nftables_calls: Arc<Mutex<Vec<String>>>,
    bring_ups: Arc<Mutex<Vec<Uuid>>>,
    tear_downs: Arc<Mutex<Vec<Uuid>>>,
    has_route_table_result: Arc<Mutex<bool>>,
    has_route_table_error: Arc<Mutex<bool>>,
    add_tcp_reset_reject_fail: Arc<Mutex<bool>>,
    /// Exposed so tests can pre-seed duplicate rule counts.
    netlink_rule_counts: Arc<Mutex<HashMap<(String, u32), u32>>>,
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
        mac: mac_from(id).to_uppercase(),
        name: Some("Test Device".to_owned()),
        hostname: None,
        manufacturer: None,
        device_type: DeviceType::Phone,
        first_seen: "2026-03-07T00:00:00Z".parse().unwrap(),
        last_seen: "2026-03-07T00:00:00Z".parse().unwrap(),
        last_ip: ip.to_owned(),
        admin_locked: false,
        zone_id: "00000000-0000-0000-0000-000000000201".parse().unwrap(),
        dns_capture_enabled: false,
        dns_capture_cap_count: 1000,
        dns_capture_cap_days: 7,
        connection_mode: wardnet_common::device::DeviceConnectionMode::Lan,
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
        override_default_dns: true,
        server_selector: None,
        resolved_server_name: None,
        endpoint_resolved_at: None,
    }
}

/// Create a sample `TunnelConfig` with optional DNS servers. Tests use
/// the helper-defaulted `override_default_dns = !dns.is_empty()`, which
/// matches the import-time default.
fn sample_tunnel_config(dns: Vec<String>) -> TunnelConfig {
    let override_default_dns = !dns.is_empty();
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
        override_default_dns,
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
    let has_route_table_result = Arc::new(Mutex::new(true));
    let has_route_table_error = Arc::new(Mutex::new(false));
    let add_tcp_reset_reject_fail = Arc::new(Mutex::new(false));

    let device_repo: Arc<dyn DeviceRepository> = Arc::new(MockDeviceRepo { devices, rules });
    let tunnel_repo: Arc<dyn TunnelRepository> = Arc::new(MockTunnelRepo {
        config: tunnel_config,
    });
    let tunnel_svc: Arc<dyn TunnelService> = Arc::new(MockTunnelService {
        tunnel,
        bring_ups: bring_ups.clone(),
        tear_downs: tear_downs.clone(),
    });
    let rule_counts: Arc<Mutex<HashMap<(String, u32), u32>>> = Arc::new(Mutex::new(HashMap::new()));
    let netlink: Arc<dyn PolicyRouter> = Arc::new(MockNetlink {
        calls: netlink_calls.clone(),
        wardnet_rules: vec![],
        route_add_failures_remaining: Arc::new(Mutex::new(0)),
        has_route_table_result: has_route_table_result.clone(),
        has_route_table_error: has_route_table_error.clone(),
        rule_counts: rule_counts.clone(),
    });
    let nftables: Arc<dyn FirewallManager> = Arc::new(MockNftables {
        calls: nftables_calls.clone(),
        add_tcp_reset_reject_fail: add_tcp_reset_reject_fail.clone(),
    });

    let system_config = Arc::new(MockSystemConfigRepo::new(&[(
        "default_policy",
        &default_policy,
    )]));

    let routing = RoutingServiceImpl::new(
        device_repo,
        tunnel_repo,
        tunnel_svc,
        netlink,
        nftables,
        system_config.clone(),
        Arc::new(crate::event::BroadcastEventBus::new(16)),
        default_policy,
        "eth0".to_owned(),
    );

    TestSetup {
        routing,
        system_config,
        netlink_calls,
        nftables_calls,
        bring_ups,
        tear_downs,
        has_route_table_result,
        has_route_table_error,
        add_tcp_reset_reject_fail,
        netlink_rule_counts: rule_counts,
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
    let has_route_table_result = Arc::new(Mutex::new(true));
    let has_route_table_error = Arc::new(Mutex::new(false));
    let add_tcp_reset_reject_fail = Arc::new(Mutex::new(false));

    let device_repo: Arc<dyn DeviceRepository> = Arc::new(MockDeviceRepo { devices, rules });
    let tunnel_repo: Arc<dyn TunnelRepository> = Arc::new(MockTunnelRepo {
        config: tunnel_config,
    });
    let tunnel_svc: Arc<dyn TunnelService> = Arc::new(MockTunnelService {
        tunnel,
        bring_ups: bring_ups.clone(),
        tear_downs: tear_downs.clone(),
    });
    let initial_counts: HashMap<(String, u32), u32> =
        kernel_rules
            .iter()
            .fold(HashMap::new(), |mut m, (ip, table)| {
                *m.entry((ip.clone(), *table)).or_insert(0) += 1;
                m
            });
    let rule_counts: Arc<Mutex<HashMap<(String, u32), u32>>> = Arc::new(Mutex::new(initial_counts));
    let netlink: Arc<dyn PolicyRouter> = Arc::new(MockNetlink {
        calls: netlink_calls.clone(),
        wardnet_rules: kernel_rules,
        route_add_failures_remaining: Arc::new(Mutex::new(0)),
        has_route_table_result: has_route_table_result.clone(),
        has_route_table_error: has_route_table_error.clone(),
        rule_counts: rule_counts.clone(),
    });
    let nftables: Arc<dyn FirewallManager> = Arc::new(MockNftables {
        calls: nftables_calls.clone(),
        add_tcp_reset_reject_fail: add_tcp_reset_reject_fail.clone(),
    });

    let system_config = Arc::new(MockSystemConfigRepo::new(&[("default_policy", "direct")]));

    let routing = RoutingServiceImpl::new(
        device_repo,
        tunnel_repo,
        tunnel_svc,
        netlink,
        nftables,
        system_config.clone(),
        Arc::new(crate::event::BroadcastEventBus::new(16)),
        "direct".to_owned(),
        "eth0".to_owned(),
    );

    TestSetup {
        routing,
        system_config,
        netlink_calls,
        nftables_calls,
        bring_ups,
        tear_downs,
        has_route_table_result,
        has_route_table_error,
        add_tcp_reset_reject_fail,
        netlink_rule_counts: rule_counts,
    }
}

/// Build a `TestSetup` where `add_route_table` fails the first N calls with a
/// "not up" error before succeeding. Used to test the retry logic.
fn setup_with_route_add_failures(failures: u32) -> TestSetup {
    let netlink_calls = Arc::new(Mutex::new(Vec::new()));
    let nftables_calls = Arc::new(Mutex::new(Vec::new()));
    let bring_ups = Arc::new(Mutex::new(Vec::new()));
    let tear_downs = Arc::new(Mutex::new(Vec::new()));
    let has_route_table_result = Arc::new(Mutex::new(true));
    let has_route_table_error = Arc::new(Mutex::new(false));
    let add_tcp_reset_reject_fail = Arc::new(Mutex::new(false));

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
    let rule_counts: Arc<Mutex<HashMap<(String, u32), u32>>> = Arc::new(Mutex::new(HashMap::new()));
    let netlink: Arc<dyn PolicyRouter> = Arc::new(MockNetlink {
        calls: netlink_calls.clone(),
        wardnet_rules: vec![],
        route_add_failures_remaining: Arc::new(Mutex::new(failures)),
        has_route_table_result: has_route_table_result.clone(),
        has_route_table_error: has_route_table_error.clone(),
        rule_counts: rule_counts.clone(),
    });
    let nftables: Arc<dyn FirewallManager> = Arc::new(MockNftables {
        calls: nftables_calls.clone(),
        add_tcp_reset_reject_fail: add_tcp_reset_reject_fail.clone(),
    });

    let system_config = Arc::new(MockSystemConfigRepo::new(&[("default_policy", "direct")]));

    let routing = RoutingServiceImpl::new(
        device_repo,
        tunnel_repo,
        tunnel_svc,
        netlink,
        nftables,
        system_config.clone(),
        Arc::new(crate::event::BroadcastEventBus::new(16)),
        "direct".to_owned(),
        "eth0".to_owned(),
    );

    TestSetup {
        routing,
        system_config,
        netlink_calls,
        nftables_calls,
        bring_ups,
        tear_downs,
        has_route_table_result,
        has_route_table_error,
        add_tcp_reset_reject_fail,
        netlink_rule_counts: rule_counts,
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

    // Direct routing adds no ip rules — only the stale-connection flush
    // (TCP RST + conntrack + route cache) that always runs after a policy
    // change so old flows don't stay pinned to a previous tunnel.
    assert!(
        nl.contains(&"flush_conntrack:192.168.1.10".to_owned()),
        "expected conntrack flush: {nl:?}"
    );
    assert!(
        nf.contains(&"add_tcp_reset_reject:192.168.1.10".to_owned()),
        "expected TCP RST reject rule: {nf:?}"
    );
    assert!(
        nf.contains(&"remove_tcp_reset_reject:192.168.1.10".to_owned()),
        "expected TCP RST reject rule removal: {nf:?}"
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
async fn apply_rule_tunnel_with_override_marks_dns_upstream_as_tunnel() {
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
        !nf.iter().any(|c| c.contains("dns_redirect")),
        "DNAT machinery removed (#342); no dns_redirect calls expected: {nf:?}"
    );
    let snapshot = ts.routing.dns_upstream_snapshot();
    let map = snapshot.load();
    let entry = map
        .get(&"192.168.1.10".parse::<std::net::IpAddr>().unwrap())
        .copied();
    assert_eq!(
        entry,
        Some(wardnet_common::dns::UpstreamId::Tunnel(tunnel_id_1())),
        "device IP should map to Tunnel(_) upstream when override_default_dns is on"
    );
}

#[tokio::test]
async fn apply_rule_tunnel_without_override_uses_default_upstream() {
    let mut config = sample_tunnel_config(vec!["9.9.9.9".to_owned()]);
    config.override_default_dns = false;

    let ts = setup_with_devices_and_tunnel(
        vec![],
        HashMap::new(),
        Some(sample_tunnel(tunnel_id_1(), "wg_ward0", TunnelStatus::Up)),
        Some(config),
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

    let snapshot = ts.routing.dns_upstream_snapshot();
    let map = snapshot.load();
    let entry = map.get(&"192.168.1.10".parse::<std::net::IpAddr>().unwrap());
    assert!(
        entry.is_none(),
        "without override, device IP must not appear in the snapshot (default upstream pool used)"
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

    // Direct routing adds no ip rules — only the stale-connection flush.
    assert!(
        nl.contains(&"flush_conntrack:192.168.1.10".to_owned()),
        "default->direct should flush conntrack: {nl:?}"
    );
    assert!(
        nf.contains(&"add_tcp_reset_reject:192.168.1.10".to_owned()),
        "default->direct should add TCP RST reject: {nf:?}"
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
        !nf.iter().any(|c| c.contains("dns_redirect")),
        "DNAT machinery removed (#342); no dns_redirect calls expected: {nf:?}"
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

    // DNAT machinery removed (#342); no dns_redirect calls should fire
    // when devices fall off this tunnel.
    assert!(
        !nf.iter().any(|c| c.contains("dns_redirect")),
        "no dns_redirect calls expected: {nf:?}"
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

// -- Tests: ensure_tunnel_table kernel route verification --------------------

#[tokio::test]
async fn ensure_tunnel_table_re_adds_when_kernel_route_missing() {
    let ts = setup();
    let target = RoutingTarget::Tunnel {
        tunnel_id: tunnel_id_1(),
    };

    // First apply — sets up the tunnel table.
    as_admin(
        ts.routing
            .apply_rule(device_id_1(), "192.168.1.10", &target),
    )
    .await
    .unwrap();

    ts.netlink_calls.lock().await.clear();
    ts.nftables_calls.lock().await.clear();

    // Apply for device 2 — ensure_tunnel_table verifies the kernel route
    // (returns true) and skips re-adding.
    as_admin(
        ts.routing
            .apply_rule(device_id_2(), "192.168.1.11", &target),
    )
    .await
    .unwrap();

    let nl = ts.netlink_calls.lock().await;
    assert!(
        nl.contains(&"has_route_table:100".to_owned()),
        "expected kernel route verification: {nl:?}"
    );
    let route_adds = nl
        .iter()
        .filter(|c| c.starts_with("add_route_table:"))
        .count();
    assert_eq!(
        route_adds, 0,
        "should not re-add route when kernel route exists: {nl:?}"
    );
}

#[tokio::test]
async fn ensure_tunnel_table_re_adds_route_when_kernel_says_missing() {
    let ts = setup();
    let target = RoutingTarget::Tunnel {
        tunnel_id: tunnel_id_1(),
    };

    // First apply — sets up the tunnel table normally.
    as_admin(
        ts.routing
            .apply_rule(device_id_1(), "192.168.1.10", &target),
    )
    .await
    .unwrap();

    // Simulate kernel route vanishing: flip has_route_table to false.
    *ts.has_route_table_result.lock().await = false;
    ts.netlink_calls.lock().await.clear();
    ts.nftables_calls.lock().await.clear();

    // Apply for device 2 — ensure_tunnel_table should detect the missing
    // route and re-add it.
    as_admin(
        ts.routing
            .apply_rule(device_id_2(), "192.168.1.11", &target),
    )
    .await
    .unwrap();

    let nl = ts.netlink_calls.lock().await;
    assert!(
        nl.contains(&"has_route_table:100".to_owned()),
        "expected kernel route check: {nl:?}"
    );
    assert!(
        nl.iter()
            .any(|c| c.starts_with("add_route_table:wg_ward0:100")),
        "expected route to be re-added after kernel reports missing: {nl:?}"
    );
}

// -- Tests: flush_stale_connections ------------------------------------------

#[tokio::test]
async fn apply_rule_injects_tcp_rst_on_switch() {
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

    let nf = ts.nftables_calls.lock().await;

    // TCP RST reject should be added then removed during flush_stale_connections.
    assert!(
        nf.contains(&"add_tcp_reset_reject:192.168.1.10".to_owned()),
        "expected TCP RST reject rule to be added: {nf:?}"
    );
    assert!(
        nf.contains(&"remove_tcp_reset_reject:192.168.1.10".to_owned()),
        "expected TCP RST reject rule to be removed: {nf:?}"
    );

    // Verify ordering: add comes before remove.
    let add_pos = nf
        .iter()
        .position(|c| c == "add_tcp_reset_reject:192.168.1.10")
        .unwrap();
    let remove_pos = nf
        .iter()
        .position(|c| c == "remove_tcp_reset_reject:192.168.1.10")
        .unwrap();
    assert!(
        add_pos < remove_pos,
        "TCP RST add should come before remove: {nf:?}"
    );
}

// -- Tests: handle_route_table_lost -----------------------------------------

#[tokio::test]
async fn handle_route_table_lost_re_applies_rules() {
    let ts = setup();
    let target = RoutingTarget::Tunnel {
        tunnel_id: tunnel_id_1(),
    };

    // Apply tunnel rule for device 1.
    as_admin(
        ts.routing
            .apply_rule(device_id_1(), "192.168.1.10", &target),
    )
    .await
    .unwrap();

    // Clear recorded calls.
    ts.netlink_calls.lock().await.clear();
    ts.nftables_calls.lock().await.clear();

    // Simulate route table 100 being lost.
    as_admin(ts.routing.handle_route_table_lost(100))
        .await
        .unwrap();

    let nl = ts.netlink_calls.lock().await;

    // The route table should be re-added (since we cleared tunnel_tables).
    assert!(
        nl.iter()
            .any(|c| c.starts_with("add_route_table:wg_ward0:100")),
        "expected route table to be re-added after loss: {nl:?}"
    );
    // The device's ip rule should be re-applied.
    assert!(
        nl.contains(&"add_ip_rule:192.168.1.10:100".to_owned()),
        "expected ip rule to be re-applied: {nl:?}"
    );
}

// -- Tests: duplicate ip rule cleanup (issue #78) ----------------------------

#[tokio::test]
async fn remove_device_kernel_state_drains_duplicate_ip_rules() {
    let ts = setup();
    let target = RoutingTarget::Tunnel {
        tunnel_id: tunnel_id_1(),
    };

    // Apply a tunnel rule — rule_counts[("192.168.1.10", 100)] becomes 1.
    as_admin(
        ts.routing
            .apply_rule(device_id_1(), "192.168.1.10", &target),
    )
    .await
    .unwrap();

    // Simulate a stale duplicate by bumping the kernel count to 2.
    ts.netlink_rule_counts
        .lock()
        .await
        .insert(("192.168.1.10".to_owned(), 100), 2);
    ts.netlink_calls.lock().await.clear();

    // Removing routes should loop until remove_ip_rule fails (count 0).
    as_admin(
        ts.routing
            .remove_device_routes(device_id_1(), "192.168.1.10"),
    )
    .await
    .unwrap();

    let nl = ts.netlink_calls.lock().await;
    // Count=2 → 2 successes + 1 failing attempt = 3 calls total.
    let remove_count = nl
        .iter()
        .filter(|c| c.as_str() == "remove_ip_rule:192.168.1.10:100")
        .count();
    assert_eq!(
        remove_count, 3,
        "expected 3 remove_ip_rule calls (2 successes + 1 drain attempt): {nl:?}"
    );
}

#[tokio::test]
async fn reconcile_prunes_duplicate_rules_for_active_device() {
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

    // Kernel has 3 entries for the same (ip, table) — 2 are duplicates.
    let kernel_rules = vec![
        ("192.168.1.10".to_owned(), 100u32),
        ("192.168.1.10".to_owned(), 100u32),
        ("192.168.1.10".to_owned(), 100u32),
    ];
    let ts = setup_with_orphaned_rules(
        vec![d1],
        rules,
        Some(sample_tunnel(tunnel_id_1(), "wg_ward0", TunnelStatus::Up)),
        Some(sample_tunnel_config(vec!["1.1.1.1".to_owned()])),
        kernel_rules,
    );

    as_admin(ts.routing.reconcile()).await.unwrap();

    let nl = ts.netlink_calls.lock().await;
    // Reconcile applies 1 rule via apply_rule, then sees count=3 in kernel →
    // extras=2 → exactly 2 remove_ip_rule calls for the duplicate pruning.
    let remove_count = nl
        .iter()
        .filter(|c| c.as_str() == "remove_ip_rule:192.168.1.10:100")
        .count();
    assert_eq!(
        remove_count, 2,
        "expected exactly 2 remove_ip_rule calls to prune 2 duplicate rules: {nl:?}"
    );
}

#[tokio::test]
async fn handle_route_table_lost_noop_when_no_devices_affected() {
    let ts = setup();

    // No devices have been routed through any table, so table 200 has no
    // affected devices. The call should return Ok without any netlink calls.
    as_admin(ts.routing.handle_route_table_lost(200))
        .await
        .unwrap();

    // No ip rule or route table calls should have been made.
    let nl = ts.netlink_calls.lock().await;
    let rule_adds: Vec<_> = nl
        .iter()
        .filter(|c| c.starts_with("add_ip_rule:") || c.starts_with("add_route_table:"))
        .collect();
    assert!(
        rule_adds.is_empty(),
        "expected no routing calls for unaffected table: {rule_adds:?}"
    );
}

#[tokio::test]
async fn ensure_tunnel_table_re_adds_route_when_has_route_table_errors() {
    let ts = setup();
    let target = RoutingTarget::Tunnel {
        tunnel_id: tunnel_id_1(),
    };

    // First apply — sets up the tunnel table normally.
    as_admin(
        ts.routing
            .apply_rule(device_id_1(), "192.168.1.10", &target),
    )
    .await
    .unwrap();

    // Make has_route_table return an error (simulates netlink failure).
    *ts.has_route_table_error.lock().await = true;
    ts.netlink_calls.lock().await.clear();
    ts.nftables_calls.lock().await.clear();

    // Apply for device 2 — ensure_tunnel_table should hit the Err branch
    // and re-add the route defensively.
    as_admin(
        ts.routing
            .apply_rule(device_id_2(), "192.168.1.11", &target),
    )
    .await
    .unwrap();

    let nl = ts.netlink_calls.lock().await;
    assert!(
        nl.contains(&"has_route_table:100".to_owned()),
        "expected kernel route check: {nl:?}"
    );
    assert!(
        nl.iter()
            .any(|c| c.starts_with("add_route_table:wg_ward0:100")),
        "expected route to be re-added after has_route_table error: {nl:?}"
    );
}

#[tokio::test]
async fn flush_stale_connections_skips_remove_when_add_rst_fails() {
    let ts = setup();
    let target = RoutingTarget::Tunnel {
        tunnel_id: tunnel_id_1(),
    };

    // Make add_tcp_reset_reject fail.
    *ts.add_tcp_reset_reject_fail.lock().await = true;

    as_admin(
        ts.routing
            .apply_rule(device_id_1(), "192.168.1.10", &target),
    )
    .await
    .unwrap();

    let nf = ts.nftables_calls.lock().await;

    // The add was attempted (and failed).
    assert!(
        nf.contains(&"add_tcp_reset_reject:192.168.1.10".to_owned()),
        "expected TCP RST add attempt: {nf:?}"
    );
    // Remove should NOT be called since add failed (rst_added = false).
    assert!(
        !nf.contains(&"remove_tcp_reset_reject:192.168.1.10".to_owned()),
        "expected no TCP RST remove when add failed: {nf:?}"
    );

    // Conntrack flush should still happen (it's independent).
    let nl = ts.netlink_calls.lock().await;
    assert!(
        nl.contains(&"flush_conntrack:192.168.1.10".to_owned()),
        "expected conntrack flush even when RST add failed: {nl:?}"
    );
}

#[tokio::test]
async fn set_default_policy_persists_and_updates_in_memory() {
    let ts = setup();

    // Initial in-memory + persisted policy is "direct".
    assert_eq!(
        as_admin(ts.routing.default_policy()).await.unwrap(),
        "direct"
    );

    let tunnel_uuid = "10000000-0000-0000-0000-000000000099";
    as_admin(ts.routing.set_default_policy(tunnel_uuid))
        .await
        .unwrap();

    assert_eq!(
        as_admin(ts.routing.default_policy()).await.unwrap(),
        tunnel_uuid
    );
}

#[tokio::test]
async fn set_default_policy_rejects_invalid_value() {
    let ts = setup();

    let err = as_admin(ts.routing.set_default_policy("not-a-uuid"))
        .await
        .expect_err("invalid policy should be rejected");

    assert!(matches!(err, AppError::BadRequest(_)));
    // In-memory state unchanged.
    assert_eq!(
        as_admin(ts.routing.default_policy()).await.unwrap(),
        "direct"
    );
}

#[tokio::test]
async fn set_default_policy_changes_resolution_of_default_target() {
    let ts = setup_with_devices_and_tunnel(
        vec![],
        HashMap::new(),
        Some(sample_tunnel(tunnel_id_1(), "wg_ward0", TunnelStatus::Up)),
        Some(sample_tunnel_config(vec!["1.1.1.1".to_owned()])),
        "direct".to_owned(),
    );

    // First apply with default policy = "direct" — RoutingTarget::Default
    // resolves to Direct, so no ip rule is added.
    as_admin(
        ts.routing
            .apply_rule(device_id_1(), "192.168.1.10", &RoutingTarget::Default),
    )
    .await
    .unwrap();

    let nl_after_direct = ts.netlink_calls.lock().await.clone();
    assert!(
        !nl_after_direct
            .iter()
            .any(|c| c.starts_with("add_ip_rule:192.168.1.10")),
        "Default → Direct should not add an ip rule: {nl_after_direct:?}"
    );

    // Switch the policy to a tunnel UUID — Default should now resolve to
    // that tunnel on the next apply.
    as_admin(ts.routing.set_default_policy(&tunnel_id_1().to_string()))
        .await
        .unwrap();

    as_admin(
        ts.routing
            .apply_rule(device_id_2(), "192.168.1.20", &RoutingTarget::Default),
    )
    .await
    .unwrap();

    let nl_after_tunnel = ts.netlink_calls.lock().await.clone();
    assert!(
        nl_after_tunnel
            .iter()
            .any(|c| c.starts_with("add_ip_rule:192.168.1.20")),
        "Default → Tunnel should add an ip rule for the new device: {nl_after_tunnel:?}"
    );
}

#[tokio::test]
async fn set_default_policy_reapplies_already_routed_default_devices() {
    // Device with a stored DB rule of RoutingTarget::Default. Under
    // policy="direct" it routes Direct (no ip rule). The moment the
    // operator switches the policy to a tunnel, the device must
    // re-route through the tunnel — without this the cached `applied`
    // entry holds Direct and apply_rule's phase-1 short-circuit would
    // keep the old route until something else (IP change / tunnel
    // bounce) happened to the device.
    let d1 = sample_device(device_id_1(), "192.168.1.10");
    let mut rules = HashMap::new();
    rules.insert(
        DEVICE_1_ID.to_owned(),
        RoutingRule {
            device_id: device_id_1(),
            target: RoutingTarget::Default,
            created_by: RuleCreator::User,
        },
    );
    let ts = setup_with_devices_and_tunnel(
        vec![d1],
        rules,
        Some(sample_tunnel(tunnel_id_1(), "wg_ward0", TunnelStatus::Up)),
        Some(sample_tunnel_config(vec!["1.1.1.1".to_owned()])),
        "direct".to_owned(),
    );

    // Initial apply under policy="direct" → no ip rule.
    as_admin(
        ts.routing
            .apply_rule(device_id_1(), "192.168.1.10", &RoutingTarget::Default),
    )
    .await
    .unwrap();
    {
        let nl = ts.netlink_calls.lock().await;
        assert!(
            !nl.iter().any(|c| c.starts_with("add_ip_rule:192.168.1.10")),
            "Default → Direct should not add an ip rule: {nl:?}"
        );
    }

    // Flip the policy, then run the handler the routing listener invokes
    // on the resulting DefaultPolicyChanged event — the re-apply of
    // already-routed devices is event-driven, not inline in the setter.
    as_admin(ts.routing.set_default_policy(&tunnel_id_1().to_string()))
        .await
        .unwrap();
    as_admin(ts.routing.handle_default_policy_changed())
        .await
        .unwrap();

    let nl = ts.netlink_calls.lock().await;
    assert!(
        nl.iter().any(|c| c.starts_with("add_ip_rule:192.168.1.10")),
        "Default → Tunnel should re-route the existing device: {nl:?}"
    );
}

#[tokio::test]
async fn set_default_policy_does_not_touch_explicit_targets() {
    // A device with an explicit Tunnel target (not Default) must NOT
    // be re-applied when the default policy changes — its rule didn't
    // depend on the policy.
    let d1 = sample_device(device_id_1(), "192.168.1.10");
    let mut rules = HashMap::new();
    rules.insert(
        DEVICE_1_ID.to_owned(),
        RoutingRule {
            device_id: device_id_1(),
            target: RoutingTarget::Direct,
            created_by: RuleCreator::User,
        },
    );
    let ts = setup_with_devices_and_tunnel(
        vec![d1],
        rules,
        Some(sample_tunnel(tunnel_id_1(), "wg_ward0", TunnelStatus::Up)),
        Some(sample_tunnel_config(vec!["1.1.1.1".to_owned()])),
        "direct".to_owned(),
    );

    // Apply the explicit Direct rule (no kernel state needed).
    as_admin(
        ts.routing
            .apply_rule(device_id_1(), "192.168.1.10", &RoutingTarget::Direct),
    )
    .await
    .unwrap();

    let baseline = ts.netlink_calls.lock().await.len();
    drop(ts.netlink_calls.lock().await);

    // Flip the default policy and run the event-driven re-apply — it
    // should NOT touch explicit-Direct devices.
    as_admin(ts.routing.set_default_policy(&tunnel_id_1().to_string()))
        .await
        .unwrap();
    as_admin(ts.routing.handle_default_policy_changed())
        .await
        .unwrap();

    let nl = ts.netlink_calls.lock().await;
    // No new add_ip_rule for the explicit-Direct device.
    let new_rules: Vec<_> = nl
        .iter()
        .skip(baseline)
        .filter(|c| c.starts_with("add_ip_rule:192.168.1.10"))
        .collect();
    assert!(
        new_rules.is_empty(),
        "policy change must not re-apply devices with explicit targets: {nl:?}"
    );
}

#[tokio::test]
async fn handle_default_policy_changed_syncs_and_reapplies_default_ruled_devices() {
    // Mirrors the tunnel-deletion path: the default policy points at a
    // tunnel, a Default-ruled device routes through it, and then the
    // policy is reset to "direct" *behind the routing service's back*
    // (persisted by the tunnel service, announced via
    // DefaultPolicyChanged). The handler must re-read the persisted
    // policy, update the in-memory copy, and re-route the device —
    // without it, the cache keeps the deleted tunnel's UUID and every
    // later resolve falls back to direct only via the NotFound path.
    let d1 = sample_device(device_id_1(), "192.168.1.10");
    let mut rules = HashMap::new();
    rules.insert(
        DEVICE_1_ID.to_owned(),
        RoutingRule {
            device_id: device_id_1(),
            target: RoutingTarget::Default,
            created_by: RuleCreator::User,
        },
    );
    let ts = setup_with_devices_and_tunnel(
        vec![d1],
        rules,
        Some(sample_tunnel(tunnel_id_1(), "wg_ward0", TunnelStatus::Up)),
        Some(sample_tunnel_config(vec!["1.1.1.1".to_owned()])),
        tunnel_id_1().to_string(),
    );

    // Initial apply under policy=tunnel → device routes through it.
    as_admin(
        ts.routing
            .apply_rule(device_id_1(), "192.168.1.10", &RoutingTarget::Default),
    )
    .await
    .unwrap();
    {
        let nl = ts.netlink_calls.lock().await;
        assert!(
            nl.iter().any(|c| c.starts_with("add_ip_rule:192.168.1.10")),
            "Default → Tunnel should add an ip rule: {nl:?}"
        );
    }

    // Persist the reset outside the routing service, then deliver the
    // event trigger.
    ts.system_config.set_default_policy("direct").await.unwrap();
    as_admin(ts.routing.handle_default_policy_changed())
        .await
        .unwrap();

    assert_eq!(
        as_admin(ts.routing.default_policy()).await.unwrap(),
        "direct",
        "in-memory policy must catch up with the persisted change"
    );
    let nl = ts.netlink_calls.lock().await;
    assert!(
        nl.iter()
            .any(|c| c.starts_with("remove_ip_rule:192.168.1.10")),
        "Default-ruled device must be re-routed off the removed policy tunnel: {nl:?}"
    );
}

#[tokio::test]
async fn handle_default_policy_changed_is_idempotent_when_policy_unchanged() {
    // A redundant trigger (set_default_policy's own event after the
    // cache was updated inline, or the listener's resync after event
    // lag) must not disturb kernel state: the sweep re-runs but
    // apply_rule's short-circuit sees the already-applied target and
    // makes no kernel calls.
    let d1 = sample_device(device_id_1(), "192.168.1.10");
    let mut rules = HashMap::new();
    rules.insert(
        DEVICE_1_ID.to_owned(),
        RoutingRule {
            device_id: device_id_1(),
            target: RoutingTarget::Default,
            created_by: RuleCreator::User,
        },
    );
    let ts = setup_with_devices_and_tunnel(
        vec![d1],
        rules,
        Some(sample_tunnel(tunnel_id_1(), "wg_ward0", TunnelStatus::Up)),
        Some(sample_tunnel_config(vec!["1.1.1.1".to_owned()])),
        tunnel_id_1().to_string(),
    );

    as_admin(
        ts.routing
            .apply_rule(device_id_1(), "192.168.1.10", &RoutingTarget::Default),
    )
    .await
    .unwrap();
    let baseline = ts.netlink_calls.lock().await.len();

    as_admin(ts.routing.handle_default_policy_changed())
        .await
        .unwrap();

    assert_eq!(
        ts.netlink_calls.lock().await.len(),
        baseline,
        "a redundant policy trigger must not touch kernel state"
    );
    assert_eq!(
        as_admin(ts.routing.default_policy()).await.unwrap(),
        tunnel_id_1().to_string(),
    );
}
