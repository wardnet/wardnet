use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;
use wardnet_common::api::LastShutdownState;
use wardnet_common::auth::AuthContext;

use crate::auth_context;
use crate::error::AppError;
use crate::system::{
    DhcpProbeOutcome, NetworkInspector, NetworkProbe, NetworkSnapshot, SystemPowerOps,
};
use crate::{SystemService, SystemServiceImpl};
use std::net::Ipv4Addr;
use wardnet_common::api::DhcpSource;
use wardnet_common::tunnel::{Tunnel, TunnelConfig};
use wardnetd_data::repository::tunnel::TunnelRow;
use wardnetd_data::repository::{LastShutdownInfo, SystemConfigRepository, TunnelRepository};

// -- Mock repositories ----------------------------------------------------

/// Mock system config repository returning fixed counts.
struct MockSystemConfigRepo {
    devices: i64,
    tunnels: i64,
    db_size: u64,
    last_shutdown: Mutex<LastShutdownInfo>,
    graceful_calls: AtomicUsize,
    ack_calls: AtomicUsize,
}

impl MockSystemConfigRepo {
    fn new(devices: i64, tunnels: i64, db_size: u64) -> Self {
        Self {
            devices,
            tunnels,
            db_size,
            last_shutdown: Mutex::new(LastShutdownInfo {
                state: LastShutdownState::Unknown,
                at: None,
                acknowledged_at: None,
            }),
            graceful_calls: AtomicUsize::new(0),
            ack_calls: AtomicUsize::new(0),
        }
    }

    fn with_last_shutdown(self, info: LastShutdownInfo) -> Self {
        *self.last_shutdown.lock().unwrap() = info;
        self
    }
}

#[async_trait]
impl SystemConfigRepository for MockSystemConfigRepo {
    async fn get(&self, _key: &str) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
    async fn set(&self, _key: &str, _value: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn device_count(&self) -> anyhow::Result<i64> {
        Ok(self.devices)
    }
    async fn tunnel_count(&self) -> anyhow::Result<i64> {
        Ok(self.tunnels)
    }
    async fn db_size_bytes(&self) -> anyhow::Result<u64> {
        Ok(self.db_size)
    }
    async fn detect_previous_shutdown(&self) -> anyhow::Result<LastShutdownInfo> {
        Ok(self.last_shutdown.lock().unwrap().clone())
    }
    async fn record_graceful_shutdown(&self) -> anyhow::Result<()> {
        self.graceful_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn record_heartbeat(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn read_last_shutdown(&self) -> anyhow::Result<LastShutdownInfo> {
        Ok(self.last_shutdown.lock().unwrap().clone())
    }
    async fn acknowledge_last_shutdown(&self) -> anyhow::Result<()> {
        self.ack_calls.fetch_add(1, Ordering::SeqCst);
        let mut info = self.last_shutdown.lock().unwrap();
        info.acknowledged_at = Some(Utc::now());
        Ok(())
    }
}

/// Mock tunnel repository returning a fixed active count.
struct MockTunnelRepo {
    active: i64,
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
        Ok(None)
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
        _tx: i64,
        _rx: i64,
        _hs: Option<&str>,
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
        Ok(self.active)
    }
}

// -- Helpers --------------------------------------------------------------

fn admin_ctx() -> AuthContext {
    AuthContext::Admin {
        admin_id: Uuid::new_v4(),
    }
}

/// Counts calls to `reboot()` / `poweroff()` so the request_*
/// service tests can assert the spawned task fired exactly once
/// after the grace window.
#[derive(Default)]
struct RecordingPowerOps {
    reboot_calls: AtomicUsize,
    poweroff_calls: AtomicUsize,
}

#[async_trait]
impl SystemPowerOps for RecordingPowerOps {
    async fn reboot(&self) -> Result<(), AppError> {
        self.reboot_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    async fn poweroff(&self) -> Result<(), AppError> {
        self.poweroff_calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

fn build_service(
    devices: i64,
    tunnels: i64,
    active_tunnels: i64,
    db_size: u64,
) -> SystemServiceImpl {
    build_service_with_power_ops(
        devices,
        tunnels,
        active_tunnels,
        db_size,
        Arc::new(RecordingPowerOps::default()),
    )
}

fn build_service_with_power_ops(
    devices: i64,
    tunnels: i64,
    active_tunnels: i64,
    db_size: u64,
    power_ops: Arc<dyn SystemPowerOps>,
) -> SystemServiceImpl {
    SystemServiceImpl::new(
        Arc::new(MockSystemConfigRepo::new(devices, tunnels, db_size)),
        Arc::new(MockTunnelRepo {
            active: active_tunnels,
        }),
        Instant::now(),
        tokio_util::sync::CancellationToken::new(),
        power_ops,
        Arc::new(StaticNetworkInspector),
        Arc::new(StaticNetworkProbe::default()),
    )
}

fn build_service_with_repo(
    repo: Arc<MockSystemConfigRepo>,
    active_tunnels: i64,
) -> SystemServiceImpl {
    SystemServiceImpl::new(
        repo,
        Arc::new(MockTunnelRepo {
            active: active_tunnels,
        }),
        Instant::now(),
        tokio_util::sync::CancellationToken::new(),
        Arc::new(RecordingPowerOps::default()),
        Arc::new(StaticNetworkInspector),
        Arc::new(StaticNetworkProbe::default()),
    )
}

struct StaticNetworkInspector;

#[async_trait]
impl NetworkInspector for StaticNetworkInspector {
    async fn inspect(&self) -> anyhow::Result<NetworkSnapshot> {
        Ok(NetworkSnapshot {
            interface: "eth0".to_owned(),
            ip: Ipv4Addr::new(192, 168, 1, 1),
            gateway: Some(Ipv4Addr::new(192, 168, 1, 254)),
            dhcp_source: DhcpSource::Static,
        })
    }
}

/// Network probe whose responses are configurable per test. The default
/// answers ARP with a fixed MAC and returns no DHCP responders.
struct StaticNetworkProbe {
    response: Option<String>,
    targets: Mutex<Vec<Ipv4Addr>>,
    dhcp_responders: Vec<Ipv4Addr>,
}

impl Default for StaticNetworkProbe {
    fn default() -> Self {
        Self {
            response: Some("AA:BB:CC:DD:EE:FF".to_owned()),
            targets: Mutex::new(Vec::new()),
            dhcp_responders: Vec::new(),
        }
    }
}

#[async_trait]
impl NetworkProbe for StaticNetworkProbe {
    async fn arp_probe(&self, target_ip: Ipv4Addr) -> anyhow::Result<Option<String>> {
        self.targets.lock().unwrap().push(target_ip);
        Ok(self.response.clone())
    }

    async fn dhcp_self_probe(&self) -> anyhow::Result<DhcpProbeOutcome> {
        Ok(DhcpProbeOutcome {
            responders: self.dhcp_responders.clone(),
        })
    }
}

// -- Tests ----------------------------------------------------------------

#[tokio::test]
async fn status_returns_correct_values() {
    let svc = build_service(5, 2, 1, 8192);

    let resp = auth_context::with_context(admin_ctx(), svc.status())
        .await
        .unwrap();
    assert_eq!(resp.version, env!("WARDNET_VERSION"));
    assert_eq!(resp.release_version, env!("WARDNET_RELEASE_VERSION"));
    assert_eq!(resp.device_count, 5);
    assert_eq!(resp.tunnel_count, 2);
    assert_eq!(resp.tunnel_active_count, 1);
    assert_eq!(resp.db_size_bytes, 8192);
    assert!(resp.uptime_seconds < 2); // just created
    assert!(resp.cpu_usage_percent >= 0.0);
    assert!(resp.memory_total_bytes > 0);
    assert!(resp.memory_used_bytes <= resp.memory_total_bytes);
}

#[tokio::test]
async fn status_version_comes_from_compile_time_env() {
    let svc = build_service(0, 0, 0, 0);

    let resp = auth_context::with_context(admin_ctx(), svc.status())
        .await
        .unwrap();
    // Both versions are compile-time constants, never "unknown".
    assert_eq!(resp.version, env!("WARDNET_VERSION"));
    assert_eq!(resp.release_version, env!("WARDNET_RELEASE_VERSION"));
}

#[tokio::test]
async fn status_anonymous_forbidden() {
    let svc = build_service(0, 0, 0, 0);
    let result = auth_context::with_context(AuthContext::Anonymous, svc.status()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn status_device_forbidden() {
    let svc = build_service(0, 0, 0, 0);
    let ctx = AuthContext::Device {
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
    };
    let result = auth_context::with_context(ctx, svc.status()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

// -- request_reboot / request_shutdown ------------------------------------

#[tokio::test(start_paused = true)]
async fn request_reboot_invokes_power_ops_after_grace() {
    let ops = Arc::new(RecordingPowerOps::default());
    let svc = build_service_with_power_ops(0, 0, 0, 0, ops.clone());

    auth_context::with_context(admin_ctx(), svc.request_reboot())
        .await
        .unwrap();

    // Yield so the spawned task gets polled and registers its sleep
    // with the paused timer driver. Without this the timer driver
    // never sees the sleep and `advance` has nothing to wake.
    tokio::task::yield_now().await;
    assert_eq!(ops.reboot_calls.load(Ordering::SeqCst), 0);

    // Advance past the 500 ms grace window, then drive the runtime
    // a few more times so the woken task runs to completion.
    tokio::time::advance(std::time::Duration::from_millis(750)).await;
    for _ in 0..20 {
        tokio::task::yield_now().await;
        tokio::time::advance(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(ops.reboot_calls.load(Ordering::SeqCst), 1);
    assert_eq!(ops.poweroff_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn request_shutdown_invokes_power_ops_after_grace() {
    let ops = Arc::new(RecordingPowerOps::default());
    let svc = build_service_with_power_ops(0, 0, 0, 0, ops.clone());

    auth_context::with_context(admin_ctx(), svc.request_shutdown())
        .await
        .unwrap();

    tokio::task::yield_now().await;
    assert_eq!(ops.poweroff_calls.load(Ordering::SeqCst), 0);

    tokio::time::advance(std::time::Duration::from_millis(750)).await;
    for _ in 0..20 {
        tokio::task::yield_now().await;
        tokio::time::advance(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(ops.poweroff_calls.load(Ordering::SeqCst), 1);
    assert_eq!(ops.reboot_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn request_reboot_returns_immediately() {
    // Without `start_paused`, real time progresses; the test just
    // confirms the call returns long before the 500 ms grace would
    // have elapsed (which means the response can flush before the
    // power op fires).
    let ops = Arc::new(RecordingPowerOps::default());
    let svc = build_service_with_power_ops(0, 0, 0, 0, ops);

    let started = std::time::Instant::now();
    auth_context::with_context(admin_ctx(), svc.request_reboot())
        .await
        .unwrap();
    assert!(
        started.elapsed() < std::time::Duration::from_millis(100),
        "request_reboot should return promptly, took {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn request_reboot_anonymous_forbidden() {
    let ops = Arc::new(RecordingPowerOps::default());
    let svc = build_service_with_power_ops(0, 0, 0, 0, ops.clone());
    let result = auth_context::with_context(AuthContext::Anonymous, svc.request_reboot()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
    assert_eq!(ops.reboot_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn request_shutdown_anonymous_forbidden() {
    let ops = Arc::new(RecordingPowerOps::default());
    let svc = build_service_with_power_ops(0, 0, 0, 0, ops.clone());
    let result = auth_context::with_context(AuthContext::Anonymous, svc.request_shutdown()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
    assert_eq!(ops.poweroff_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn request_reboot_device_forbidden() {
    let ops = Arc::new(RecordingPowerOps::default());
    let svc = build_service_with_power_ops(0, 0, 0, 0, ops.clone());
    let ctx = AuthContext::Device {
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
    };
    let result = auth_context::with_context(ctx, svc.request_reboot()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
    assert_eq!(ops.reboot_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn request_shutdown_device_forbidden() {
    let ops = Arc::new(RecordingPowerOps::default());
    let svc = build_service_with_power_ops(0, 0, 0, 0, ops.clone());
    let ctx = AuthContext::Device {
        mac: "AA:BB:CC:DD:EE:01".to_owned(),
    };
    let result = auth_context::with_context(ctx, svc.request_shutdown()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
    assert_eq!(ops.poweroff_calls.load(Ordering::SeqCst), 0);
}

// -- network probes / status -----------------------------------------------

/// Inspector with configurable IP + gateway. Used by the
/// `network_status` / `discover_gateway_mac` / `dhcp_self_probe` tests
/// where the default `StaticNetworkInspector` would over-constrain
/// the behaviour under test.
struct ConfigurableNetworkInspector {
    interface: String,
    ip: Ipv4Addr,
    gateway: Option<Ipv4Addr>,
    dhcp_source: DhcpSource,
}

#[async_trait]
impl NetworkInspector for ConfigurableNetworkInspector {
    async fn inspect(&self) -> anyhow::Result<NetworkSnapshot> {
        Ok(NetworkSnapshot {
            interface: self.interface.clone(),
            ip: self.ip,
            gateway: self.gateway,
            dhcp_source: self.dhcp_source,
        })
    }
}

/// Probe whose ARP reply and DHCP responders are configurable, with
/// the actual ARP target IP recorded so tests can assert which
/// gateway was probed.
struct ConfigurableNetworkProbe {
    arp_response: Option<String>,
    arp_targets: Mutex<Vec<Ipv4Addr>>,
    dhcp_responders: Vec<Ipv4Addr>,
}

#[async_trait]
impl NetworkProbe for ConfigurableNetworkProbe {
    async fn arp_probe(&self, target_ip: Ipv4Addr) -> anyhow::Result<Option<String>> {
        self.arp_targets.lock().unwrap().push(target_ip);
        Ok(self.arp_response.clone())
    }
    async fn dhcp_self_probe(&self) -> anyhow::Result<DhcpProbeOutcome> {
        Ok(DhcpProbeOutcome {
            responders: self.dhcp_responders.clone(),
        })
    }
}

/// In-memory [`SystemConfigRepository`] so persistence assertions can
/// read what `discover_gateway_mac` wrote.
#[derive(Default)]
struct RecordingSystemConfig {
    store: Mutex<std::collections::HashMap<String, String>>,
}

#[async_trait]
impl SystemConfigRepository for RecordingSystemConfig {
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

struct ProbeFixture {
    svc: SystemServiceImpl,
    probe: Arc<ConfigurableNetworkProbe>,
    system_config: Arc<RecordingSystemConfig>,
}

fn build_probe_fixture(
    inspector: ConfigurableNetworkInspector,
    arp_response: Option<String>,
    dhcp_responders: Vec<Ipv4Addr>,
) -> ProbeFixture {
    let probe = Arc::new(ConfigurableNetworkProbe {
        arp_response,
        arp_targets: Mutex::new(Vec::new()),
        dhcp_responders,
    });
    let system_config = Arc::new(RecordingSystemConfig::default());
    let svc = SystemServiceImpl::new(
        system_config.clone(),
        Arc::new(MockTunnelRepo { active: 0 }),
        Instant::now(),
        tokio_util::sync::CancellationToken::new(),
        Arc::new(RecordingPowerOps::default()),
        Arc::new(inspector),
        probe.clone(),
    );
    ProbeFixture {
        svc,
        probe,
        system_config,
    }
}

fn fresh_inspector() -> ConfigurableNetworkInspector {
    ConfigurableNetworkInspector {
        interface: "eth0".to_owned(),
        ip: Ipv4Addr::new(192, 168, 1, 1),
        gateway: Some(Ipv4Addr::new(192, 168, 1, 254)),
        dhcp_source: DhcpSource::Static,
    }
}

#[tokio::test]
async fn network_status_returns_inspector_snapshot() {
    let fx = build_probe_fixture(fresh_inspector(), None, vec![]);
    let resp = auth_context::with_context(admin_ctx(), fx.svc.network_status())
        .await
        .unwrap();
    assert_eq!(resp.interface, "eth0");
    assert_eq!(resp.ip, Ipv4Addr::new(192, 168, 1, 1));
    assert_eq!(resp.gateway, Some(Ipv4Addr::new(192, 168, 1, 254)));
    assert_eq!(resp.dhcp_source, DhcpSource::Static);
}

#[tokio::test]
async fn network_status_anonymous_forbidden() {
    let fx = build_probe_fixture(fresh_inspector(), None, vec![]);
    let result = fx.svc.network_status().await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn discover_gateway_mac_manual_path_persists_normalised_mac() {
    let fx = build_probe_fixture(fresh_inspector(), None, vec![]);
    let resp = auth_context::with_context(
        admin_ctx(),
        fx.svc
            .discover_gateway_mac(wardnet_common::api::DiscoverGatewayMacRequest {
                mac: Some("aa:bb:cc:dd:ee:ff".to_owned()),
                target_ip: None,
            }),
    )
    .await
    .unwrap();
    // Normalised to upper-case for storage.
    assert_eq!(resp.mac, "AA:BB:CC:DD:EE:FF");
    assert_eq!(resp.source, wardnet_common::api::RouterMacSource::Manual);
    // No ARP probe should have been issued on the manual path.
    assert!(fx.probe.arp_targets.lock().unwrap().is_empty());
    // Persisted under `router_mac`.
    assert_eq!(
        fx.system_config.get("router_mac").await.unwrap().as_deref(),
        Some("AA:BB:CC:DD:EE:FF")
    );
}

#[tokio::test]
async fn discover_gateway_mac_manual_path_rejects_invalid_mac() {
    let fx = build_probe_fixture(fresh_inspector(), None, vec![]);
    let result = auth_context::with_context(
        admin_ctx(),
        fx.svc
            .discover_gateway_mac(wardnet_common::api::DiscoverGatewayMacRequest {
                mac: Some("not-a-mac".to_owned()),
                target_ip: None,
            }),
    )
    .await;
    assert!(matches!(result, Err(AppError::BadRequest(_))));
    // Nothing persisted on rejection.
    assert!(fx.system_config.get("router_mac").await.unwrap().is_none());
}

#[tokio::test]
async fn discover_gateway_mac_auto_no_gateway_returns_bad_request() {
    let mut inspector = fresh_inspector();
    inspector.gateway = None;
    let fx = build_probe_fixture(inspector, Some("aa:bb:cc:dd:ee:ff".to_owned()), vec![]);
    let result = auth_context::with_context(
        admin_ctx(),
        fx.svc
            .discover_gateway_mac(wardnet_common::api::DiscoverGatewayMacRequest {
                mac: None,
                target_ip: None,
            }),
    )
    .await;
    assert!(matches!(result, Err(AppError::BadRequest(_))));
    // Probe shouldn't fire when there's no target.
    assert!(fx.probe.arp_targets.lock().unwrap().is_empty());
}

#[tokio::test]
async fn discover_gateway_mac_no_arp_reply_returns_not_found() {
    // Inspector reports a gateway, probe returns None (timeout) → NotFound.
    let fx = build_probe_fixture(fresh_inspector(), None, vec![]);
    let result = auth_context::with_context(
        admin_ctx(),
        fx.svc
            .discover_gateway_mac(wardnet_common::api::DiscoverGatewayMacRequest {
                mac: None,
                target_ip: None,
            }),
    )
    .await;
    assert!(matches!(result, Err(AppError::NotFound(_))));
    assert_eq!(
        *fx.probe.arp_targets.lock().unwrap(),
        vec![Ipv4Addr::new(192, 168, 1, 254)]
    );
}

#[tokio::test]
async fn discover_gateway_mac_auto_path_persists_and_uses_gateway() {
    let fx = build_probe_fixture(
        fresh_inspector(),
        Some("11:22:33:44:55:66".to_owned()),
        vec![],
    );
    let resp = auth_context::with_context(
        admin_ctx(),
        fx.svc
            .discover_gateway_mac(wardnet_common::api::DiscoverGatewayMacRequest {
                mac: None,
                target_ip: None,
            }),
    )
    .await
    .unwrap();
    assert_eq!(resp.mac, "11:22:33:44:55:66");
    assert_eq!(resp.source, wardnet_common::api::RouterMacSource::Arp);
    assert_eq!(
        *fx.probe.arp_targets.lock().unwrap(),
        vec![Ipv4Addr::new(192, 168, 1, 254)]
    );
    assert_eq!(
        fx.system_config.get("router_mac").await.unwrap().as_deref(),
        Some("11:22:33:44:55:66")
    );
}

#[tokio::test]
async fn dhcp_self_probe_classifies_our_ip_as_wardnet() {
    let fx = build_probe_fixture(fresh_inspector(), None, vec![Ipv4Addr::new(192, 168, 1, 1)]);
    let resp = auth_context::with_context(admin_ctx(), fx.svc.dhcp_self_probe())
        .await
        .unwrap();
    assert!(resp.wardnet_responded);
    assert!(!resp.foreign_responded);
    assert!(resp.foreign_server_ip.is_none());
}

#[tokio::test]
async fn dhcp_self_probe_classifies_other_ip_as_foreign() {
    let fx = build_probe_fixture(fresh_inspector(), None, vec![Ipv4Addr::new(10, 0, 0, 1)]);
    let resp = auth_context::with_context(admin_ctx(), fx.svc.dhcp_self_probe())
        .await
        .unwrap();
    assert!(!resp.wardnet_responded);
    assert!(resp.foreign_responded);
    assert_eq!(resp.foreign_server_ip, Some(Ipv4Addr::new(10, 0, 0, 1)));
}

#[tokio::test]
async fn dhcp_self_probe_handles_both_responders() {
    let fx = build_probe_fixture(
        fresh_inspector(),
        None,
        vec![Ipv4Addr::new(192, 168, 1, 1), Ipv4Addr::new(10, 0, 0, 1)],
    );
    let resp = auth_context::with_context(admin_ctx(), fx.svc.dhcp_self_probe())
        .await
        .unwrap();
    assert!(resp.wardnet_responded);
    assert!(resp.foreign_responded);
    assert_eq!(resp.foreign_server_ip, Some(Ipv4Addr::new(10, 0, 0, 1)));
}

#[tokio::test]
async fn dhcp_self_probe_picks_first_foreign_when_multiple() {
    let fx = build_probe_fixture(
        fresh_inspector(),
        None,
        vec![
            Ipv4Addr::new(10, 0, 0, 1),
            Ipv4Addr::new(10, 0, 0, 2),
            Ipv4Addr::new(192, 168, 1, 1),
        ],
    );
    let resp = auth_context::with_context(admin_ctx(), fx.svc.dhcp_self_probe())
        .await
        .unwrap();
    assert!(resp.wardnet_responded);
    assert!(resp.foreign_responded);
    // First foreign in iteration order — pins the "first foreign wins"
    // branch in service.rs.
    assert_eq!(resp.foreign_server_ip, Some(Ipv4Addr::new(10, 0, 0, 1)));
}

#[tokio::test]
async fn dhcp_self_probe_anonymous_forbidden() {
    let fx = build_probe_fixture(fresh_inspector(), None, vec![]);
    let result = fx.svc.dhcp_self_probe().await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

#[tokio::test]
async fn discover_gateway_mac_anonymous_forbidden() {
    let fx = build_probe_fixture(fresh_inspector(), None, vec![]);
    let result = fx
        .svc
        .discover_gateway_mac(wardnet_common::api::DiscoverGatewayMacRequest {
            mac: None,
            target_ip: None,
        })
        .await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}

// -- last_shutdown / record_graceful / acknowledge ------------------------

#[tokio::test]
async fn status_includes_last_shutdown_unknown() {
    let svc = build_service(0, 0, 0, 0);
    let resp = auth_context::with_context(admin_ctx(), svc.status())
        .await
        .unwrap();
    assert_eq!(resp.last_shutdown.state, LastShutdownState::Unknown);
    assert!(resp.last_shutdown.at.is_none());
    assert!(resp.last_shutdown.acknowledged_at.is_none());
}

#[tokio::test]
async fn status_includes_last_shutdown_unclean() {
    let at: DateTime<Utc> = Utc::now();
    let repo = Arc::new(
        MockSystemConfigRepo::new(0, 0, 0).with_last_shutdown(LastShutdownInfo {
            state: LastShutdownState::Unclean,
            at: Some(at),
            acknowledged_at: None,
        }),
    );
    let svc = build_service_with_repo(repo, 0);

    let resp = auth_context::with_context(admin_ctx(), svc.status())
        .await
        .unwrap();
    assert_eq!(resp.last_shutdown.state, LastShutdownState::Unclean);
    assert_eq!(resp.last_shutdown.at, Some(at));
}

#[tokio::test]
async fn record_graceful_shutdown_calls_repo() {
    let repo = Arc::new(MockSystemConfigRepo::new(0, 0, 0));
    let svc = build_service_with_repo(repo.clone(), 0);

    auth_context::with_context(admin_ctx(), svc.record_graceful_shutdown())
        .await
        .unwrap();
    assert_eq!(repo.graceful_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn record_graceful_shutdown_anonymous_forbidden() {
    let repo = Arc::new(MockSystemConfigRepo::new(0, 0, 0));
    let svc = build_service_with_repo(repo.clone(), 0);

    let result =
        auth_context::with_context(AuthContext::Anonymous, svc.record_graceful_shutdown()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
    assert_eq!(repo.graceful_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn acknowledge_last_shutdown_calls_repo() {
    let repo = Arc::new(MockSystemConfigRepo::new(0, 0, 0));
    let svc = build_service_with_repo(repo.clone(), 0);

    auth_context::with_context(admin_ctx(), svc.acknowledge_last_shutdown())
        .await
        .unwrap();
    assert_eq!(repo.ack_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn acknowledge_last_shutdown_anonymous_forbidden() {
    let repo = Arc::new(MockSystemConfigRepo::new(0, 0, 0));
    let svc = build_service_with_repo(repo.clone(), 0);

    let result =
        auth_context::with_context(AuthContext::Anonymous, svc.acknowledge_last_shutdown()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
    assert_eq!(repo.ack_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn record_heartbeat_admin_succeeds() {
    let repo = Arc::new(MockSystemConfigRepo::new(0, 0, 0));
    let svc = build_service_with_repo(repo, 0);

    auth_context::with_context(admin_ctx(), svc.record_heartbeat())
        .await
        .unwrap();
}

#[tokio::test]
async fn record_heartbeat_anonymous_forbidden() {
    let repo = Arc::new(MockSystemConfigRepo::new(0, 0, 0));
    let svc = build_service_with_repo(repo, 0);

    let result = auth_context::with_context(AuthContext::Anonymous, svc.record_heartbeat()).await;
    assert!(matches!(result, Err(AppError::Forbidden(_))));
}
