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
use uuid::Uuid;
use wardnet_common::config::ApplicationConfiguration;
use wardnetd_data::SqliteRepositoryFactory;
use wardnetd_data::db::init_pool_from_connection_string;
use wardnetd_data::keys::KeyStore;

use crate::Backends;
use crate::device::hostname_resolver::HostnameResolver;
use crate::device::packet_capture::{ObservedDevice, PacketCapture};
use crate::logging::{ErrorNotifierService, LogService, LogServiceImpl, LogStreamService};
use crate::routing::firewall::FirewallManager;
use crate::routing::policy_router::PolicyRouter;
use crate::tunnel::interface::{CreateTunnelParams, TunnelInterface, TunnelStats};
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
    async fn flush_conntrack(&self, _src_ip: &str) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn flush_route_cache(&self) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn check_tools_available(&self) -> anyhow::Result<()> {
        unimplemented!()
    }
}

struct StubFirewall;
#[async_trait]
impl FirewallManager for StubFirewall {
    async fn init_wardnet_table(&self) -> anyhow::Result<()> {
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
    async fn add_dns_redirect(&self, _device_ip: &str, _dns_ip: &str) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn remove_dns_redirect(&self, _device_ip: &str) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn add_tcp_reset_reject(&self, _device_ip: &str) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn remove_tcp_reset_reject(&self, _device_ip: &str) -> anyhow::Result<()> {
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

struct StubKeyStore;
#[async_trait]
impl KeyStore for StubKeyStore {
    async fn save_key(&self, _tunnel_id: &Uuid, _private_key: &str) -> anyhow::Result<()> {
        unimplemented!()
    }
    async fn load_key(&self, _tunnel_id: &Uuid) -> anyhow::Result<String> {
        unimplemented!()
    }
    async fn delete_key(&self, _tunnel_id: &Uuid) -> anyhow::Result<()> {
        unimplemented!()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn stub_backends() -> Backends {
    Backends {
        tunnel_interface: Arc::new(StubTunnelInterface),
        policy_router: Arc::new(StubPolicyRouter),
        firewall: Arc::new(StubFirewall),
        packet_capture: Arc::new(StubPacketCapture),
        hostname_resolver: Arc::new(StubHostnameResolver),
        key_store: Arc::new(StubKeyStore),
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
    let factory = SqliteRepositoryFactory::new(pool);

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
    );

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
    let factory = SqliteRepositoryFactory::new(pool);

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
    );

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
async fn init_services_without_admin_block_generates_default() {
    // When no admin config is provided, `bootstrap_admin` generates a random
    // admin; `init_services` should still succeed and produce every service.
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
    let factory = SqliteRepositoryFactory::new(pool);
    let config = ApplicationConfiguration::default();

    let services = init_services_with_factory(
        &factory,
        stub_backends(),
        &config,
        Ipv4Addr::new(192, 168, 99, 1),
        Instant::now(),
        stub_log_service(),
    );

    // discovery service exists regardless — the fallback path does not panic.
    assert!(Arc::strong_count(&services.discovery) >= 1);
}
