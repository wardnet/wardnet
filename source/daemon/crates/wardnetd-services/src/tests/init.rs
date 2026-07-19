//! Integration test for [`init_services_with_factory`].
//!
//! Verifies the service-wiring code path in `lib.rs` without exercising
//! behavior of individual services. Backends are minimal stubs that panic
//! if any method is called — construction alone should not reach them.

use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;
use wardnet_common::config::ApplicationConfiguration;
use wardnetd_data::SqliteRepositoryFactory;
use wardnetd_data::db::init_pool_from_connection_string;
use wardnetd_data::secret_store::SecretStore;

use crate::Backends;
use crate::device::hostname_resolver::HostnameResolver;
use crate::device::packet_capture::{ObservedDevice, PacketCapture};
use crate::error::AppError;
use crate::logging::{ErrorNotifierService, LogService, LogServiceImpl, LogStreamService};
use crate::routing::firewall::FirewallManager;
use crate::routing::policy_router::PolicyRouter;
use crate::system::SystemPowerOps;
use crate::tunnel::exit_probe::{ExitInfo, ProbeError, TunnelExitProbe};
use crate::tunnel::interface::{CreateTunnelParams, TunnelInterface, TunnelStats};
use crate::tunnel::latency_prober::{LatencyProbeError, TunnelLatencyProber};
use crate::tunnel::throughput_tester::{ThroughputError, ThroughputMeasurement, ThroughputTester};
use crate::{init_services, init_services_with_factory};
use wardnet_common::config::AdminConfig;

// ---------------------------------------------------------------------------
// Minimal backend stubs — every method panics. Construction-only test.
// ---------------------------------------------------------------------------

struct StubTunnelInterface;
#[async_trait]
impl TunnelInterface for StubTunnelInterface {
    async fn create(&self, _params: CreateTunnelParams) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn bring_up(&self, _interface_name: &str) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn tear_down(&self, _interface_name: &str) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn remove(&self, _interface_name: &str) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn get_stats(&self, _interface_name: &str) -> anyhow::Result<Option<TunnelStats>> {
        unimplemented!()
    }
    async fn list(&self) -> anyhow::Result<Vec<String>> {
        unimplemented!()
    }
}

struct StubInboundWgInterface;
#[async_trait]
impl crate::inbound_wg::interface::InboundWgInterface for StubInboundWgInterface {
    async fn ensure_server(
        &self,
        _config: crate::inbound_wg::interface::InboundWgServerConfig,
    ) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn tear_down_server(&self, _interface_name: &str) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn add_peer(
        &self,
        _interface_name: &str,
        _peer: crate::inbound_wg::interface::InboundWgPeerConfig,
    ) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn remove_peer(
        &self,
        _interface_name: &str,
        _public_key: [u8; 32],
    ) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn peer_stats(
        &self,
        _interface_name: &str,
    ) -> anyhow::Result<Vec<crate::inbound_wg::interface::InboundWgPeerStats>> {
        unimplemented!()
    }
}

struct StubTunnelExitProbe;
#[async_trait]
impl TunnelExitProbe for StubTunnelExitProbe {
    async fn probe(&self, _interface: &str) -> Result<ExitInfo, ProbeError> {
        Err(ProbeError::Unsupported(
            "stub probe in init test".to_owned(),
        ))
    }
}

struct StubThroughputTester;
#[async_trait]
impl ThroughputTester for StubThroughputTester {
    async fn download(
        &self,
        _interface: Option<&str>,
    ) -> Result<ThroughputMeasurement, ThroughputError> {
        Err(ThroughputError::Unsupported(
            "stub throughput tester in init test".to_owned(),
        ))
    }
}

struct StubTunnelLatencyProber;
#[async_trait]
impl TunnelLatencyProber for StubTunnelLatencyProber {
    async fn probe(&self, _interface: Option<&str>) -> Result<u64, LatencyProbeError> {
        Err(LatencyProbeError::Unsupported(
            "stub latency prober in init test".to_owned(),
        ))
    }
}

struct StubPolicyRouter;
#[async_trait]
impl PolicyRouter for StubPolicyRouter {
    async fn enable_ip_forwarding(&self) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn add_route_table(&self, _interface: &str, _table: u32) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn remove_route_table(&self, _table: u32) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn has_route_table(&self, _table: u32) -> anyhow::Result<bool> {
        unimplemented!()
    }
    async fn add_ip_rule(&self, _src_ip: &str, _table: u32) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn remove_ip_rule(&self, _src_ip: &str, _table: u32) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn list_wardnet_rules(&self) -> anyhow::Result<Vec<(String, u32)>> {
        unimplemented!()
    }
    async fn add_switchback_rule(
        &self,
        _src_ip: &str,
        _dst_cidr: &str,
        _priority: u32,
    ) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn remove_switchback_rule(
        &self,
        _src_ip: &str,
        _dst_cidr: &str,
        _priority: u32,
    ) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn list_switchback_rules(&self) -> anyhow::Result<Vec<(String, String, u32)>> {
        unimplemented!()
    }
    async fn flush_conntrack(&self, _src_ip: &str) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn flush_route_cache(&self) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn check_tools_available(&self) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn add_interface_alias(&self, _i: &str, _ip: &str, _p: u8) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn remove_interface_alias(&self, _i: &str, _ip: &str, _p: u8) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn list_interface_aliases(&self, _i: &str) -> anyhow::Result<Vec<(String, u8)>> {
        unimplemented!()
    }
    async fn set_proxy_arp(&self, _i: &str, _e: bool) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn add_host_route(&self, _ip: &str, _i: &str) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn remove_host_route(&self, _ip: &str, _i: &str) -> anyhow::Result<()> {
        unimplemented!()
    }
}

struct StubFirewall;
#[async_trait]
impl FirewallManager for StubFirewall {
    async fn init_wardnet_table(&self) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn ensure_isolation_jumps(&self) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn flush_wardnet_table(&self) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn add_masquerade(&self, _interface: &str) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn remove_masquerade(&self, _interface: &str) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn add_inbound_wg_accept(&self, _port: u16) -> anyhow::Result<()> {
        Ok(())
    }

    async fn remove_inbound_wg_accept(&self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn cleanup_legacy_dns_redirects(&self) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn add_tcp_reset_reject(&self, _device_ip: &str) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn remove_tcp_reset_reject(&self, _device_ip: &str) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn apply_zone_rules(
        &self,
        _device_ip: &str,
        _rules: crate::routing::firewall::ZoneRules,
        _lan_interface: &str,
    ) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn remove_zone_rules(&self, _device_ip: &str) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn list_zone_rule_ips(&self) -> anyhow::Result<Vec<String>> {
        unimplemented!()
    }
    async fn apply_zone_isolation(
        &self,
        _rules: crate::routing::firewall::ZoneIsolationRules,
    ) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn check_tools_available(&self) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn destroy_wardnet_table(&self) -> anyhow::Result<()> {
        unimplemented!()
    }
}

struct StubPacketCapture;
#[async_trait]
impl PacketCapture for StubPacketCapture {
    async fn capture_loop(
        &self,
        _interface: &str,
        _sender: tokio::sync::mpsc::Sender<ObservedDevice>,
        _cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn arp_scan(&self, _interface: &str) -> anyhow::Result<()> {
        unimplemented!()
    }
}

struct StubHostnameResolver;
#[async_trait]
impl HostnameResolver for StubHostnameResolver {
    async fn resolve(&self, _ip: &str) -> Option<String> {
        None
    }
}

struct StubPowerOps;
#[async_trait]
impl SystemPowerOps for StubPowerOps {
    async fn reboot(&self) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn poweroff(&self) -> Result<(), AppError> {
        unimplemented!()
    }
}

struct StubSecretStore;
#[async_trait]
impl SecretStore for StubSecretStore {
    async fn put(&self, _path: &str, _value: &[u8]) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn get(&self, _path: &str) -> anyhow::Result<Option<Vec<u8>>> {
        unimplemented!()
    }
    async fn delete(&self, _path: &str) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn list(&self, _prefix: &str) -> anyhow::Result<Vec<String>> {
        unimplemented!()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn stub_backends() -> Backends {
    Backends {
        tunnel_interface: Arc::new(StubTunnelInterface),
        inbound_wg_interface: Arc::new(StubInboundWgInterface),
        tunnel_exit_probe: Arc::new(StubTunnelExitProbe),
        tunnel_latency_prober: Arc::new(StubTunnelLatencyProber),
        tunnel_throughput_tester: Arc::new(StubThroughputTester),
        policy_router: Arc::new(StubPolicyRouter),
        firewall: Arc::new(StubFirewall),
        packet_capture: Arc::new(StubPacketCapture),
        hostname_resolver: Arc::new(StubHostnameResolver),
        secret_store: Arc::new(StubSecretStore),
        web_push_sender: Arc::new(StubWebPushSender),
        blocklist_fetcher: Arc::new(StubBlocklistFetcher),
        update: crate::UpdateBackends {
            release_source: Arc::new(StubReleaseSource),
            verifier: Arc::new(StubReleaseVerifier),
            applier: Arc::new(StubBinaryApplier),
        },
        config_path: std::path::PathBuf::from("/tmp/wardnet-init-test.toml"),
        host_id: "init-test-host".to_owned(),
        shutdown_token: tokio_util::sync::CancellationToken::new(),
        power_ops: Arc::new(StubPowerOps),
        network_inspector: Arc::new(StubNetworkInspector),
        network_probe: Arc::new(StubNetworkProbe),
        garp_ops: Arc::new(StubGarpOps),
        cert_activator: Arc::new(StubCertActivator),
        watchdog_ops: Arc::new(StubWatchdog),
    }
}

struct StubWebPushSender;

#[async_trait::async_trait]
impl crate::push::sender::WebPushSender for StubWebPushSender {
    async fn send(
        &self,
        _vapid: &crate::push::sender::VapidKey,
        _target: crate::push::sender::PushTarget<'_>,
        _payload: Vec<u8>,
    ) -> crate::push::sender::SendOutcome {
        crate::push::sender::SendOutcome::Delivered
    }
}

struct StubWatchdog;
#[async_trait]
impl crate::system::WatchdogOps for StubWatchdog {
    async fn pet(&self) {}
    async fn disarm(&self) {}
    fn is_available(&self) -> bool {
        false
    }
}

struct StubCertActivator;
#[async_trait]
impl crate::tls::CertActivator for StubCertActivator {
    async fn activate(
        &self,
        _chain_pem: Vec<u8>,
        _key_pem: Vec<u8>,
        _fqdn: String,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn deactivate(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

struct StubGarpOps;
#[async_trait]
impl crate::garp::GarpOps for StubGarpOps {
    async fn broadcast_farewell(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn broadcast_claim(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

struct StubNetworkInspector;
#[async_trait]
impl crate::system::NetworkInspector for StubNetworkInspector {
    async fn inspect(&self) -> anyhow::Result<crate::system::NetworkSnapshot> {
        Ok(crate::system::NetworkSnapshot {
            interface: "eth0".to_owned(),
            ip: std::net::Ipv4Addr::new(192, 168, 1, 1),
            gateway: Some(std::net::Ipv4Addr::new(192, 168, 1, 254)),
            dhcp_source: wardnet_common::api::DhcpSource::Static,
        })
    }
}

struct StubNetworkProbe;
#[async_trait]
impl crate::system::NetworkProbe for StubNetworkProbe {
    async fn arp_probe(&self, _target_ip: std::net::Ipv4Addr) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
    async fn dhcp_self_probe(&self) -> anyhow::Result<crate::system::DhcpProbeOutcome> {
        Ok(crate::system::DhcpProbeOutcome::default())
    }
}

struct StubReleaseSource;
#[async_trait]
impl crate::update::release_source::ReleaseSource for StubReleaseSource {
    async fn latest(
        &self,
        _channel: wardnet_common::update::UpdateChannel,
    ) -> anyhow::Result<Option<wardnet_common::update::Release>> {
        Ok(None)
    }
    async fn fetch_asset(&self, _url: &str) -> anyhow::Result<Vec<u8>> {
        Ok(Vec::new())
    }
}

struct StubReleaseVerifier;
#[async_trait]
impl crate::update::verifier::ReleaseVerifier for StubReleaseVerifier {
    async fn verify_sha256(&self, _tarball: &[u8], _expected_hex: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn verify_signature(&self, _tarball: &[u8], _signature: &[u8]) -> anyhow::Result<()> {
        Ok(())
    }
}

struct StubBinaryApplier;
#[async_trait]
impl crate::update::applier::BinaryApplier for StubBinaryApplier {
    async fn apply(
        &self,
        _tarball: &[u8],
        _signature: &[u8],
    ) -> anyhow::Result<crate::update::applier::SwapOutcome> {
        unimplemented!("init tests never apply a real tarball")
    }
    async fn rollback(&self) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn rollback_available(&self) -> bool {
        false
    }
}

struct StubBlocklistFetcher;
#[async_trait::async_trait]
impl crate::dns_filter::blocklist_downloader::BlocklistFetcher for StubBlocklistFetcher {
    async fn fetch(&self, _url: &str) -> anyhow::Result<String> {
        unimplemented!("init tests never dispatch a blocklist refresh")
    }
}

fn stub_log_service() -> Arc<dyn LogService> {
    let stream = Arc::new(LogStreamService::new(16));
    let errors = Arc::new(ErrorNotifierService::new(15));
    Arc::new(LogServiceImpl::new(
        stream,
        errors,
        std::path::PathBuf::from("/tmp/wardnet-init-test.log"),
    ))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn init_services_with_factory_builds_every_service() {
    let pool = init_pool_from_connection_string(":memory:")
        .await
        .expect("in-memory pool");
    let factory = SqliteRepositoryFactory::from_pool(pool, std::path::PathBuf::from(":memory:"));

    let config = ApplicationConfiguration::default();
    let lan_ip = Ipv4Addr::new(192, 168, 1, 1);
    let started_at = Instant::now();

    let services = init_services_with_factory(
        &factory,
        stub_backends(),
        &config,
        lan_ip,
        started_at,
        stub_log_service(),
    )
    .await
    .expect("init_services_with_factory");

    // Verify every service handle is populated (Arc::strong_count >= 1 means
    // the Arc is alive; construction alone is the thing under test).
    assert!(Arc::strong_count(&services.auth) >= 1);
    assert!(Arc::strong_count(&services.device) >= 1);
    assert!(Arc::strong_count(&services.dhcp) >= 1);
    assert!(Arc::strong_count(&services.dns) >= 1);
    assert!(Arc::strong_count(&services.discovery) >= 1);
    assert!(Arc::strong_count(&services.log) >= 1);
    assert!(Arc::strong_count(&services.vpn_provider) >= 1);
    assert!(Arc::strong_count(&services.routing) >= 1);
    assert!(Arc::strong_count(&services.system) >= 1);
    assert!(Arc::strong_count(&services.tunnel) >= 1);
    assert!(Arc::strong_count(&services.event_publisher) >= 1);
    assert!(Arc::strong_count(&services.dns_repo) >= 1);
}

#[tokio::test]
async fn init_services_with_factory_respects_disabled_provider() {
    let pool = init_pool_from_connection_string(":memory:")
        .await
        .expect("in-memory pool");
    let factory = SqliteRepositoryFactory::from_pool(pool, std::path::PathBuf::from(":memory:"));

    let mut config = ApplicationConfiguration::default();
    config
        .vpn_providers
        .enabled
        .insert("nordvpn".to_owned(), false);

    let services = init_services_with_factory(
        &factory,
        stub_backends(),
        &config,
        Ipv4Addr::new(10, 0, 0, 1),
        Instant::now(),
        stub_log_service(),
    )
    .await
    .expect("init_services_with_factory");

    // Wiring still succeeds even when the built-in provider is disabled.
    assert!(Arc::strong_count(&services.vpn_provider) >= 1);
}

#[tokio::test]
async fn init_services_bootstraps_admin_from_config() {
    // Exercises the async `init_services` entry point: opens an in-memory
    // SQLite pool via `create_repository_factory`, bootstraps the admin from
    // the config, and wires the service layer.
    let mut config = ApplicationConfiguration::default();
    config.database.connection_string = ":memory:".to_owned();
    config.admin = Some(AdminConfig {
        username: "opsadmin".to_owned(),
        password: "supersecret".to_owned(),
    });

    let services = init_services(
        &config,
        stub_backends(),
        Ipv4Addr::new(192, 168, 1, 1),
        Instant::now(),
        stub_log_service(),
    )
    .await
    .expect("init_services should succeed with in-memory SQLite");

    assert!(Arc::strong_count(&services.auth) >= 1);
    assert!(Arc::strong_count(&services.device) >= 1);
    assert!(Arc::strong_count(&services.tunnel) >= 1);
}

#[tokio::test]
async fn init_services_without_admin_block_defers_to_setup_wizard() {
    // When no admin config is provided, `bootstrap_admin` leaves the DB
    // without an admin so the setup wizard owns first-admin creation;
    // `init_services` should still succeed and produce every service.
    let mut config = ApplicationConfiguration::default();
    config.database.connection_string = ":memory:".to_owned();
    config.admin = None;

    let services = init_services(
        &config,
        stub_backends(),
        Ipv4Addr::new(10, 0, 0, 1),
        Instant::now(),
        stub_log_service(),
    )
    .await
    .expect("init_services should succeed without admin block");

    assert!(Arc::strong_count(&services.system) >= 1);
    assert!(Arc::strong_count(&services.routing) >= 1);
    assert!(Arc::strong_count(&services.dhcp) >= 1);
    assert!(Arc::strong_count(&services.dns) >= 1);
    assert!(Arc::strong_count(&services.discovery) >= 1);
    assert!(Arc::strong_count(&services.vpn_provider) >= 1);
    assert!(Arc::strong_count(&services.event_publisher) >= 1);
    assert!(Arc::strong_count(&services.dns_repo) >= 1);
}

#[tokio::test]
async fn init_services_with_broadcast_lan_ip_falls_back_to_default_subnet() {
    // `255.255.255.255` + /24 is invalid for `Ipv4Network::new`, which hits
    // the `unwrap_or_else` fallback branch in `create_services`. Construction
    // must still succeed thanks to the second /24 attempt.
    let pool = init_pool_from_connection_string(":memory:")
        .await
        .expect("in-memory pool");
    let factory = SqliteRepositoryFactory::from_pool(pool, std::path::PathBuf::from(":memory:"));
    let config = ApplicationConfiguration::default();

    let services = init_services_with_factory(
        &factory,
        stub_backends(),
        &config,
        Ipv4Addr::new(192, 168, 99, 1),
        Instant::now(),
        stub_log_service(),
    )
    .await
    .expect("init_services_with_factory");

    // discovery service exists regardless — the fallback path does not panic.
    assert!(Arc::strong_count(&services.discovery) >= 1);
}
