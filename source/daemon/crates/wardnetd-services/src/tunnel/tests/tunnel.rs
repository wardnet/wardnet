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
use crate::jobs::{JobService, JobServiceImpl};
use crate::stats::buffer::StatsBuffer;
use crate::stats::meter::Meter;
use crate::tunnel::exit_probe::{ExitInfo, ProbeError, TunnelExitProbe};
use crate::tunnel::interface::{CreateTunnelParams, TunnelInterface, TunnelStats};
use crate::tunnel::key_store::KeyStore;
use crate::tunnel::latency_prober::{LatencyProbeError, TunnelLatencyProber};
use crate::tunnel::service::summarize_latency;
use crate::tunnel::throughput_tester::{ThroughputError, ThroughputMeasurement, ThroughputTester};
use crate::vpn::resolver::{EmptyServerListError, ServerResolver};
use crate::{TunnelService, TunnelServiceImpl};
use wardnet_common::speed_test::TunnelSpeedTestResult;
use wardnet_common::tunnel::BestServerSelector;
use wardnetd_data::repository::tunnel::TunnelRow;
use wardnetd_data::repository::tunnel_speed_test::{SpeedTestRow, TunnelSpeedTestRepository};
use wardnetd_data::repository::{DeviceRepository, SystemConfigRepository, TunnelRepository};

/// Helper to create an admin auth context for tests.
pub(super) fn admin_ctx() -> AuthContext {
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

/// Stats persisted by `update_stats`; tracked separately because
/// `TunnelRow` is the insert DTO and doesn't carry live stats columns.
#[derive(Default, Clone)]
struct TunnelStatsRow {
    bytes_tx: i64,
    bytes_rx: i64,
    last_handshake: Option<chrono::DateTime<chrono::Utc>>,
}

pub(super) struct MockTunnelRepo {
    rows: Mutex<Vec<TunnelRow>>,
    stats: Mutex<std::collections::HashMap<String, TunnelStatsRow>>,
}

impl MockTunnelRepo {
    fn new() -> Self {
        Self {
            rows: Mutex::new(Vec::new()),
            stats: Mutex::new(std::collections::HashMap::new()),
        }
    }
}

#[async_trait]
impl TunnelRepository for MockTunnelRepo {
    async fn find_all(&self) -> anyhow::Result<Vec<Tunnel>> {
        let rows = self.rows.lock().unwrap();
        let stats = self.stats.lock().unwrap();
        rows.iter().map(|r| row_to_tunnel(r, &stats)).collect()
    }

    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<Tunnel>> {
        let rows = self.rows.lock().unwrap();
        let stats = self.stats.lock().unwrap();
        let row = rows.iter().find(|r| r.id == id);
        row.map(|r| row_to_tunnel(r, &stats)).transpose()
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
                    override_default_dns: r.override_default_dns,
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

    async fn update_dns_override(&self, id: &str, value: bool) -> anyhow::Result<()> {
        let mut rows = self.rows.lock().unwrap();
        if let Some(r) = rows.iter_mut().find(|r| r.id == id) {
            r.override_default_dns = value;
        }
        Ok(())
    }

    async fn update_stats(
        &self,
        id: &str,
        bytes_tx: i64,
        bytes_rx: i64,
        last_handshake: Option<&str>,
    ) -> anyhow::Result<()> {
        let parsed = last_handshake.map(str::parse).transpose()?;
        self.stats.lock().unwrap().insert(
            id.to_owned(),
            TunnelStatsRow {
                bytes_tx,
                bytes_rx,
                last_handshake: parsed,
            },
        );
        Ok(())
    }

    async fn update_endpoint(
        &self,
        id: &str,
        endpoint: &str,
        peer_config_json: &str,
        server_name: &str,
        resolved_at: &str,
    ) -> anyhow::Result<()> {
        let mut rows = self.rows.lock().unwrap();
        if let Some(r) = rows.iter_mut().find(|r| r.id == id) {
            r.endpoint = endpoint.to_owned();
            r.peer_config = peer_config_json.to_owned();
            r.resolved_server_name = Some(server_name.to_owned());
            r.endpoint_resolved_at = Some(resolved_at.to_owned());
        }
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

/// Convert a `TunnelRow` into a `Tunnel` for mock responses, layering in
/// the latest stats observation if one exists.
fn row_to_tunnel(
    r: &TunnelRow,
    stats_map: &std::collections::HashMap<String, TunnelStatsRow>,
) -> anyhow::Result<Tunnel> {
    let parsed_status = match r.status.as_str() {
        "up" => TunnelStatus::Up,
        "connecting" => TunnelStatus::Connecting,
        "reconnecting" => TunnelStatus::Reconnecting,
        _ => TunnelStatus::Down,
    };
    let s = stats_map.get(&r.id).cloned().unwrap_or_default();
    Ok(Tunnel {
        id: r.id.parse()?,
        label: r.label.clone(),
        country_code: r.country_code.clone(),
        provider: r.provider.clone(),
        interface_name: r.interface_name.clone(),
        endpoint: r.endpoint.clone(),
        status: parsed_status,
        last_handshake: s.last_handshake,
        bytes_tx: s.bytes_tx.cast_unsigned(),
        bytes_rx: s.bytes_rx.cast_unsigned(),
        created_at: chrono::Utc::now(),
        override_default_dns: r.override_default_dns,
        server_selector: r
            .server_selector_country
            .as_ref()
            .map(|c| BestServerSelector { country: c.clone() }),
        resolved_server_name: r.resolved_server_name.clone(),
        endpoint_resolved_at: r
            .endpoint_resolved_at
            .as_deref()
            .and_then(|s| s.parse().ok()),
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
pub(super) struct MockTunnelInterface {
    created: Mutex<Vec<String>>,
    brought_up: Mutex<Vec<String>>,
    torn_down: Mutex<Vec<String>>,
    removed: Mutex<Vec<String>>,
    /// Names of interfaces currently considered present in the kernel.
    /// `create` adds, `remove` deletes — drives `list()`.
    live: Mutex<std::collections::HashSet<String>>,
    stats_behavior: Mutex<StatsBehavior>,
}

impl MockTunnelInterface {
    fn new() -> Self {
        Self {
            created: Mutex::new(Vec::new()),
            brought_up: Mutex::new(Vec::new()),
            torn_down: Mutex::new(Vec::new()),
            removed: Mutex::new(Vec::new()),
            live: Mutex::new(std::collections::HashSet::new()),
            stats_behavior: Mutex::new(StatsBehavior::None),
        }
    }

    pub(super) fn set_stats(&self, stats: TunnelStats) {
        *self.stats_behavior.lock().unwrap() = StatsBehavior::Some(stats);
    }

    pub(super) fn created_count(&self) -> usize {
        self.created.lock().unwrap().len()
    }

    pub(super) fn torn_down_count(&self) -> usize {
        self.torn_down.lock().unwrap().len()
    }

    fn set_stats_error(&self) {
        *self.stats_behavior.lock().unwrap() = StatsBehavior::Err;
    }

    /// Simulate an external event that drops the kernel iface (kernel
    /// reboot, `modprobe -r wireguard`, `ip link delete`) without going
    /// through the service's tear-down path.
    fn drop_iface(&self, name: &str) {
        self.live.lock().unwrap().remove(name);
    }
}

#[async_trait]
impl TunnelInterface for MockTunnelInterface {
    async fn create(&self, params: CreateTunnelParams) -> anyhow::Result<()> {
        self.live
            .lock()
            .unwrap()
            .insert(params.interface_name.clone());
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
        self.live.lock().unwrap().remove(interface_name);
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
        Ok(self.live.lock().unwrap().iter().cloned().collect())
    }
}

// -- Mock TunnelExitProbe -------------------------------------------------

enum ProbeBehavior {
    Ok(ExitInfo),
    Err(ProbeError),
}

struct MockTunnelExitProbe {
    behavior: Mutex<ProbeBehavior>,
    calls: Mutex<Vec<String>>,
    delay_ms: std::sync::atomic::AtomicU64,
}

impl MockTunnelExitProbe {
    fn new() -> Self {
        Self {
            behavior: Mutex::new(ProbeBehavior::Ok(ExitInfo {
                ip: "198.51.100.7".to_owned(),
                country_code: "SE".to_owned(),
            })),
            calls: Mutex::new(Vec::new()),
            delay_ms: std::sync::atomic::AtomicU64::new(0),
        }
    }

    fn set_ok(&self, ip: &str, country_code: &str) {
        *self.behavior.lock().unwrap() = ProbeBehavior::Ok(ExitInfo {
            ip: ip.to_owned(),
            country_code: country_code.to_owned(),
        });
    }

    fn set_err(&self, err: ProbeError) {
        *self.behavior.lock().unwrap() = ProbeBehavior::Err(err);
    }

    fn set_delay(&self, ms: u64) {
        self.delay_ms.store(ms, std::sync::atomic::Ordering::SeqCst);
    }

    fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

#[async_trait]
impl TunnelExitProbe for MockTunnelExitProbe {
    async fn probe(&self, interface: &str) -> Result<ExitInfo, ProbeError> {
        self.calls.lock().unwrap().push(interface.to_owned());
        let delay_ms = self.delay_ms.load(std::sync::atomic::Ordering::SeqCst);
        if delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        }
        match &*self.behavior.lock().unwrap() {
            ProbeBehavior::Ok(info) => Ok(info.clone()),
            ProbeBehavior::Err(ProbeError::Connect(m)) => Err(ProbeError::Connect(m.clone())),
            ProbeBehavior::Err(ProbeError::Parse(m)) => Err(ProbeError::Parse(m.clone())),
            ProbeBehavior::Err(ProbeError::Timeout(ms)) => Err(ProbeError::Timeout(*ms)),
            ProbeBehavior::Err(ProbeError::Unsupported(m)) => {
                Err(ProbeError::Unsupported(m.clone()))
            }
        }
    }
}

// -- Mock TunnelLatencyProber ---------------------------------------------

enum LatencyBehavior {
    Ok(u64),
    Err(LatencyProbeError),
}

pub(super) struct MockTunnelLatencyProber {
    behavior: Mutex<LatencyBehavior>,
    calls: Mutex<Vec<String>>,
}

impl MockTunnelLatencyProber {
    fn new() -> Self {
        Self {
            behavior: Mutex::new(LatencyBehavior::Ok(42)),
            calls: Mutex::new(Vec::new()),
        }
    }

    #[allow(dead_code)]
    fn set_ok(&self, rtt_ms: u64) {
        *self.behavior.lock().unwrap() = LatencyBehavior::Ok(rtt_ms);
    }

    #[allow(dead_code)]
    pub(super) fn set_err(&self, err: LatencyProbeError) {
        *self.behavior.lock().unwrap() = LatencyBehavior::Err(err);
    }

    #[allow(dead_code)]
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl TunnelLatencyProber for MockTunnelLatencyProber {
    async fn probe(&self, interface: Option<&str>) -> Result<u64, LatencyProbeError> {
        self.calls
            .lock()
            .unwrap()
            .push(interface.unwrap_or("<direct>").to_owned());
        match &*self.behavior.lock().unwrap() {
            LatencyBehavior::Ok(rtt) => Ok(*rtt),
            LatencyBehavior::Err(LatencyProbeError::Probe(m)) => {
                Err(LatencyProbeError::Probe(m.clone()))
            }
            LatencyBehavior::Err(LatencyProbeError::Timeout(ms)) => {
                Err(LatencyProbeError::Timeout(*ms))
            }
            LatencyBehavior::Err(LatencyProbeError::Unsupported(m)) => {
                Err(LatencyProbeError::Unsupported(m.clone()))
            }
        }
    }
}

// -- Mock ThroughputTester ------------------------------------------------

/// Deterministic throughput tester. Records each leg (interface = `None`
/// for direct, `Some(iface)` for tunnel) and can be made to fail a chosen
/// leg or delay so an in-flight run overlaps with a second request.
pub(super) struct MockThroughputTester {
    direct_mbps: Mutex<f64>,
    tunnel_mbps: Mutex<f64>,
    fail_direct: Mutex<bool>,
    fail_tunnel: Mutex<bool>,
    delay_ms: Mutex<u64>,
    calls: Mutex<Vec<Option<String>>>,
}

impl MockThroughputTester {
    pub(super) fn new() -> Self {
        Self {
            direct_mbps: Mutex::new(94.0),
            tunnel_mbps: Mutex::new(85.0),
            fail_direct: Mutex::new(false),
            fail_tunnel: Mutex::new(false),
            delay_ms: Mutex::new(0),
            calls: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn set_fail_tunnel(&self, v: bool) {
        *self.fail_tunnel.lock().unwrap() = v;
    }

    pub(super) fn set_delay(&self, ms: u64) {
        *self.delay_ms.lock().unwrap() = ms;
    }

    pub(super) fn calls(&self) -> Vec<Option<String>> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl ThroughputTester for MockThroughputTester {
    async fn download(
        &self,
        interface: Option<&str>,
    ) -> Result<ThroughputMeasurement, ThroughputError> {
        self.calls
            .lock()
            .unwrap()
            .push(interface.map(str::to_owned));
        let delay = *self.delay_ms.lock().unwrap();
        if delay > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
        }
        let is_tunnel = interface.is_some();
        let fail = if is_tunnel {
            *self.fail_tunnel.lock().unwrap()
        } else {
            *self.fail_direct.lock().unwrap()
        };
        if fail {
            return Err(ThroughputError::Download("mock leg failure".to_owned()));
        }
        let mbps = if is_tunnel {
            *self.tunnel_mbps.lock().unwrap()
        } else {
            *self.direct_mbps.lock().unwrap()
        };
        Ok(ThroughputMeasurement { mbps })
    }
}

// -- Mock TunnelSpeedTestRepository ---------------------------------------

/// In-memory speed test repository capturing inserted rows.
pub(super) struct MockSpeedTestRepo {
    rows: Mutex<Vec<SpeedTestRow>>,
}

impl MockSpeedTestRepo {
    pub(super) fn new() -> Self {
        Self {
            rows: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn count(&self) -> usize {
        self.rows.lock().unwrap().len()
    }

    pub(super) fn rows(&self) -> Vec<SpeedTestRow> {
        self.rows.lock().unwrap().clone()
    }
}

#[async_trait]
impl TunnelSpeedTestRepository for MockSpeedTestRepo {
    async fn insert(&self, row: &SpeedTestRow) -> anyhow::Result<()> {
        self.rows.lock().unwrap().push(row.clone());
        Ok(())
    }

    async fn find_recent(
        &self,
        tunnel_id: &str,
        limit: i64,
    ) -> anyhow::Result<Vec<TunnelSpeedTestResult>> {
        let mut rows: Vec<SpeedTestRow> = self
            .rows
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.tunnel_id == tunnel_id)
            .cloned()
            .collect();
        // Newest first.
        rows.sort_by(|a, b| b.tested_at.cmp(&a.tested_at));
        rows.truncate(usize::try_from(limit).unwrap_or(0));
        rows.into_iter()
            .map(|r| {
                Ok(TunnelSpeedTestResult {
                    id: r.id.parse()?,
                    tunnel_id: r.tunnel_id.parse()?,
                    direct_throughput_mbps: r.direct_throughput_mbps,
                    tunnel_throughput_mbps: r.tunnel_throughput_mbps,
                    direct_latency_ms: r.direct_latency_ms,
                    tunnel_latency_ms: r.tunnel_latency_ms,
                    direct_jitter_ms: r.direct_jitter_ms,
                    tunnel_jitter_ms: r.tunnel_jitter_ms,
                    tested_at: r.tested_at.parse()?,
                })
            })
            .collect()
    }
}

// -- Mock SystemConfigRepository -------------------------------------------

/// In-memory key-value mock backing the `default_policy` check that
/// `delete_tunnel` performs before tearing a tunnel down.
pub(super) struct MockSystemConfigRepo {
    store: Mutex<std::collections::HashMap<String, String>>,
}

impl MockSystemConfigRepo {
    fn new() -> Self {
        Self {
            store: Mutex::new(std::collections::HashMap::new()),
        }
    }
}

#[async_trait]
impl SystemConfigRepository for MockSystemConfigRepo {
    async fn get(&self, key: &str) -> anyhow::Result<Option<String>> {
        Ok(self.store.lock().unwrap().get(key).cloned())
    }

    async fn set(&self, key: &str, value: &str) -> anyhow::Result<()> {
        self.store
            .lock()
            .unwrap()
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }

    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        self.store.lock().unwrap().remove(key);
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
    async fn update_last_seen_and_ip(
        &self,
        _id: &str,
        _ip: &str,
        _ts: &str,
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
    async fn find_all_rules(&self) -> anyhow::Result<Vec<RoutingRule>> {
        Ok(vec![])
    }
    async fn upsert_user_rule(&self, _id: &str, _json: &str, _now: &str) -> anyhow::Result<()> {
        Ok(())
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
    async fn update_admin_locked(&self, _id: &str, _locked: bool) -> anyhow::Result<()> {
        Ok(())
    }
    async fn assign_zone(&self, _device_id: &str, _zone_id: &str) -> anyhow::Result<bool> {
        Ok(true)
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
    async fn update_last_seen_and_ip(
        &self,
        _id: &str,
        _ip: &str,
        _ts: &str,
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
    async fn find_all_rules(&self) -> anyhow::Result<Vec<RoutingRule>> {
        Ok(vec![])
    }
    async fn upsert_user_rule(&self, _id: &str, _json: &str, _now: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn find_devices_for_tunnel(&self, _tid: &str) -> anyhow::Result<Vec<Device>> {
        Ok(vec![])
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
    async fn assign_zone(&self, _device_id: &str, _zone_id: &str) -> anyhow::Result<bool> {
        Ok(true)
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

// -- Helpers --------------------------------------------------------------

// -- Mock ServerResolver --------------------------------------------------

struct MockServerResolver;

#[async_trait]
impl ServerResolver for MockServerResolver {
    async fn resolve(
        &self,
        _provider_id: &str,
        _selector: &BestServerSelector,
        _port: u16,
    ) -> anyhow::Result<Option<(String, String)>> {
        Ok(None)
    }
}

// -- Configurable resolver for bring_up branch tests ----------------------

enum FakeResolveResult {
    Success(String, String),
    EmptyList { country: String, provider: String },
    Transient(String),
}

struct FakeServerResolver {
    result: FakeResolveResult,
}

#[async_trait]
impl ServerResolver for FakeServerResolver {
    async fn resolve(
        &self,
        _provider_id: &str,
        _selector: &BestServerSelector,
        _port: u16,
    ) -> anyhow::Result<Option<(String, String)>> {
        match &self.result {
            FakeResolveResult::Success(ep, name) => Ok(Some((ep.clone(), name.clone()))),
            FakeResolveResult::EmptyList { country, provider } => {
                Err(anyhow::Error::new(EmptyServerListError {
                    country: country.clone(),
                    provider: provider.clone(),
                }))
            }
            FakeResolveResult::Transient(msg) => Err(anyhow::anyhow!("{msg}")),
        }
    }
}

pub(super) struct TestHarness {
    pub(super) svc: Arc<TunnelServiceImpl>,
    pub(super) tunnels: Arc<MockTunnelRepo>,
    pub(super) tunnel_iface: Arc<MockTunnelInterface>,
    pub(super) system_config: Arc<MockSystemConfigRepo>,
    events: Arc<MockEventPublisher>,
    keys: Arc<MockKeyStore>,
    stats_buffer: Arc<StatsBuffer>,
    exit_probe: Arc<MockTunnelExitProbe>,
    pub(super) latency_prober: Arc<MockTunnelLatencyProber>,
    pub(super) throughput_tester: Arc<MockThroughputTester>,
    pub(super) speed_test_repo: Arc<MockSpeedTestRepo>,
    pub(super) jobs: Arc<dyn JobService>,
}

/// Single construction point for the tunnel-service test harness; the
/// named entry points below vary only the dependency a test cares about.
fn build_harness_inner(
    device_repo: Arc<dyn DeviceRepository>,
    server_resolver: Arc<dyn ServerResolver>,
) -> TestHarness {
    let repo = Arc::new(MockTunnelRepo::new());
    let tunnel_iface = Arc::new(MockTunnelInterface::new());
    let keys = Arc::new(MockKeyStore::new());
    let system_config = Arc::new(MockSystemConfigRepo::new());
    let events = Arc::new(MockEventPublisher::new());
    let exit_probe = Arc::new(MockTunnelExitProbe::new());
    let latency_prober = Arc::new(MockTunnelLatencyProber::new());
    let stats_buffer = StatsBuffer::new();
    let meter = Arc::new(Meter::new(stats_buffer.clone()));

    let throughput_tester = Arc::new(MockThroughputTester::new());
    let speed_test_repo = Arc::new(MockSpeedTestRepo::new());
    let jobs: Arc<dyn JobService> = JobServiceImpl::new();

    let svc = TunnelServiceImpl::with_key_store(
        repo.clone(),
        device_repo,
        system_config.clone(),
        tunnel_iface.clone(),
        exit_probe.clone(),
        latency_prober.clone(),
        throughput_tester.clone(),
        keys.clone(),
        events.clone(),
        meter,
        server_resolver,
        jobs.clone(),
        speed_test_repo.clone(),
        3,
    );

    TestHarness {
        svc: Arc::new(svc),
        tunnels: repo,
        tunnel_iface,
        system_config,
        events,
        keys,
        stats_buffer,
        exit_probe,
        latency_prober,
        throughput_tester,
        speed_test_repo,
        jobs,
    }
}

pub(super) fn build_harness() -> TestHarness {
    build_harness_inner(
        Arc::new(MockDeviceRepoForTunnel),
        Arc::new(MockServerResolver),
    )
}

fn build_harness_with_device_repo(device_repo: Arc<dyn DeviceRepository>) -> TestHarness {
    build_harness_inner(device_repo, Arc::new(MockServerResolver))
}

fn build_harness_with_resolver(server_resolver: Arc<dyn ServerResolver>) -> TestHarness {
    build_harness_inner(Arc::new(MockDeviceRepoForTunnel), server_resolver)
}

fn sample_request() -> CreateTunnelRequest {
    CreateTunnelRequest {
        label: "Sweden VPN".to_owned(),
        country_code: "SE".to_owned(),
        provider: Some("Mullvad".to_owned()),
        config: SAMPLE_CONF.to_owned(),
        server_selector: None,
        resolved_server_name: None,
    }
}

fn sample_request_with_selector() -> CreateTunnelRequest {
    CreateTunnelRequest {
        label: "Sweden VPN".to_owned(),
        country_code: "SE".to_owned(),
        provider: Some("nordvpn".to_owned()),
        config: SAMPLE_CONF.to_owned(),
        server_selector: Some(BestServerSelector {
            country: "SE".to_owned(),
        }),
        resolved_server_name: Some("Sweden #1".to_owned()),
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
        server_selector: None,
        resolved_server_name: None,
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

    // Verify TunnelConnecting event was published — the tunnel only flips
    // to `Up` once the health-check loop observes a handshake.
    let events = h.events.published_events();
    assert_eq!(events.len(), 1);
    assert!(
        matches!(&events[0], WardnetEvent::TunnelConnecting { tunnel_id, .. } if *tunnel_id == id)
    );

    // Verify tunnel status is now connecting.
    let tunnel = auth_context::with_context(admin_ctx(), h.svc.get_tunnel(id))
        .await
        .unwrap();
    assert_eq!(tunnel.status, TunnelStatus::Connecting);
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
async fn bring_up_when_iface_already_configured_is_noop() {
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    // Bring up the first time — status becomes `Connecting`.
    auth_context::with_context(admin_ctx(), h.svc.bring_up(id))
        .await
        .unwrap();

    // Clear events from the first bring_up.
    h.events.events.lock().unwrap().clear();

    // Bring up again -- should be a no-op because the kernel iface is
    // already configured.
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
async fn delete_tunnel_resets_default_policy_pointing_at_it() {
    // Deleting the tunnel that *is* the global default policy must reset
    // the policy to "direct" and announce the change — otherwise the
    // config keeps a dangling tunnel UUID and every `Default`-ruled
    // device silently degrades to direct on its next resolve.
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    h.system_config
        .set_default_policy(&id.to_string())
        .await
        .unwrap();

    auth_context::with_context(admin_ctx(), h.svc.delete_tunnel(id))
        .await
        .unwrap();

    assert_eq!(
        h.system_config
            .get_default_policy()
            .await
            .unwrap()
            .as_deref(),
        Some("direct"),
        "default policy must be reset to direct when its tunnel is deleted"
    );
    let events = h.events.published_events();
    assert!(
        events.iter().any(|e| matches!(
            e,
            WardnetEvent::DefaultPolicyChanged { policy, .. } if policy == "direct"
        )),
        "deletion of the default-policy tunnel must publish DefaultPolicyChanged: {events:?}"
    );
}

#[tokio::test]
async fn delete_tunnel_resets_default_policy_stored_in_non_canonical_form() {
    // set_default_policy stores the admin-supplied string verbatim and the
    // UUID parser accepts non-canonical encodings, so the policy can
    // reference this tunnel as e.g. an uppercase UUID. The reset must
    // match by parsed identity, not by exact string.
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    h.system_config
        .set_default_policy(&id.to_string().to_uppercase())
        .await
        .unwrap();

    auth_context::with_context(admin_ctx(), h.svc.delete_tunnel(id))
        .await
        .unwrap();

    assert_eq!(
        h.system_config
            .get_default_policy()
            .await
            .unwrap()
            .as_deref(),
        Some("direct"),
        "a non-canonically encoded default policy must still be reset"
    );
    let events = h.events.published_events();
    assert!(
        events.iter().any(|e| matches!(
            e,
            WardnetEvent::DefaultPolicyChanged { policy, .. } if policy == "direct"
        )),
        "the reset must publish DefaultPolicyChanged: {events:?}"
    );
}

#[tokio::test]
async fn delete_tunnel_leaves_unrelated_default_policy_untouched() {
    // Deleting a tunnel that is NOT the default policy must not rewrite
    // the policy or publish a policy-change event.
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    let other_tunnel = Uuid::new_v4().to_string();
    h.system_config
        .set_default_policy(&other_tunnel)
        .await
        .unwrap();

    auth_context::with_context(admin_ctx(), h.svc.delete_tunnel(id))
        .await
        .unwrap();

    assert_eq!(
        h.system_config
            .get_default_policy()
            .await
            .unwrap()
            .as_deref(),
        Some(other_tunnel.as_str()),
        "an unrelated default policy must survive tunnel deletion"
    );
    let events = h.events.published_events();
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, WardnetEvent::DefaultPolicyChanged { .. })),
        "deleting a non-default-policy tunnel must not publish DefaultPolicyChanged: {events:?}"
    );
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

/// Helper: bring tunnel up + drive a stats observation through the iface
/// mock, mimicking what the stats loop would persist into the DB.
async fn bring_up_then_observe_handshake(
    h: &TestHarness,
    id: Uuid,
    handshake: Option<chrono::DateTime<chrono::Utc>>,
) {
    auth_context::with_context(admin_ctx(), h.svc.bring_up(id))
        .await
        .unwrap();
    h.tunnel_iface.set_stats(TunnelStats {
        bytes_tx: 0,
        bytes_rx: 0,
        last_handshake: handshake,
    });
    h.svc.collect_stats().await.unwrap();
    h.events.events.lock().unwrap().clear();
}

#[tokio::test]
async fn collect_stats_picks_up_connecting_tunnels() {
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    // After bring_up the status is `Connecting` — not yet `Up`.
    auth_context::with_context(admin_ctx(), h.svc.bring_up(id))
        .await
        .unwrap();
    h.events.events.lock().unwrap().clear();

    h.tunnel_iface.set_stats(TunnelStats {
        bytes_tx: 64,
        bytes_rx: 0,
        last_handshake: None,
    });
    h.svc.collect_stats().await.unwrap();

    let events = h.events.published_events();
    assert!(
        events.iter().any(|e| matches!(
            e,
            WardnetEvent::TunnelStatsUpdated {
                tunnel_id, status, ..
            } if *tunnel_id == id && *status == TunnelStatus::Connecting
        )),
        "expected TunnelStatsUpdated with Connecting status",
    );
}

#[tokio::test]
async fn collect_stats_does_not_change_status() {
    // Separation-of-concerns guarantee: collect_stats is a pure observer
    // and must never mutate tunnel status. Status transitions are owned
    // by run_health_check.
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    auth_context::with_context(admin_ctx(), h.svc.bring_up(id))
        .await
        .unwrap();

    // A fresh handshake — the kind of signal that *would* have triggered
    // a status flip in the old code.
    let fresh = chrono::Utc::now() - chrono::Duration::seconds(10);
    h.tunnel_iface.set_stats(TunnelStats {
        bytes_tx: 0,
        bytes_rx: 0,
        last_handshake: Some(fresh),
    });

    h.svc.collect_stats().await.unwrap();

    // Status must still be `Connecting` — only run_health_check flips it.
    let tunnel = auth_context::with_context(admin_ctx(), h.svc.get_tunnel(id))
        .await
        .unwrap();
    assert_eq!(tunnel.status, TunnelStatus::Connecting);
}

#[tokio::test]
async fn run_health_check_connecting_to_up_on_handshake() {
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    let fresh = chrono::Utc::now() - chrono::Duration::seconds(10);
    bring_up_then_observe_handshake(&h, id, Some(fresh)).await;

    h.svc.run_health_check().await.unwrap();

    let tunnel = auth_context::with_context(admin_ctx(), h.svc.get_tunnel(id))
        .await
        .unwrap();
    assert_eq!(tunnel.status, TunnelStatus::Up);

    let events = h.events.published_events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, WardnetEvent::TunnelUp { tunnel_id, .. } if *tunnel_id == id)),
        "expected TunnelUp event",
    );
}

#[tokio::test]
async fn run_health_check_connecting_stays_connecting_without_handshake() {
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    bring_up_then_observe_handshake(&h, id, None).await;

    h.svc.run_health_check().await.unwrap();

    let tunnel = auth_context::with_context(admin_ctx(), h.svc.get_tunnel(id))
        .await
        .unwrap();
    assert_eq!(tunnel.status, TunnelStatus::Connecting);

    let events = h.events.published_events();
    assert!(
        events
            .iter()
            .all(|e| !matches!(e, WardnetEvent::TunnelUp { .. })
                && !matches!(e, WardnetEvent::TunnelReconnecting { .. })),
        "no transition events expected",
    );
}

#[tokio::test]
async fn run_health_check_up_to_reconnecting_when_handshake_stale() {
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    // Drive Connecting → Up via a fresh handshake.
    let fresh = chrono::Utc::now() - chrono::Duration::seconds(10);
    bring_up_then_observe_handshake(&h, id, Some(fresh)).await;
    h.svc.run_health_check().await.unwrap();
    assert_eq!(
        auth_context::with_context(admin_ctx(), h.svc.get_tunnel(id))
            .await
            .unwrap()
            .status,
        TunnelStatus::Up,
    );

    // Now simulate the handshake going stale — collect_stats persists the
    // older timestamp.
    let stale = chrono::Utc::now() - chrono::Duration::minutes(10);
    h.tunnel_iface.set_stats(TunnelStats {
        bytes_tx: 0,
        bytes_rx: 0,
        last_handshake: Some(stale),
    });
    h.svc.collect_stats().await.unwrap();
    h.events.events.lock().unwrap().clear();

    h.svc.run_health_check().await.unwrap();

    let tunnel = auth_context::with_context(admin_ctx(), h.svc.get_tunnel(id))
        .await
        .unwrap();
    assert_eq!(tunnel.status, TunnelStatus::Reconnecting);

    let events = h.events.published_events();
    assert!(
        events.iter().any(|e| matches!(
            e,
            WardnetEvent::TunnelReconnecting { tunnel_id, .. } if *tunnel_id == id
        )),
        "expected TunnelReconnecting event",
    );
}

#[tokio::test]
async fn run_health_check_up_to_reconnecting_when_handshake_absent() {
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    // Drive Connecting → Up via a fresh handshake.
    let fresh = chrono::Utc::now() - chrono::Duration::seconds(10);
    bring_up_then_observe_handshake(&h, id, Some(fresh)).await;
    h.svc.run_health_check().await.unwrap();

    // Handshake disappears (e.g. iface state lost between observations).
    h.tunnels
        .update_stats(&id.to_string(), 0, 0, None)
        .await
        .unwrap();
    h.events.events.lock().unwrap().clear();

    h.svc.run_health_check().await.unwrap();

    let tunnel = auth_context::with_context(admin_ctx(), h.svc.get_tunnel(id))
        .await
        .unwrap();
    assert_eq!(tunnel.status, TunnelStatus::Reconnecting);
}

#[tokio::test]
async fn run_health_check_reconnecting_to_up_on_recovery() {
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    // Drive the tunnel into Reconnecting.
    bring_up_then_observe_handshake(&h, id, None).await;
    // Force status to `Reconnecting` directly to set up the test, without
    // waiting 3 min of wall-clock for the Up→Reconnecting transition.
    h.tunnels
        .update_status(&id.to_string(), "reconnecting")
        .await
        .unwrap();

    // Peer starts replying — fresh handshake observed.
    let fresh = chrono::Utc::now() - chrono::Duration::seconds(5);
    h.tunnel_iface.set_stats(TunnelStats {
        bytes_tx: 0,
        bytes_rx: 0,
        last_handshake: Some(fresh),
    });
    h.svc.collect_stats().await.unwrap();
    h.events.events.lock().unwrap().clear();

    h.svc.run_health_check().await.unwrap();

    let tunnel = auth_context::with_context(admin_ctx(), h.svc.get_tunnel(id))
        .await
        .unwrap();
    assert_eq!(tunnel.status, TunnelStatus::Up);

    let events = h.events.published_events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, WardnetEvent::TunnelUp { tunnel_id, .. } if *tunnel_id == id)),
        "expected TunnelUp event on recovery",
    );
}

#[tokio::test]
async fn run_health_check_no_active_tunnels_is_noop() {
    let h = build_harness();
    // No tunnels brought up; run_health_check iterates over an empty list.
    h.svc.run_health_check().await.unwrap();
}

#[tokio::test]
async fn run_health_check_iface_missing_flips_up_to_down() {
    // Issue #311: after a daemon restart or `modprobe -r wireguard`, the DB
    // still says `Up` while the kernel iface is gone. Health check must
    // reconcile and flip to `Down` so on-demand bring-up can pick it back up.
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    let fresh = chrono::Utc::now() - chrono::Duration::seconds(10);
    bring_up_then_observe_handshake(&h, id, Some(fresh)).await;
    h.svc.run_health_check().await.unwrap();
    assert_eq!(
        auth_context::with_context(admin_ctx(), h.svc.get_tunnel(id))
            .await
            .unwrap()
            .status,
        TunnelStatus::Up,
    );

    // Simulate an external event that drops the kernel iface without
    // touching DB state — daemon restart, kernel module unload, etc.
    h.tunnel_iface.drop_iface("wg_ward0");
    h.events.events.lock().unwrap().clear();

    h.svc.run_health_check().await.unwrap();

    let tunnel = auth_context::with_context(admin_ctx(), h.svc.get_tunnel(id))
        .await
        .unwrap();
    assert_eq!(tunnel.status, TunnelStatus::Down);

    let events = h.events.published_events();
    assert!(
        events.iter().any(|e| matches!(
            e,
            WardnetEvent::TunnelDown { tunnel_id, reason, .. }
                if *tunnel_id == id && reason == "interface absent"
        )),
        "expected TunnelDown event with 'interface absent' reason",
    );
}

#[tokio::test]
async fn run_health_check_iface_missing_flips_reconnecting_to_down() {
    // Same scenario as the daemon-restart case but where status is already
    // `Reconnecting` when the iface vanishes — the case in the original
    // bug report.
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    bring_up_then_observe_handshake(&h, id, None).await;
    h.tunnels
        .update_status(&id.to_string(), "reconnecting")
        .await
        .unwrap();

    h.tunnel_iface.drop_iface("wg_ward0");
    h.events.events.lock().unwrap().clear();

    h.svc.run_health_check().await.unwrap();

    assert_eq!(
        auth_context::with_context(admin_ctx(), h.svc.get_tunnel(id))
            .await
            .unwrap()
            .status,
        TunnelStatus::Down,
    );
}

#[tokio::test]
async fn run_health_check_stuck_reconnecting_triggers_recreate() {
    // When the iface is present but the peer hasn't replied for far longer
    // than the stale threshold, the tunnel may be wedged or the peer DNS
    // may have rotated. Health check must tear down + bring up to reset.
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    bring_up_then_observe_handshake(&h, id, None).await;
    h.tunnels
        .update_status(&id.to_string(), "reconnecting")
        .await
        .unwrap();

    // Persist a last_handshake well past the recovery threshold (15 min).
    let very_stale = chrono::Utc::now() - chrono::Duration::minutes(30);
    h.tunnels
        .update_stats(&id.to_string(), 0, 0, Some(&very_stale.to_rfc3339()))
        .await
        .unwrap();
    h.tunnel_iface.torn_down.lock().unwrap().clear();
    h.tunnel_iface.removed.lock().unwrap().clear();
    h.tunnel_iface.created.lock().unwrap().clear();
    h.tunnel_iface.brought_up.lock().unwrap().clear();
    h.events.events.lock().unwrap().clear();

    h.svc.run_health_check().await.unwrap();

    // Recreate path must have run: tear-down + remove + create + bring-up.
    assert_eq!(
        h.tunnel_iface.torn_down.lock().unwrap().len(),
        1,
        "expected exactly one tear_down call"
    );
    assert_eq!(
        h.tunnel_iface.removed.lock().unwrap().len(),
        1,
        "expected exactly one remove call"
    );
    assert_eq!(
        h.tunnel_iface.created.lock().unwrap().len(),
        1,
        "expected exactly one create call"
    );
    assert_eq!(
        h.tunnel_iface.brought_up.lock().unwrap().len(),
        1,
        "expected exactly one bring_up call"
    );

    // Domain events must be published so subscribers (routing engine, web
    // UI) react to the recreate.
    let events = h.events.published_events();
    assert!(
        events.iter().any(|e| matches!(
            e,
            WardnetEvent::TunnelDown { tunnel_id, reason, .. }
                if *tunnel_id == id && reason == "stuck in reconnecting, recreating"
        )),
        "expected TunnelDown event with stuck-reconnecting reason",
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            WardnetEvent::TunnelConnecting { tunnel_id, .. } if *tunnel_id == id
        )),
        "expected TunnelConnecting event after recreate",
    );
}

#[tokio::test]
async fn run_health_check_reconnecting_within_threshold_no_recreate() {
    // Reconnecting with a stale-but-recent handshake should NOT trigger a
    // recreate yet — give the peer time to come back on its own.
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    bring_up_then_observe_handshake(&h, id, None).await;
    h.tunnels
        .update_status(&id.to_string(), "reconnecting")
        .await
        .unwrap();

    // 5 min old: past stale (3 min) but well below recovery (15 min).
    let stale_recent = chrono::Utc::now() - chrono::Duration::minutes(5);
    h.tunnels
        .update_stats(&id.to_string(), 0, 0, Some(&stale_recent.to_rfc3339()))
        .await
        .unwrap();
    h.tunnel_iface.torn_down.lock().unwrap().clear();
    h.tunnel_iface.removed.lock().unwrap().clear();

    h.svc.run_health_check().await.unwrap();

    assert!(
        h.tunnel_iface.torn_down.lock().unwrap().is_empty(),
        "tear_down must not be called for a tunnel still within the recovery threshold"
    );
    assert!(
        h.tunnel_iface.removed.lock().unwrap().is_empty(),
        "remove must not be called for a tunnel still within the recovery threshold"
    );
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
        server_selector: None,
        resolved_server_name: None,
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
        server_selector: None,
        resolved_server_name: None,
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

// -- Byte-delta recording -------------------------------------------------

/// Drain the stats buffer and return the (counter-only) values for the
/// `tunnel.bytes.tx` / `tunnel.bytes.rx` metrics with the given tunnel
/// label. Filters out any rows for other metrics or label sets.
fn drain_byte_deltas(h: &TestHarness, tunnel_id: Uuid) -> (f64, f64) {
    let labels = format!(r#"{{"tunnel_id":"{tunnel_id}"}}"#);
    let mut tx = 0.0_f64;
    let mut rx = 0.0_f64;
    for row in h.stats_buffer.drain() {
        if row.labels != labels {
            continue;
        }
        match row.metric.as_str() {
            "tunnel.bytes.tx" => tx += row.value,
            "tunnel.bytes.rx" => rx += row.value,
            _ => {}
        }
    }
    (tx, rx)
}

#[tokio::test]
async fn byte_deltas_first_observation_is_baseline_only() {
    let h = build_harness();
    let id = Uuid::new_v4();

    h.svc.record_byte_deltas(id, 1_000, 2_000).await;

    let (tx, rx) = drain_byte_deltas(&h, id);
    assert!(tx.abs() < f64::EPSILON);
    assert!(rx.abs() < f64::EPSILON);
}

#[tokio::test]
async fn byte_deltas_records_positive_increments() {
    let h = build_harness();
    let id = Uuid::new_v4();

    h.svc.record_byte_deltas(id, 1_000, 2_000).await;
    h.svc.record_byte_deltas(id, 6_000, 9_000).await;

    let (tx, rx) = drain_byte_deltas(&h, id);
    assert!((tx - 5_000.0).abs() < f64::EPSILON);
    assert!((rx - 7_000.0).abs() < f64::EPSILON);
}

#[tokio::test]
async fn byte_deltas_counter_reset_does_not_emit_delta() {
    let h = build_harness();
    let id = Uuid::new_v4();

    h.svc.record_byte_deltas(id, 10_000, 20_000).await;
    // Counter went *down* — kernel WireGuard counters reset on iface
    // recreate. The new value becomes the fresh baseline; we must not
    // emit a negative or fabricated delta.
    h.svc.record_byte_deltas(id, 4_200, 7_500).await;

    let (tx, rx) = drain_byte_deltas(&h, id);
    assert!(tx.abs() < f64::EPSILON);
    assert!(rx.abs() < f64::EPSILON);

    // The reset value is now baseline — the next monotonic observation
    // produces a delta against it.
    h.svc.record_byte_deltas(id, 5_200, 7_900).await;
    let (tx, rx) = drain_byte_deltas(&h, id);
    assert!((tx - 1_000.0).abs() < f64::EPSILON);
    assert!((rx - 400.0).abs() < f64::EPSILON);
}

// -- probe_latencies ------------------------------------------------------

fn insert_up_tunnel(h: &TestHarness, id: Uuid, interface_name: &str) {
    h.tunnels.rows.lock().unwrap().push(TunnelRow {
        id: id.to_string(),
        label: "test".to_owned(),
        country_code: "SE".to_owned(),
        provider: None,
        interface_name: interface_name.to_owned(),
        endpoint: "198.51.100.1:51820".to_owned(),
        status: "up".to_owned(),
        address: "[]".to_owned(),
        dns: "[]".to_owned(),
        peer_config: "{}".to_owned(),
        listen_port: None,
        override_default_dns: false,
        server_selector_country: None,
        resolved_server_name: None,
        endpoint_resolved_at: None,
    });
}

fn drain_latency_rtt(h: &TestHarness, tunnel_id: Uuid) -> Option<f64> {
    let labels = format!(r#"{{"tunnel_id":"{tunnel_id}"}}"#);
    for row in h.stats_buffer.drain() {
        if row.metric == "tunnel.latency.rtt_ms" && row.labels == labels {
            return Some(row.value);
        }
    }
    None
}

#[tokio::test]
async fn probe_latencies_emits_gauge_for_active_tunnel() {
    let h = build_harness();
    let id = Uuid::new_v4();
    insert_up_tunnel(&h, id, "wg_ward0");
    h.latency_prober.set_ok(57);

    h.svc.probe_latencies().await.unwrap();

    let rtt = drain_latency_rtt(&h, id).expect("expected one latency gauge row");
    assert!((rtt - 57.0).abs() < f64::EPSILON);
    assert_eq!(h.latency_prober.calls(), vec!["wg_ward0".to_owned()]);
}

#[tokio::test]
async fn probe_latencies_skips_down_tunnels() {
    let h = build_harness();
    let id = Uuid::new_v4();
    h.tunnels.rows.lock().unwrap().push(TunnelRow {
        id: id.to_string(),
        label: "test".to_owned(),
        country_code: "SE".to_owned(),
        provider: None,
        interface_name: "wg_ward0".to_owned(),
        endpoint: "198.51.100.1:51820".to_owned(),
        status: "down".to_owned(),
        address: "[]".to_owned(),
        dns: "[]".to_owned(),
        peer_config: "{}".to_owned(),
        listen_port: None,
        override_default_dns: false,
        server_selector_country: None,
        resolved_server_name: None,
        endpoint_resolved_at: None,
    });

    h.svc.probe_latencies().await.unwrap();

    assert!(h.latency_prober.calls().is_empty());
    assert!(drain_latency_rtt(&h, id).is_none());
}

#[tokio::test]
async fn probe_latencies_continues_after_single_failure() {
    let h = build_harness();
    let id_a = Uuid::new_v4();
    let id_b = Uuid::new_v4();
    insert_up_tunnel(&h, id_a, "wg_ward0");
    insert_up_tunnel(&h, id_b, "wg_ward1");
    h.latency_prober.set_err(LatencyProbeError::Timeout(1500));

    // Both interfaces should still get probed even though every call errs.
    h.svc.probe_latencies().await.unwrap();
    let calls = h.latency_prober.calls();
    assert_eq!(calls.len(), 2);
    assert!(calls.contains(&"wg_ward0".to_owned()));
    assert!(calls.contains(&"wg_ward1".to_owned()));
    assert!(drain_latency_rtt(&h, id_a).is_none());
    assert!(drain_latency_rtt(&h, id_b).is_none());
}

#[tokio::test]
async fn probe_latencies_short_circuits_on_unsupported() {
    let h = build_harness();
    let id_a = Uuid::new_v4();
    let id_b = Uuid::new_v4();
    insert_up_tunnel(&h, id_a, "wg_ward0");
    insert_up_tunnel(&h, id_b, "wg_ward1");
    h.latency_prober
        .set_err(LatencyProbeError::Unsupported("not linux".to_owned()));

    h.svc.probe_latencies().await.unwrap();

    // Platform-level error: stop iterating instead of pinging every
    // tunnel only to fail the same way.
    assert_eq!(h.latency_prober.calls().len(), 1);
}

// -- list_tunnel_devices --------------------------------------------------

#[tokio::test]
async fn list_tunnel_devices_returns_empty_for_known_tunnel() {
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    let result = auth_context::with_context(admin_ctx(), h.svc.list_tunnel_devices(id))
        .await
        .unwrap();
    assert!(result.devices.is_empty());
}

#[tokio::test]
async fn list_tunnel_devices_unknown_tunnel_returns_not_found() {
    let h = build_harness();
    let result =
        auth_context::with_context(admin_ctx(), h.svc.list_tunnel_devices(Uuid::new_v4())).await;
    assert!(matches!(result, Err(AppError::NotFound(_))));
}

#[tokio::test]
async fn list_tunnel_devices_anonymous_forbidden() {
    let h = build_harness();
    let result = auth_context::with_context(
        AuthContext::Anonymous,
        h.svc.list_tunnel_devices(Uuid::new_v4()),
    )
    .await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

// -- test_tunnel ----------------------------------------------------------

/// Helper: import a tunnel and return its id.
pub(super) async fn imported_tunnel_id(h: &TestHarness) -> Uuid {
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    resp.tunnel.id
}

#[tokio::test]
async fn test_tunnel_returns_result() {
    let h = build_harness();
    let id = imported_tunnel_id(&h).await;
    h.exit_probe.set_ok("203.0.113.7", "DE");
    // `bring_up_internal` flips status to `connecting`; for the test we
    // need `get_stats` to report a fresh handshake so the readiness gate
    // clears immediately.
    h.tunnel_iface.set_stats(TunnelStats {
        bytes_tx: 0,
        bytes_rx: 0,
        last_handshake: Some(chrono::Utc::now()),
    });

    let result = auth_context::with_context(admin_ctx(), h.svc.test_tunnel(id))
        .await
        .expect("test_tunnel should succeed");
    assert_eq!(result.tunnel_id, id);
    assert_eq!(result.exit_ip, "203.0.113.7");
    assert_eq!(result.country_code, "DE");
    assert_eq!(h.exit_probe.call_count(), 1);
}

#[tokio::test]
async fn test_tunnel_tears_down_when_was_down() {
    let h = build_harness();
    let id = imported_tunnel_id(&h).await;
    h.tunnel_iface.set_stats(TunnelStats {
        bytes_tx: 0,
        bytes_rx: 0,
        last_handshake: Some(chrono::Utc::now()),
    });

    auth_context::with_context(admin_ctx(), h.svc.test_tunnel(id))
        .await
        .unwrap();

    let torn_down = h.tunnel_iface.torn_down.lock().unwrap();
    assert!(
        !torn_down.is_empty(),
        "expected tear_down call when starting from Down"
    );
}

#[tokio::test]
async fn test_tunnel_leaves_up_when_was_up() {
    let h = build_harness();
    let id = imported_tunnel_id(&h).await;
    // Pre-mark the tunnel `up` so test_tunnel skips bring_up + tear_down.
    h.tunnels
        .update_status(&id.to_string(), "up")
        .await
        .unwrap();
    h.tunnel_iface.set_stats(TunnelStats {
        bytes_tx: 0,
        bytes_rx: 0,
        last_handshake: Some(chrono::Utc::now()),
    });

    auth_context::with_context(admin_ctx(), h.svc.test_tunnel(id))
        .await
        .unwrap();

    let torn_down = h.tunnel_iface.torn_down.lock().unwrap();
    assert!(
        torn_down.is_empty(),
        "expected no tear_down when tunnel was already up"
    );
    let brought_up = h.tunnel_iface.brought_up.lock().unwrap();
    assert!(
        brought_up.is_empty(),
        "expected no bring_up when tunnel was already up"
    );
}

#[tokio::test]
async fn test_tunnel_returns_conflict_when_test_in_flight() {
    let h = Arc::new(build_harness());
    let id = imported_tunnel_id(&h).await;
    h.tunnel_iface.set_stats(TunnelStats {
        bytes_tx: 0,
        bytes_rx: 0,
        last_handshake: Some(chrono::Utc::now()),
    });
    // Hold the first probe open long enough to overlap with the second.
    h.exit_probe.set_delay(300);

    // Spawn one in-flight call; before it finishes, kick off a second.
    let h1 = h.clone();
    let first = tokio::spawn(async move {
        auth_context::with_context(admin_ctx(), h1.svc.test_tunnel(id)).await
    });

    // Yield long enough that the first call is past `acquire_in_flight`.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let second = auth_context::with_context(admin_ctx(), h.svc.test_tunnel(id)).await;
    assert!(
        matches!(second, Err(AppError::Conflict(_))),
        "expected Conflict on second concurrent call, got {second:?}"
    );

    // Drain the first call so the test doesn't leak a task.
    first.await.unwrap().unwrap();
}

#[tokio::test]
async fn test_tunnel_handshake_timeout_returns_upstream_unavailable() {
    let h = build_harness();
    let id = imported_tunnel_id(&h).await;
    // Default `MockTunnelInterface::get_stats` returns `Ok(None)`, which
    // never satisfies the freshness check — the readiness gate must time
    // out within its budget.
    let started = std::time::Instant::now();
    let result = auth_context::with_context(admin_ctx(), h.svc.test_tunnel(id)).await;
    let elapsed = started.elapsed();
    assert!(
        matches!(result, Err(AppError::UpstreamUnavailable(_))),
        "expected UpstreamUnavailable on handshake timeout, got {result:?}"
    );
    // The poll budget is 3.5 s; allow generous slack so a slow CI host
    // doesn't fail the assertion while still catching a runaway sleep.
    assert!(
        elapsed < std::time::Duration::from_secs(6),
        "handshake timeout should not take more than ~3.5 s, took {elapsed:?}"
    );
}

#[tokio::test]
async fn test_tunnel_anonymous_forbidden() {
    let h = build_harness();
    let id = imported_tunnel_id(&h).await;
    let result = auth_context::with_context(AuthContext::Anonymous, h.svc.test_tunnel(id)).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn test_tunnel_probe_connect_error_maps_to_upstream_unavailable() {
    let h = build_harness();
    let id = imported_tunnel_id(&h).await;
    h.exit_probe
        .set_err(ProbeError::Connect("dns failed".to_owned()));
    h.tunnel_iface.set_stats(TunnelStats {
        bytes_tx: 0,
        bytes_rx: 0,
        last_handshake: Some(chrono::Utc::now()),
    });

    let result = auth_context::with_context(admin_ctx(), h.svc.test_tunnel(id)).await;
    assert!(matches!(result, Err(AppError::UpstreamUnavailable(_))));
}

#[tokio::test]
async fn test_tunnel_probe_timeout_maps_to_upstream_unavailable() {
    let h = build_harness();
    let id = imported_tunnel_id(&h).await;
    h.exit_probe.set_err(ProbeError::Timeout(1500));
    h.tunnel_iface.set_stats(TunnelStats {
        bytes_tx: 0,
        bytes_rx: 0,
        last_handshake: Some(chrono::Utc::now()),
    });

    let result = auth_context::with_context(admin_ctx(), h.svc.test_tunnel(id)).await;
    assert!(matches!(result, Err(AppError::UpstreamUnavailable(_))));
}

#[tokio::test]
async fn test_tunnel_probe_parse_error_maps_to_upstream_unavailable() {
    let h = build_harness();
    let id = imported_tunnel_id(&h).await;
    h.exit_probe
        .set_err(ProbeError::Parse("missing ip= field".to_owned()));
    h.tunnel_iface.set_stats(TunnelStats {
        bytes_tx: 0,
        bytes_rx: 0,
        last_handshake: Some(chrono::Utc::now()),
    });

    let result = auth_context::with_context(admin_ctx(), h.svc.test_tunnel(id)).await;
    assert!(matches!(result, Err(AppError::UpstreamUnavailable(_))));
}

#[tokio::test]
async fn test_tunnel_probe_unsupported_maps_to_internal() {
    let h = build_harness();
    let id = imported_tunnel_id(&h).await;
    h.exit_probe
        .set_err(ProbeError::Unsupported("not on linux".to_owned()));
    h.tunnel_iface.set_stats(TunnelStats {
        bytes_tx: 0,
        bytes_rx: 0,
        last_handshake: Some(chrono::Utc::now()),
    });

    let result = auth_context::with_context(admin_ctx(), h.svc.test_tunnel(id)).await;
    assert!(matches!(result, Err(AppError::Internal(_))));
}

#[tokio::test]
async fn test_tunnel_get_stats_error_maps_to_internal() {
    let h = build_harness();
    let id = imported_tunnel_id(&h).await;
    // `set_stats_error` makes `get_stats` return `Err`, which the
    // handshake-readiness loop must surface as `Internal` rather than
    // looping forever or falling through as a timeout.
    h.tunnel_iface.set_stats_error();

    let result = auth_context::with_context(admin_ctx(), h.svc.test_tunnel(id)).await;
    assert!(
        matches!(result, Err(AppError::Internal(_))),
        "expected Internal on stats Err, got {result:?}"
    );
}

// -- Re-resolution branch tests -------------------------------------------

#[tokio::test]
async fn bring_up_reresolves_best_server() {
    let resolver = Arc::new(FakeServerResolver {
        result: FakeResolveResult::Success(
            "198.51.100.99:51820".to_owned(),
            "Sweden #99".to_owned(),
        ),
    });
    let h = build_harness_with_resolver(resolver);

    let resp = auth_context::with_context(
        admin_ctx(),
        h.svc.import_tunnel(sample_request_with_selector()),
    )
    .await
    .unwrap();
    let id = resp.tunnel.id;

    auth_context::with_context(admin_ctx(), h.svc.bring_up(id))
        .await
        .unwrap();

    // Re-resolution must have persisted the new endpoint in the repo.
    let rows = h.tunnels.rows.lock().unwrap();
    assert_eq!(rows[0].endpoint, "198.51.100.99:51820");
    assert_eq!(rows[0].resolved_server_name.as_deref(), Some("Sweden #99"));
    assert!(rows[0].endpoint_resolved_at.is_some());
}

#[tokio::test]
async fn bring_up_empty_server_list_is_fatal() {
    let resolver = Arc::new(FakeServerResolver {
        result: FakeResolveResult::EmptyList {
            country: "SE".to_owned(),
            provider: "nordvpn".to_owned(),
        },
    });
    let h = build_harness_with_resolver(resolver);

    let resp = auth_context::with_context(
        admin_ctx(),
        h.svc.import_tunnel(sample_request_with_selector()),
    )
    .await
    .unwrap();
    let id = resp.tunnel.id;

    let result = auth_context::with_context(admin_ctx(), h.svc.bring_up(id)).await;
    assert!(
        matches!(result, Err(AppError::Internal(_))),
        "EmptyServerListError must be fatal, got {result:?}"
    );
}

#[tokio::test]
async fn bring_up_transient_resolver_error_uses_stored_endpoint() {
    let resolver = Arc::new(FakeServerResolver {
        result: FakeResolveResult::Transient("connection refused".to_owned()),
    });
    let h = build_harness_with_resolver(resolver);

    let resp = auth_context::with_context(
        admin_ctx(),
        h.svc.import_tunnel(sample_request_with_selector()),
    )
    .await
    .unwrap();
    let id = resp.tunnel.id;
    let original_endpoint = resp.tunnel.endpoint.clone();

    // Transient errors must not fail bring_up — fall back to stored endpoint.
    auth_context::with_context(admin_ctx(), h.svc.bring_up(id))
        .await
        .unwrap();

    // Stored endpoint must not have been overwritten.
    let rows = h.tunnels.rows.lock().unwrap();
    assert_eq!(rows[0].endpoint, original_endpoint);
}

#[tokio::test]
async fn bring_up_with_selector_and_resolver_returning_none_uses_stored_endpoint() {
    // MockServerResolver returns Ok(None) — "no provider registered".
    // bring_up must succeed and keep the stored endpoint unchanged.
    let resolver = Arc::new(MockServerResolver);
    let h = build_harness_with_resolver(resolver);

    let resp = auth_context::with_context(
        admin_ctx(),
        h.svc.import_tunnel(sample_request_with_selector()),
    )
    .await
    .unwrap();
    let id = resp.tunnel.id;
    let original_endpoint = resp.tunnel.endpoint.clone();

    auth_context::with_context(admin_ctx(), h.svc.bring_up(id))
        .await
        .unwrap();

    let rows = h.tunnels.rows.lock().unwrap();
    assert_eq!(rows[0].endpoint, original_endpoint);
}

// -- rebuild ------------------------------------------------------------------

#[tokio::test]
async fn rebuild_success() {
    let h = build_harness();
    let resp = auth_context::with_context(admin_ctx(), h.svc.import_tunnel(sample_request()))
        .await
        .unwrap();
    let id = resp.tunnel.id;

    // Bring the tunnel up so it is in `Connecting` state — rebuild has work to do.
    auth_context::with_context(admin_ctx(), h.svc.bring_up(id))
        .await
        .unwrap();

    // Clear the interface call counts so we can assert rebuild's own calls.
    h.tunnel_iface.torn_down.lock().unwrap().clear();
    h.tunnel_iface.removed.lock().unwrap().clear();
    h.tunnel_iface.created.lock().unwrap().clear();
    h.tunnel_iface.brought_up.lock().unwrap().clear();

    auth_context::with_context(admin_ctx(), h.svc.rebuild(id))
        .await
        .unwrap();

    // Rebuild must tear down the existing interface and bring a fresh one up.
    assert_eq!(
        h.tunnel_iface.torn_down.lock().unwrap().as_slice(),
        ["wg_ward0"]
    );
    assert_eq!(
        h.tunnel_iface.removed.lock().unwrap().as_slice(),
        ["wg_ward0"]
    );
    assert_eq!(
        h.tunnel_iface.created.lock().unwrap().as_slice(),
        ["wg_ward0"]
    );
    assert_eq!(
        h.tunnel_iface.brought_up.lock().unwrap().as_slice(),
        ["wg_ward0"]
    );
}

#[tokio::test]
async fn rebuild_not_found() {
    let h = build_harness();
    let result = auth_context::with_context(admin_ctx(), h.svc.rebuild(Uuid::new_v4())).await;
    assert!(matches!(result, Err(AppError::NotFound(_))));
}

// ── summarize_latency (pure) ───────────────────────────────

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn empty_is_zero() {
    let (median, jitter) = summarize_latency(&[]);
    assert!(approx(median, 0.0));
    assert!(approx(jitter, 0.0));
}

#[test]
fn single_sample_has_no_jitter() {
    let (median, jitter) = summarize_latency(&[42]);
    assert!(approx(median, 42.0));
    assert!(approx(jitter, 0.0));
}

#[test]
fn odd_count_takes_middle_and_sample_stddev() {
    // Sorted: [10, 20, 30] -> median 20; mean 20, sample variance
    // (100 + 0 + 100) / 2 = 100 -> stddev 10.
    let (median, jitter) = summarize_latency(&[10, 30, 20]);
    assert!(approx(median, 20.0));
    assert!(approx(jitter, 10.0));
}

#[test]
fn even_count_averages_the_two_middles() {
    // Sorted: [10, 20, 30, 40] -> median midpoint(20, 30) = 25.
    let (median, _) = summarize_latency(&[40, 10, 30, 20]);
    assert!(approx(median, 25.0));
}
