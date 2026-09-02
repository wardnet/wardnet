//! Service-layer tests for [`ZoneEnforcementServiceImpl`].
//!
//! Builds real `SqliteNetworkZoneRepository` / `SqliteDeviceRepository` /
//! `SqliteSystemConfigRepository` over an in-memory pool (the workspace
//! migration set seeds the three system zones) and drives the enforcer against
//! a recording [`FirewallManager`] and a recording [`RoutingService`], asserting
//! the packet policy each zone implies, live re-keying, orphan cleanup, and the
//! default-policy clamp callback.

use std::collections::HashMap;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use arc_swap::ArcSwap;
use async_trait::async_trait;
use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use tokio::sync::Mutex;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;
use wardnet_common::dns::UpstreamId;
use wardnet_common::network_zone::{
    AllowedTargetKind, NetworkZone, ZoneProvenance, ZoneStance, ZoneSubnet,
};
use wardnet_common::routing::RoutingTarget;
use wardnet_common::zone_exception::{
    ExceptionEndpoint, ExceptionEndpointKind, ServiceSet, ServiceSpec, ZoneException,
};
use wardnetd_data::repository::device::DeviceRow;
use wardnetd_data::repository::{
    DeviceRepository, NetworkZoneRepository, SqliteDeviceRepository, SqliteDhcpRepository,
    SqliteNetworkZoneRepository, SqliteSystemConfigRepository, SqliteZoneExceptionRepository,
    SystemConfigRepository, ZoneExceptionRepository,
};

use crate::auth_context;
use crate::dhcp::{DhcpService, DhcpServiceImpl};
use crate::error::AppError;
use crate::event::BroadcastEventBus;
use crate::routing::RoutingService;
use crate::routing::firewall::{FirewallManager, ZoneIsolationRules, ZoneRules};
use crate::routing::policy_router::PolicyRouter;
use crate::zone_enforcement::service::{ZoneEnforcementService, ZoneEnforcementServiceImpl};

const TRUSTED: &str = "00000000-0000-0000-0000-000000000201";
const IOT: &str = "00000000-0000-0000-0000-000000000202";
const GUEST: &str = "00000000-0000-0000-0000-000000000203";
/// A manual, admin-created direct-only zone (forbids tunnel egress).
const DIRECT_ONLY: &str = "00000000-0000-0000-0000-0000000009a1";
const LAN_IFACE: &str = "eth0";

// -- Recording FirewallManager -----------------------------------------------

#[derive(Default)]
struct RecordingFirewall {
    calls: Arc<Mutex<Vec<String>>>,
    /// IPs `list_zone_rule_ips` should report (simulates rules already in kernel).
    zone_rule_ips: Arc<Mutex<Vec<String>>>,
    /// The most-recently applied full L3 isolation state, for exact assertions.
    isolation: Arc<Mutex<Option<ZoneIsolationRules>>>,
}

#[async_trait]
impl FirewallManager for RecordingFirewall {
    async fn init_wardnet_table(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn ensure_isolation_jumps(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn flush_wardnet_table(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn add_masquerade(&self, _interface: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn remove_masquerade(&self, _interface: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn add_inbound_wg_accept(&self, _port: u16) -> anyhow::Result<()> {
        Ok(())
    }

    async fn remove_inbound_wg_accept(&self) -> anyhow::Result<()> {
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
        device_ip: &str,
        rules: ZoneRules,
        lan_interface: &str,
    ) -> anyhow::Result<()> {
        self.calls.lock().await.push(format!(
            "apply:{device_ip}:direct={}:tunnel={}:adminui={}:lan={lan_interface}",
            rules.allow_direct, rules.allow_tunnel, rules.admin_ui_reachable
        ));
        Ok(())
    }
    async fn remove_zone_rules(&self, device_ip: &str) -> anyhow::Result<()> {
        self.calls.lock().await.push(format!("remove:{device_ip}"));
        Ok(())
    }
    async fn list_zone_rule_ips(&self) -> anyhow::Result<Vec<String>> {
        Ok(self.zone_rule_ips.lock().await.clone())
    }
    async fn apply_zone_isolation(&self, rules: ZoneIsolationRules) -> anyhow::Result<()> {
        self.calls.lock().await.push(format!(
            "isolation:allows={}:denies={}:members={}",
            rules.allows.len(),
            rules.deny_pairs.len(),
            rules.member_isolation_subnets.len()
        ));
        *self.isolation.lock().await = Some(rules);
        Ok(())
    }
    async fn check_tools_available(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn destroy_wardnet_table(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

// -- Recording PolicyRouter --------------------------------------------------

/// Records the #737-relevant calls (aliases, proxy-arp, host routes, conntrack
/// flushes) and returns a settable set of existing aliases from
/// `list_interface_aliases` so the stale-alias reconciler can be exercised.
#[derive(Default)]
struct RecordingPolicy {
    calls: Arc<Mutex<Vec<String>>>,
    /// Aliases `list_interface_aliases` should report (existing addrs on iface).
    existing_aliases: Arc<Mutex<Vec<(String, u8)>>>,
    /// Entries `list_neigh_proxies` should report (existing pneigh on iface).
    existing_neigh_proxies: Arc<Mutex<Vec<String>>>,
    /// When set, `list_neigh_proxies` fails, to exercise the enforcer's
    /// degrade-to-adds-only path.
    fail_list_neigh: Arc<std::sync::atomic::AtomicBool>,
    /// When set, the pneigh add/remove + proxy-arp mutations fail, to exercise
    /// the enforcer's warn-and-continue error paths.
    fail_neigh_mutations: Arc<std::sync::atomic::AtomicBool>,
    /// When set, host-route add/remove fail, so the enforcer's warn-and-continue
    /// path around `manage_host_route` is exercised (#1198).
    fail_host_routes: Arc<std::sync::atomic::AtomicBool>,
}

#[async_trait]
impl PolicyRouter for RecordingPolicy {
    async fn enable_ip_forwarding(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn add_route_table(&self, _interface: &str, _table: u32) -> anyhow::Result<()> {
        Ok(())
    }
    async fn remove_route_table(&self, _table: u32) -> anyhow::Result<()> {
        Ok(())
    }
    async fn has_route_table(&self, _table: u32) -> anyhow::Result<bool> {
        Ok(true)
    }
    async fn add_ip_rule(&self, _src_ip: &str, _table: u32, _priority: u32) -> anyhow::Result<()> {
        Ok(())
    }
    async fn remove_ip_rule(
        &self,
        _src_ip: &str,
        _table: u32,
        _priority: u32,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn list_wardnet_rules(&self) -> anyhow::Result<Vec<(String, u32, u32)>> {
        Ok(Vec::new())
    }
    async fn add_switchback_rule(
        &self,
        _src_ip: &str,
        _dst_cidr: &str,
        _priority: u32,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn remove_switchback_rule(
        &self,
        _src_ip: &str,
        _dst_cidr: &str,
        _priority: u32,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn list_switchback_rules(&self) -> anyhow::Result<Vec<(String, String, u32)>> {
        Ok(Vec::new())
    }
    async fn add_domain_route_rule(
        &self,
        _src_ip: &str,
        _dst_ip: &str,
        _table: u32,
        _priority: u32,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn remove_domain_route_rule(
        &self,
        _src_ip: &str,
        _dst_ip: &str,
        _table: u32,
        _priority: u32,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn list_domain_route_rules(
        &self,
        _priority: u32,
    ) -> anyhow::Result<Vec<(String, String, u32)>> {
        Ok(Vec::new())
    }
    async fn flush_conntrack(&self, src_ip: &str) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push(format!("flush_conntrack:{src_ip}"));
        Ok(())
    }
    async fn flush_route_cache(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn check_tools_available(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn add_interface_alias(&self, i: &str, ip: &str, p: u8) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push(format!("add_alias:{i}:{ip}/{p}"));
        Ok(())
    }
    async fn remove_interface_alias(&self, i: &str, ip: &str, p: u8) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push(format!("remove_alias:{i}:{ip}/{p}"));
        Ok(())
    }
    async fn list_interface_aliases(&self, _i: &str) -> anyhow::Result<Vec<(String, u8)>> {
        Ok(self.existing_aliases.lock().await.clone())
    }
    async fn set_proxy_arp(&self, i: &str, e: bool) -> anyhow::Result<()> {
        self.calls.lock().await.push(format!("proxy_arp:{i}:{e}"));
        if self
            .fail_neigh_mutations
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            anyhow::bail!("mock proxy_arp failure");
        }
        Ok(())
    }
    async fn add_neigh_proxy(&self, ip: &str, i: &str) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push(format!("add_neigh_proxy:{ip}:{i}"));
        if self
            .fail_neigh_mutations
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            anyhow::bail!("mock add_neigh_proxy failure");
        }
        Ok(())
    }
    async fn remove_neigh_proxy(&self, ip: &str, i: &str) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push(format!("remove_neigh_proxy:{ip}:{i}"));
        if self
            .fail_neigh_mutations
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            anyhow::bail!("mock remove_neigh_proxy failure");
        }
        Ok(())
    }
    async fn list_neigh_proxies(&self, _i: &str) -> anyhow::Result<Vec<String>> {
        if self
            .fail_list_neigh
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            anyhow::bail!("mock list_neigh_proxies failure");
        }
        Ok(self.existing_neigh_proxies.lock().await.clone())
    }
    async fn add_host_route(
        &self,
        ip: &str,
        i: &str,
        pref_src: std::net::Ipv4Addr,
    ) -> anyhow::Result<()> {
        self.calls
            .lock()
            .await
            .push(format!("add_host_route:{ip}:{i}:{pref_src}"));
        if self
            .fail_host_routes
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            anyhow::bail!("mock add_host_route failure");
        }
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

// -- Recording RoutingService (only apply_rule_for_device is exercised) ------

#[derive(Default)]
struct RecordingRouting {
    /// `<device_id>=<target-debug>` for each clamp callback.
    clamps: Arc<Mutex<Vec<String>>>,
    /// `<device_id>=<device_ip>=[<cidr>,...]` for each switchback-target push.
    switchback: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl RoutingService for RecordingRouting {
    async fn apply_rule(
        &self,
        _device_id: Uuid,
        _device_ip: &str,
        _target: &RoutingTarget,
    ) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn remove_device_routes(
        &self,
        _device_id: Uuid,
        _device_ip: &str,
    ) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn handle_ip_change(
        &self,
        _device_id: Uuid,
        _old_ip: &str,
        _new_ip: &str,
    ) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn handle_tunnel_down(&self, _tunnel_id: Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn handle_tunnel_up(&self, _tunnel_id: Uuid) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn reconcile(&self) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn handle_route_table_lost(&self, _table: u32) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn devices_using_tunnel(&self, _tunnel_id: Uuid) -> Result<Vec<Uuid>, AppError> {
        unimplemented!()
    }
    async fn apply_rule_for_device(
        &self,
        device_id: Uuid,
        target: &RoutingTarget,
    ) -> Result<(), AppError> {
        self.clamps
            .lock()
            .await
            .push(format!("{device_id}={target:?}"));
        Ok(())
    }
    async fn apply_rule_for_discovered_device(
        &self,
        _device_id: Uuid,
        _ip: &str,
    ) -> Result<(), AppError> {
        unimplemented!()
    }
    #[allow(clippy::similar_names)]
    async fn set_switchback_targets(
        &self,
        device_id: Uuid,
        device_ip: String,
        target_cidrs: Vec<String>,
    ) -> Result<(), AppError> {
        self.switchback
            .lock()
            .await
            .push(format!("{device_id}={device_ip}={target_cidrs:?}"));
        Ok(())
    }
    async fn route_resolved_domain(
        &self,
        _device_ip: &str,
        _resolved_ips: &[std::net::IpAddr],
        _target: &wardnet_common::routing_profile::DomainRoutingTarget,
        _ttl_secs: u32,
    ) -> Result<(), AppError> {
        Ok(())
    }
    async fn gc_domain_routes(&self) -> Result<(), AppError> {
        Ok(())
    }
    async fn set_default_policy(&self, _policy: &str) -> Result<(), AppError> {
        unimplemented!()
    }
    async fn default_policy(&self) -> Result<String, AppError> {
        unimplemented!()
    }
    async fn handle_default_policy_changed(&self) -> Result<(), AppError> {
        unimplemented!()
    }
    fn dns_upstream_snapshot(&self) -> Arc<ArcSwap<HashMap<IpAddr, UpstreamId>>> {
        unimplemented!()
    }
    async fn rebuild_dns_upstream_snapshot(&self) -> Result<(), AppError> {
        unimplemented!()
    }
    fn dns_device_upstream_snapshot(&self) -> Arc<ArcSwap<HashMap<Uuid, UpstreamId>>> {
        unimplemented!()
    }
    async fn rebuild_dns_device_upstream_snapshot(&self) -> Result<(), AppError> {
        unimplemented!()
    }
}

// -- Harness -----------------------------------------------------------------

/// The LAN IP the harness uses; base subnet is `192.168.1.0/24`.
const LAN_IP: Ipv4Addr = Ipv4Addr::new(192, 168, 1, 1);

struct Harness {
    svc: ZoneEnforcementServiceImpl,
    zones: Arc<dyn NetworkZoneRepository>,
    devices: Arc<dyn DeviceRepository>,
    system_config: Arc<dyn SystemConfigRepository>,
    exceptions: Arc<dyn ZoneExceptionRepository>,
    fw_calls: Arc<Mutex<Vec<String>>>,
    fw_zone_ips: Arc<Mutex<Vec<String>>>,
    fw_isolation: Arc<Mutex<Option<ZoneIsolationRules>>>,
    policy_calls: Arc<Mutex<Vec<String>>>,
    existing_aliases: Arc<Mutex<Vec<(String, u8)>>>,
    existing_neigh_proxies: Arc<Mutex<Vec<String>>>,
    fail_list_neigh: Arc<std::sync::atomic::AtomicBool>,
    fail_neigh_mutations: Arc<std::sync::atomic::AtomicBool>,
    fail_host_routes: Arc<std::sync::atomic::AtomicBool>,
    clamps: Arc<Mutex<Vec<String>>>,
    switchback: Arc<Mutex<Vec<String>>>,
}

async fn test_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../wardnetd-data/migrations")
        .run(&pool)
        .await
        .unwrap();
    pool
}

async fn build() -> Harness {
    let pool = test_pool().await;
    let zones: Arc<dyn NetworkZoneRepository> =
        Arc::new(SqliteNetworkZoneRepository::new(pool.clone()));
    let devices: Arc<dyn DeviceRepository> = Arc::new(SqliteDeviceRepository::new(pool.clone()));
    let system_config: Arc<dyn SystemConfigRepository> =
        Arc::new(SqliteSystemConfigRepository::new(pool.clone()));
    let exceptions: Arc<dyn ZoneExceptionRepository> =
        Arc::new(SqliteZoneExceptionRepository::new(pool.clone()));

    let fw_calls = Arc::new(Mutex::new(Vec::new()));
    let fw_zone_ips = Arc::new(Mutex::new(Vec::new()));
    let fw_isolation = Arc::new(Mutex::new(None));
    let firewall: Arc<dyn FirewallManager> = Arc::new(RecordingFirewall {
        calls: fw_calls.clone(),
        zone_rule_ips: fw_zone_ips.clone(),
        isolation: fw_isolation.clone(),
    });
    let policy_calls = Arc::new(Mutex::new(Vec::new()));
    let existing_aliases = Arc::new(Mutex::new(Vec::new()));
    let existing_neigh_proxies = Arc::new(Mutex::new(Vec::new()));
    let fail_list_neigh = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let fail_neigh_mutations = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let fail_host_routes = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let policy_router: Arc<dyn PolicyRouter> = Arc::new(RecordingPolicy {
        calls: policy_calls.clone(),
        existing_aliases: existing_aliases.clone(),
        existing_neigh_proxies: existing_neigh_proxies.clone(),
        fail_list_neigh: fail_list_neigh.clone(),
        fail_neigh_mutations: fail_neigh_mutations.clone(),
        fail_host_routes: fail_host_routes.clone(),
    });
    let clamps = Arc::new(Mutex::new(Vec::new()));
    let switchback = Arc::new(Mutex::new(Vec::new()));
    let routing: Arc<dyn RoutingService> = Arc::new(RecordingRouting {
        clamps: clamps.clone(),
        switchback: switchback.clone(),
    });

    // A real DHCP service over the same in-memory pool so `release_lease` and
    // scope resolution behave; only its lease-release side effect is asserted.
    let events = Arc::new(BroadcastEventBus::new(64));
    let dhcp_repo = Arc::new(SqliteDhcpRepository::new(pool));
    let dhcp: Arc<dyn DhcpService> = Arc::new(DhcpServiceImpl::new(
        dhcp_repo,
        system_config.clone(),
        events,
        devices.clone(),
        zones.clone(),
        LAN_IP,
    ));

    let svc = ZoneEnforcementServiceImpl::new(
        zones.clone(),
        devices.clone(),
        system_config.clone(),
        exceptions.clone(),
        firewall,
        policy_router,
        routing,
        dhcp,
        LAN_IFACE.to_owned(),
        LAN_IP,
    );

    Harness {
        svc,
        zones,
        devices,
        system_config,
        exceptions,
        fw_calls,
        fw_zone_ips,
        fw_isolation,
        policy_calls,
        existing_aliases,
        existing_neigh_proxies,
        fail_list_neigh,
        fail_neigh_mutations,
        fail_host_routes,
        clamps,
        switchback,
    }
}

async fn as_admin<F: Future>(fut: F) -> F::Output {
    auth_context::with_context(AuthContext::system(), fut).await
}

/// Insert a device with a fixed MAC/IP into `zone_id` and return its id.
async fn insert_device(devices: &Arc<dyn DeviceRepository>, ip: &str, zone_id: &str) -> Uuid {
    let id = Uuid::new_v4();
    let mac = format!(
        "02:00:00:00:{:02x}:{:02x}",
        id.as_bytes()[0],
        id.as_bytes()[1]
    );
    devices
        .insert(&DeviceRow {
            id: id.to_string(),
            mac,
            hostname: None,
            manufacturer: None,
            manufacturer_source: None,
            is_randomized: false,
            device_type: "unknown".to_owned(),
            first_seen: "2026-07-01T00:00:00Z".to_owned(),
            last_seen: "2026-07-01T00:00:00Z".to_owned(),
            last_ip: ip.to_owned(),
            zone_id: zone_id.to_owned(),
            connection_mode: wardnet_common::device::DeviceConnectionMode::Lan,
        })
        .await
        .unwrap();
    id
}

/// Insert a manual zone with the given `allowed_targets` + admin-UI flag.
async fn insert_zone(
    zones: &Arc<dyn NetworkZoneRepository>,
    id: &str,
    name: &str,
    allowed: Vec<AllowedTargetKind>,
    admin_ui_reachable: bool,
) {
    let now = chrono::Utc::now();
    zones
        .insert(&NetworkZone {
            id: id.parse().unwrap(),
            name: name.to_owned(),
            provenance: ZoneProvenance::Manual,
            isolation_stance: ZoneStance::SharedSubnet,
            allowed_targets: allowed,
            member_isolation: false,
            subnet: None,
            admin_ui_reachable,
            is_default: false,
            is_default_for_new: false,
            created_at: now,
            updated_at: now,
        })
        .await
        .unwrap();
}

async fn calls(h: &Harness) -> Vec<String> {
    h.fw_calls.lock().await.clone()
}

async fn policy_calls(h: &Harness) -> Vec<String> {
    h.policy_calls.lock().await.clone()
}

/// Insert a manual zone with a subnet and an optional `member_isolation` flag.
async fn insert_subnet_zone(
    zones: &Arc<dyn NetworkZoneRepository>,
    id: &str,
    name: &str,
    cidr: &str,
    member_isolation: bool,
) {
    let now = chrono::Utc::now();
    zones
        .insert(&NetworkZone {
            id: id.parse().unwrap(),
            name: name.to_owned(),
            provenance: ZoneProvenance::Manual,
            isolation_stance: ZoneStance::IsolateMembers,
            allowed_targets: vec![AllowedTargetKind::Direct, AllowedTargetKind::Tunnel],
            member_isolation,
            subnet: Some(ZoneSubnet {
                cidr: cidr.to_owned(),
            }),
            admin_ui_reachable: true,
            is_default: false,
            is_default_for_new: false,
            created_at: now,
            updated_at: now,
        })
        .await
        .unwrap();
}

/// Turn Wardnet-DHCP-mode on so the L3 isolation surface is live.
async fn enable_dhcp(h: &Harness) {
    h.system_config.set("dhcp_enabled", "true").await.unwrap();
}

/// The most-recently applied full L3 isolation state.
async fn isolation(h: &Harness) -> ZoneIsolationRules {
    h.fw_isolation
        .lock()
        .await
        .clone()
        .expect("apply_zone_isolation was called")
}

const ZONE_A: &str = "00000000-0000-0000-0000-0000000009c1";
const ZONE_B: &str = "00000000-0000-0000-0000-0000000009c2";

/// The manual tunnel-only zone (forbids direct egress) used to exercise the
/// "no valid fallback" clamp branch.
const TUNNEL_ONLY: &str = "00000000-0000-0000-0000-0000000009b2";

// -- Tests -------------------------------------------------------------------

#[tokio::test]
async fn apply_zone_reapplies_only_its_members() {
    let h = build().await;
    insert_zone(
        &h.zones,
        DIRECT_ONLY,
        "DirectOnly",
        vec![AllowedTargetKind::Direct],
        true,
    )
    .await;
    let _m1 = insert_device(&h.devices, "192.168.1.200", DIRECT_ONLY).await;
    let _m2 = insert_device(&h.devices, "192.168.1.201", DIRECT_ONLY).await;
    let _other = insert_device(&h.devices, "192.168.1.202", GUEST).await;

    as_admin(h.svc.apply_zone(DIRECT_ONLY.parse().unwrap()))
        .await
        .unwrap();

    let c = calls(&h).await;
    assert!(
        c.contains(
            &"apply:192.168.1.200:direct=true:tunnel=false:adminui=true:lan=eth0".to_owned()
        ),
        "member 1 re-applied: {c:?}"
    );
    assert!(
        c.contains(
            &"apply:192.168.1.201:direct=true:tunnel=false:adminui=true:lan=eth0".to_owned()
        ),
        "member 2 re-applied: {c:?}"
    );
    assert!(
        !c.iter().any(|x| x.starts_with("apply:192.168.1.202:")),
        "a device in a different zone is left alone: {c:?}"
    );
}

#[tokio::test]
async fn apply_zone_unknown_zone_is_noop() {
    let h = build().await;
    let _dev = insert_device(&h.devices, "192.168.1.210", GUEST).await;

    as_admin(h.svc.apply_zone(Uuid::new_v4())).await.unwrap();

    assert!(
        calls(&h).await.is_empty(),
        "a deleted/unknown zone installs nothing"
    );
}

#[tokio::test]
async fn default_policy_flip_leaves_tunnel_only_zone_unclamped() {
    let h = build().await;
    // A tunnel-only zone forbids direct, so a `direct` policy has no valid
    // fallback: the enforcer leaves the binding for the packet-layer drop.
    insert_zone(
        &h.zones,
        TUNNEL_ONLY,
        "TunnelOnly",
        vec![AllowedTargetKind::Tunnel],
        true,
    )
    .await;
    let dev = insert_device(&h.devices, "192.168.1.220", TUNNEL_ONLY).await;
    let default_json = serde_json::to_string(&RoutingTarget::Default).unwrap();
    h.devices
        .upsert_user_rule(&dev.to_string(), &default_json, "2026-07-01T00:00:00Z")
        .await
        .unwrap();

    as_admin(h.svc.handle_default_policy_changed("direct"))
        .await
        .unwrap();

    assert!(
        h.clamps.lock().await.is_empty(),
        "tunnel-only zone under a direct policy has no direct fallback, so nothing is clamped"
    );
}

#[tokio::test]
async fn apply_device_maps_seed_zone_to_packet_policy() {
    let h = build().await;
    // Guest: allows both targets, admin-UI reachable.
    let guest = insert_device(&h.devices, "192.168.1.50", GUEST).await;
    // IoT: allows both targets, admin-UI *not* reachable.
    let iot = insert_device(&h.devices, "192.168.1.51", IOT).await;

    as_admin(h.svc.apply_device(guest)).await.unwrap();
    as_admin(h.svc.apply_device(iot)).await.unwrap();

    let c = calls(&h).await;
    assert!(
        c.contains(&"apply:192.168.1.50:direct=true:tunnel=true:adminui=true:lan=eth0".to_owned()),
        "guest maps to fully-permissive, reachable: {c:?}"
    );
    assert!(
        c.contains(&"apply:192.168.1.51:direct=true:tunnel=true:adminui=false:lan=eth0".to_owned()),
        "IoT maps to admin-UI-unreachable: {c:?}"
    );
}

#[tokio::test]
async fn apply_device_direct_only_zone_forbids_tunnel() {
    let h = build().await;
    insert_zone(
        &h.zones,
        DIRECT_ONLY,
        "DirectOnly",
        vec![AllowedTargetKind::Direct],
        true,
    )
    .await;
    let dev = insert_device(&h.devices, "192.168.1.60", DIRECT_ONLY).await;

    as_admin(h.svc.apply_device(dev)).await.unwrap();

    let c = calls(&h).await;
    assert!(
        c.contains(&"apply:192.168.1.60:direct=true:tunnel=false:adminui=true:lan=eth0".to_owned()),
        "direct-only zone forbids tunnel egress: {c:?}"
    );
}

#[tokio::test]
async fn handle_ip_change_rekeys_rules() {
    let h = build().await;
    let dev = insert_device(&h.devices, "192.168.1.70", IOT).await;

    as_admin(h.svc.handle_ip_change(dev, "192.168.1.70", "192.168.1.71"))
        .await
        .unwrap();

    let c = calls(&h).await;
    assert_eq!(
        c.first().unwrap(),
        "remove:192.168.1.70",
        "old IP dropped first: {c:?}"
    );
    assert!(
        c.iter().any(|call| call.starts_with("apply:192.168.1.71:")),
        "new IP rules installed: {c:?}"
    );
}

#[tokio::test]
async fn remove_device_tears_down_rules() {
    let h = build().await;
    let dev = insert_device(&h.devices, "192.168.1.80", GUEST).await;

    as_admin(h.svc.remove_device(dev, "192.168.1.80"))
        .await
        .unwrap();

    // The per-device rule teardown happens first; remove_device also recomputes
    // the whole L3 isolation state (empty here — DHCP is off).
    let c = calls(&h).await;
    assert_eq!(
        c.first().unwrap(),
        "remove:192.168.1.80",
        "rules torn down: {c:?}"
    );
    assert!(
        c.iter().any(|x| x.starts_with("isolation:")),
        "isolation recomputed: {c:?}"
    );
}

#[tokio::test]
async fn reconcile_applies_all_devices_and_drops_orphans() {
    let h = build().await;
    let _d1 = insert_device(&h.devices, "192.168.1.90", GUEST).await;
    let _d2 = insert_device(&h.devices, "192.168.1.91", IOT).await;
    // Simulate a stale rule for an IP no longer backed by any device.
    h.fw_zone_ips
        .lock()
        .await
        .extend(["192.168.1.90".to_owned(), "10.9.9.9".to_owned()]);

    as_admin(h.svc.reconcile()).await.unwrap();

    let c = calls(&h).await;
    assert!(
        c.iter().any(|x| x.starts_with("apply:192.168.1.90:")),
        "live device applied: {c:?}"
    );
    assert!(
        c.iter().any(|x| x.starts_with("apply:192.168.1.91:")),
        "live device applied: {c:?}"
    );
    assert!(
        c.contains(&"remove:10.9.9.9".to_owned()),
        "orphan IP cleaned up: {c:?}"
    );
    assert!(
        !c.contains(&"remove:192.168.1.90".to_owned()),
        "live IP is not treated as an orphan: {c:?}"
    );
}

#[tokio::test]
async fn default_policy_flip_clamps_forbidden_binding() {
    let h = build().await;
    insert_zone(
        &h.zones,
        DIRECT_ONLY,
        "DirectOnly",
        vec![AllowedTargetKind::Direct],
        true,
    )
    .await;
    // A direct-only device on a Default rule, and a Trusted device on Default.
    let direct_dev = insert_device(&h.devices, "192.168.1.100", DIRECT_ONLY).await;
    let trusted_dev = insert_device(&h.devices, "192.168.1.101", TRUSTED).await;
    let default_json = serde_json::to_string(&RoutingTarget::Default).unwrap();
    h.devices
        .upsert_user_rule(
            &direct_dev.to_string(),
            &default_json,
            "2026-07-01T00:00:00Z",
        )
        .await
        .unwrap();
    h.devices
        .upsert_user_rule(
            &trusted_dev.to_string(),
            &default_json,
            "2026-07-01T00:00:00Z",
        )
        .await
        .unwrap();

    // Policy flips to a tunnel: the direct-only device's Default now resolves to
    // a forbidden tunnel, so it must be clamped; the Trusted device is fine.
    let tunnel_policy = Uuid::new_v4().to_string();
    as_admin(h.svc.handle_default_policy_changed(&tunnel_policy))
        .await
        .unwrap();

    let clamps = h.clamps.lock().await.clone();
    assert_eq!(clamps.len(), 1, "exactly one device clamped: {clamps:?}");
    assert!(
        clamps[0].starts_with(&direct_dev.to_string()) && clamps[0].contains("Direct"),
        "direct-only device pinned to Direct: {clamps:?}"
    );
}

#[tokio::test]
async fn default_policy_flip_to_direct_clamps_nothing() {
    let h = build().await;
    let dev = insert_device(&h.devices, "192.168.1.110", TRUSTED).await;
    let default_json = serde_json::to_string(&RoutingTarget::Default).unwrap();
    h.devices
        .upsert_user_rule(&dev.to_string(), &default_json, "2026-07-01T00:00:00Z")
        .await
        .unwrap();
    // Keep system_config's default_policy consistent (unused by this path, but
    // documents intent).
    h.system_config.set_default_policy("direct").await.unwrap();

    as_admin(h.svc.handle_default_policy_changed("direct"))
        .await
        .unwrap();

    assert!(
        h.clamps.lock().await.is_empty(),
        "a direct policy is permitted by every seed zone, so nothing is clamped"
    );
}

// -- Issue #737: L3 isolation ------------------------------------------------

#[tokio::test]
async fn two_subnet_zones_deny_all_ordered_cross_subnet_pairs() {
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "ZoneA", "10.44.1.0/24", false).await;
    insert_subnet_zone(&h.zones, ZONE_B, "ZoneB", "10.44.2.0/24", false).await;

    as_admin(h.svc.handle_exceptions_changed()).await.unwrap();

    let rules = isolation(&h).await;
    let base = "192.168.1.0/24".to_owned();
    let a = "10.44.1.0/24".to_owned();
    let b = "10.44.2.0/24".to_owned();
    // Both directions between the two zone subnets.
    assert!(rules.deny_pairs.contains(&(a.clone(), b.clone())));
    assert!(rules.deny_pairs.contains(&(b.clone(), a.clone())));
    // Both directions between each zone subnet and the base subnet.
    assert!(rules.deny_pairs.contains(&(a.clone(), base.clone())));
    assert!(rules.deny_pairs.contains(&(base.clone(), a.clone())));
    assert!(rules.deny_pairs.contains(&(b.clone(), base.clone())));
    assert!(rules.deny_pairs.contains(&(base, b)));
    // 3 subnets ⇒ 3*2 ordered pairs.
    assert_eq!(rules.deny_pairs.len(), 6, "{:?}", rules.deny_pairs);
}

#[tokio::test]
async fn casting_exception_yields_bidirectional_allows() {
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "ZoneA", "10.44.1.0/24", false).await;
    insert_subnet_zone(&h.zones, ZONE_B, "ZoneB", "10.44.2.0/24", false).await;
    // A phone in zone A casting to a TV in zone B.
    let phone = insert_device(&h.devices, "10.44.1.10", ZONE_A).await;
    let tv = insert_device(&h.devices, "10.44.2.20", ZONE_B).await;
    let now = chrono::Utc::now();
    h.exceptions
        .insert(&ZoneException {
            id: Uuid::new_v4(),
            from: ExceptionEndpoint {
                kind: ExceptionEndpointKind::Device,
                id: phone,
            },
            to: ExceptionEndpoint {
                kind: ExceptionEndpointKind::Device,
                id: tv,
            },
            service: ServiceSpec::Preset {
                set: ServiceSet::Casting,
            },
            bidirectional: true,
            created_at: now,
            updated_at: now,
        })
        .await
        .unwrap();

    as_admin(h.svc.handle_exceptions_changed()).await.unwrap();

    let rules = isolation(&h).await;
    // Every allow is from the phone /32 to the TV /32 (the resolved endpoints).
    let from = "10.44.1.10/32";
    let to = "10.44.2.20/32";
    // 5353/udp (mDNS) present in the forward direction.
    assert!(
        rules.allows.iter().any(|a| a.from_cidr == from
            && a.to_cidr == to
            && a.proto == "udp"
            && a.port_start == 5353),
        "mDNS allow present: {:?}",
        rules.allows
    );
    // 8009/tcp (Chromecast) present in the forward direction.
    assert!(
        rules.allows.iter().any(|a| a.from_cidr == from
            && a.to_cidr == to
            && a.proto == "tcp"
            && a.port_start == 8009),
        "Chromecast allow present: {:?}",
        rules.allows
    );
    // Every allow carries the bidirectional flag so the firewall renders both
    // directions.
    assert!(rules.allows.iter().all(|a| a.bidirectional));
    // One allow per Casting port.
    assert_eq!(rules.allows.len(), ServiceSet::Casting.ports().len());
    // The rebuild landed (allows-before-denies ordering is the firewall's job).
    assert!(
        calls(&h)
            .await
            .iter()
            .any(|c| c.starts_with("isolation:allows=")),
        "a chain rebuild was triggered"
    );
}

/// A host-route failure must be warn-logged, never fatal: the device's packet
/// rules and the zone's isolation state are already applied by the time the
/// route is managed, so aborting there would leave enforcement half-applied
/// with no retry (#1198).
#[tokio::test]
async fn host_route_failure_is_warned_not_fatal() {
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "IsoZone", "10.44.1.0/24", true).await;
    let member = insert_device(&h.devices, "10.44.1.10", ZONE_A).await;
    h.fail_host_routes
        .store(true, std::sync::atomic::Ordering::SeqCst);

    as_admin(h.svc.apply_device(member))
        .await
        .expect("a failing host route must not fail the whole apply");

    let pc = policy_calls(&h).await;
    assert!(
        pc.iter()
            .any(|c| c.starts_with("add_host_route:10.44.1.10")),
        "the add was still attempted: {pc:?}"
    );
}

/// A zone whose CIDR won't parse yields no gateway, so there is no preferred
/// source to install and the host route must be skipped rather than added
/// without one — adding it without a source is the #1198 bug itself. Write-time
/// validation rejects such a CIDR, so this only guards a direct DB edit or
/// older data, but the fallback has to be the safe direction.
#[tokio::test]
async fn unparseable_zone_subnet_installs_no_host_route() {
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "BadZone", "not-a-cidr", true).await;
    let member = insert_device(&h.devices, "10.44.1.10", ZONE_A).await;

    as_admin(h.svc.apply_device(member)).await.unwrap();

    let pc = policy_calls(&h).await;
    assert!(
        !pc.iter().any(|c| c.starts_with("add_host_route:")),
        "an unparseable zone subnet must not produce a host route: {pc:?}"
    );
}

/// `handle_ip_change` must drop the old address's `/32` and install one for the
/// new address, preferring the zone gateway — the path a device takes when it
/// re-DHCPs into its zone's subnet, which is exactly how a phone acquires the
/// route in production (#1198).
#[tokio::test]
async fn handle_ip_change_rekeys_the_host_route_onto_the_new_ip() {
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "IsoZone", "10.44.1.0/24", true).await;
    let member = insert_device(&h.devices, "10.44.1.10", ZONE_A).await;

    as_admin(h.svc.handle_ip_change(member, "10.44.1.10", "10.44.1.11"))
        .await
        .unwrap();

    let pc = policy_calls(&h).await;
    assert!(
        pc.contains(&"remove_host_route:10.44.1.10:eth0".to_owned()),
        "the stale IP's host route is dropped: {pc:?}"
    );
    assert!(
        pc.contains(&"add_host_route:10.44.1.11:eth0:10.44.1.1".to_owned()),
        "the new IP gets a host route preferring the zone gateway: {pc:?}"
    );
}

/// A zone move leaves the device on its *old* address — `handle_zone_change`
/// has only just released the lease — so the in-subnet guard yields no gateway
/// and the stale `/32` is dropped rather than re-pointed at the new zone's
/// gateway. That is the safe direction: a route naming a gateway the device is
/// not behind is the #1198 failure itself. The route comes back via
/// `handle_ip_change` once the device re-DHCPs into the new subnet.
#[tokio::test]
async fn handle_zone_change_drops_the_host_route_until_the_device_re_ips() {
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "IsoZone", "10.44.1.0/24", true).await;
    // Still on its previous address, as a freshly-moved device would be.
    let member = insert_device(&h.devices, "192.168.100.50", ZONE_A).await;

    as_admin(h.svc.handle_zone_change(member)).await.unwrap();

    let pc = policy_calls(&h).await;
    assert!(
        !pc.iter().any(|c| c.starts_with("add_host_route:")),
        "a device still on its old address must not get a /32 pointing at the \
         new zone's gateway (#1198): {pc:?}"
    );
    assert!(
        pc.contains(&"remove_host_route:192.168.100.50:eth0".to_owned()),
        "the stale route is dropped: {pc:?}"
    );
}

/// The other half: once the device *is* inside its zone's subnet, a zone change
/// re-applies the route with that zone's gateway.
#[tokio::test]
async fn handle_zone_change_installs_the_host_route_for_an_in_subnet_member() {
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "IsoZone", "10.44.1.0/24", true).await;
    let member = insert_device(&h.devices, "10.44.1.10", ZONE_A).await;

    as_admin(h.svc.handle_zone_change(member)).await.unwrap();

    let pc = policy_calls(&h).await;
    assert!(
        pc.contains(&"add_host_route:10.44.1.10:eth0:10.44.1.1".to_owned()),
        "an in-subnet member's route is re-applied for its zone: {pc:?}"
    );
}

/// A device whose address is outside its member-isolated zone's subnet must get
/// no host route at all.
///
/// It keeps its old base-subnet address until it re-DHCPs, so a `/32` naming
/// this zone's gateway as preferred source would make the daemon's replies
/// leave with an address outside the device's own subnet — the same
/// wrong-source-IP failure the preferred source exists to fix (#1198). The
/// proxy-neighbour path already applies exactly this in-subnet guard.
#[tokio::test]
async fn out_of_subnet_member_gets_no_host_route() {
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "IsoZone", "10.44.1.0/24", true).await;
    // Still on the base subnet, not yet re-DHCPed into 10.44.1.0/24.
    let stale = insert_device(&h.devices, "192.168.100.50", ZONE_A).await;

    as_admin(h.svc.apply_device(stale)).await.unwrap();

    let pc = policy_calls(&h).await;
    assert!(
        !pc.iter()
            .any(|c| c.starts_with("add_host_route:192.168.100.50")),
        "an out-of-subnet device must not get a /32 sourced from a zone \
         gateway it is not behind (#1198): {pc:?}"
    );
}

/// A zone edit moves the gateway alias, leaving every member's `/32` naming an
/// address that is no longer local — which the kernel then drops, silently
/// removing the on-link path member isolation needs. `apply_zone` raises no
/// per-device event, so it must re-manage the routes itself (#1198).
#[tokio::test]
async fn apply_zone_reasserts_member_host_routes() {
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "IsoZone", "10.44.1.0/24", true).await;
    insert_device(&h.devices, "10.44.1.10", ZONE_A).await;

    as_admin(h.svc.apply_zone(ZONE_A.parse().unwrap()))
        .await
        .unwrap();

    let pc = policy_calls(&h).await;
    assert!(
        pc.contains(&"add_host_route:10.44.1.10:eth0:10.44.1.1".to_owned()),
        "a zone edit must re-assert its members' host routes, or they keep a \
         preferred source the alias reconcile just removed (#1198): {pc:?}"
    );
}

/// Regression test for #1198.
///
/// Every other host-route call site is device-event-driven (`apply_device`,
/// `handle_ip_change`, `handle_zone_change`). Older builds installed these
/// `/32`s with no preferred source, so a box upgrading into the fix would keep
/// its broken routes — and its member-isolated devices would stay without DNS —
/// until each device happened to re-DHCP or an admin touched it. Startup
/// reconcile must re-assert the host route for every member-isolated device so
/// the upgrade alone repairs the box.
#[tokio::test]
async fn reconcile_reasserts_member_host_routes_with_the_zone_gateway() {
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "IsoZone", "10.44.1.0/24", true).await;
    insert_device(&h.devices, "10.44.1.10", ZONE_A).await;

    as_admin(h.svc.reconcile()).await.unwrap();

    let pc = policy_calls(&h).await;
    assert!(
        pc.contains(&"add_host_route:10.44.1.10:eth0:10.44.1.1".to_owned()),
        "startup reconcile must re-assert the member host route so an upgrade \
         heals a box carrying prefsrc-less routes (#1198): {pc:?}"
    );
}

/// The host route names the zone gateway as its preferred source, and the
/// kernel rejects a route whose `RTA_PREFSRC` is not a local address. So the
/// gateway alias — installed by `reconcile_isolation` — must be in place before
/// the route is added, on every path that does both.
#[tokio::test]
async fn gateway_alias_is_installed_before_the_host_route_that_prefers_it() {
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "IsoZone", "10.44.1.0/24", true).await;
    let member = insert_device(&h.devices, "10.44.1.10", ZONE_A).await;

    as_admin(h.svc.apply_device(member)).await.unwrap();

    let pc = policy_calls(&h).await;
    let alias = pc
        .iter()
        .position(|c| c.starts_with("add_alias:eth0:10.44.1.1/"));
    let route = pc
        .iter()
        .position(|c| c.starts_with("add_host_route:10.44.1.10"));
    let (Some(alias), Some(route)) = (alias, route) else {
        panic!("expected both a gateway alias and a host route: {pc:?}");
    };
    assert!(
        alias < route,
        "the zone gateway alias must exist before a route prefers it as source, \
         or the kernel rejects the add with EINVAL (#1198): {pc:?}"
    );
}

#[tokio::test]
async fn member_isolation_adds_proxy_neigh_entry_not_interface_proxy_arp() {
    // Issue #1107: interface-wide `proxy_arp=1` made the Pi answer ARP for ANY
    // address a tunnel-bound device probed (macOS "duplicate IP", LAN-peer
    // hijack) while never firing for the intended same-interface case. Member
    // isolation must install a per-member pneigh entry instead, and must never
    // enable the interface-wide sysctl.
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "IsoZone", "10.44.1.0/24", true).await;
    let member = insert_device(&h.devices, "10.44.1.10", ZONE_A).await;

    as_admin(h.svc.apply_device(member)).await.unwrap();

    let rules = isolation(&h).await;
    assert!(
        rules
            .member_isolation_subnets
            .contains(&"10.44.1.0/24".to_owned()),
        "member subnet listed: {:?}",
        rules.member_isolation_subnets
    );
    let pc = policy_calls(&h).await;
    assert!(
        pc.contains(&"add_neigh_proxy:10.44.1.10:eth0".to_owned()),
        "per-member proxy-neighbour entry added: {pc:?}"
    );
    assert!(
        !pc.contains(&"proxy_arp:eth0:true".to_owned()),
        "interface-wide proxy-arp must never be enabled (#1107): {pc:?}"
    );
    assert!(
        pc.contains(&"add_host_route:10.44.1.10:eth0:10.44.1.1".to_owned()),
        "member host route added, preferring the zone gateway as source (#1198): {pc:?}"
    );
}

#[tokio::test]
async fn out_of_subnet_member_gets_no_proxy_neigh_entry() {
    // A device freshly moved into an isolate-members zone can still hold its
    // old base-subnet address until it re-DHCPs. A pneigh entry for that
    // address would make the Pi answer ARP for a base-subnet IP the DHCP pool
    // may re-lease — the duplicate-IP failure of #1107, re-created. Only
    // in-subnet members get entries.
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "IsoZone", "10.44.1.0/24", true).await;
    let straggler = insert_device(&h.devices, "192.168.1.50", ZONE_A).await;

    as_admin(h.svc.apply_device(straggler)).await.unwrap();

    let pc = policy_calls(&h).await;
    assert!(
        !pc.iter().any(|c| c.starts_with("add_neigh_proxy:")),
        "no pneigh entry for an out-of-subnet member address: {pc:?}"
    );
}

#[tokio::test]
async fn proxy_neigh_entries_self_heal_against_kernel_state() {
    // The kernel purges pneigh entries on a link flap, and a netlink call can
    // fail transiently — so the reconcile must diff against the kernel's own
    // listing every pass, not a stored snapshot. With the kernel still
    // reporting no entries, a second identical reconcile must re-add.
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "IsoZone", "10.44.1.0/24", true).await;
    let member = insert_device(&h.devices, "10.44.1.10", ZONE_A).await;

    as_admin(h.svc.apply_device(member)).await.unwrap();
    // The mock kernel never records the add (list stays empty), simulating a
    // purge/failed apply; an unchanged second pass must retry, not skip.
    as_admin(h.svc.apply_device(member)).await.unwrap();

    let adds = policy_calls(&h)
        .await
        .into_iter()
        .filter(|c| c == "add_neigh_proxy:10.44.1.10:eth0")
        .count();
    assert_eq!(adds, 2, "each pass re-converges against kernel state");
}

#[tokio::test]
async fn unusable_and_own_address_members_get_no_proxy_neigh_entry() {
    // A member row without a parseable IP (a repaired #886 row) and a member
    // row claiming the zone's own gateway address are both skipped — only the
    // real in-subnet member gets a pneigh entry.
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "IsoZone", "10.44.1.0/24", true).await;
    let member = insert_device(&h.devices, "10.44.1.10", ZONE_A).await;
    insert_device(&h.devices, "", ZONE_A).await;
    insert_device(&h.devices, "10.44.1.1", ZONE_A).await;

    as_admin(h.svc.apply_device(member)).await.unwrap();

    let adds: Vec<String> = policy_calls(&h)
        .await
        .into_iter()
        .filter(|c| c.starts_with("add_neigh_proxy:"))
        .collect();
    assert_eq!(
        adds,
        vec!["add_neigh_proxy:10.44.1.10:eth0".to_owned()],
        "only the real member gets an entry"
    );
}

#[tokio::test]
async fn proxy_neigh_list_failure_degrades_to_adds_only() {
    // A failed kernel listing must not block the idempotent adds (existing
    // degrades to empty) and must not prune blind — and with no stored
    // snapshot, the next pass simply retries the listing.
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "IsoZone", "10.44.1.0/24", true).await;
    let member = insert_device(&h.devices, "10.44.1.10", ZONE_A).await;
    *h.existing_neigh_proxies.lock().await = vec!["10.44.1.99".to_owned()];
    h.fail_list_neigh
        .store(true, std::sync::atomic::Ordering::SeqCst);

    as_admin(h.svc.apply_device(member)).await.unwrap();

    let pc = policy_calls(&h).await;
    assert!(
        pc.contains(&"add_neigh_proxy:10.44.1.10:eth0".to_owned()),
        "adds still run when the listing fails: {pc:?}"
    );
    assert!(
        !pc.iter().any(|c| c.starts_with("remove_neigh_proxy:")),
        "no blind pruning without a listing: {pc:?}"
    );
}

#[tokio::test]
async fn proxy_neigh_mutation_failures_are_nonfatal() {
    // A failed add or remove is warn-logged and never aborts the reconcile —
    // the remaining enforcement (isolation rules, aliases) must still land.
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "IsoZone", "10.44.1.0/24", true).await;
    let member = insert_device(&h.devices, "10.44.1.10", ZONE_A).await;
    *h.existing_neigh_proxies.lock().await = vec!["10.44.1.99".to_owned()];
    h.fail_neigh_mutations
        .store(true, std::sync::atomic::Ordering::SeqCst);

    as_admin(h.svc.apply_device(member)).await.unwrap();

    let pc = policy_calls(&h).await;
    assert!(
        pc.contains(&"add_neigh_proxy:10.44.1.10:eth0".to_owned())
            && pc.contains(&"remove_neigh_proxy:10.44.1.99:eth0".to_owned()),
        "both mutations attempted despite failing: {pc:?}"
    );
    assert!(
        calls(&h).await.iter().any(|c| c.starts_with("isolation:")),
        "isolation rebuild still ran after pneigh failures"
    );
}

#[tokio::test]
async fn reconcile_survives_legacy_proxy_arp_clear_failure() {
    // The migration clear is best-effort: a sysctl failure must not abort the
    // startup reconcile that installs every device's rules.
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "IsoZone", "10.44.1.0/24", true).await;
    insert_device(&h.devices, "10.44.1.10", ZONE_A).await;
    h.fail_neigh_mutations
        .store(true, std::sync::atomic::Ordering::SeqCst);

    as_admin(h.svc.reconcile()).await.unwrap();

    assert!(
        policy_calls(&h)
            .await
            .contains(&"proxy_arp:eth0:false".to_owned()),
        "legacy clear attempted"
    );
}

#[tokio::test]
async fn stale_proxy_neigh_entries_pruned() {
    // A pneigh entry no longer backed by an isolate-members device (departed
    // device, or member isolation turned off) is removed on reconcile.
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "IsoZone", "10.44.1.0/24", true).await;
    let member = insert_device(&h.devices, "10.44.1.10", ZONE_A).await;
    *h.existing_neigh_proxies.lock().await = vec!["10.44.1.99".to_owned()];

    as_admin(h.svc.apply_device(member)).await.unwrap();

    let pc = policy_calls(&h).await;
    assert!(
        pc.contains(&"remove_neigh_proxy:10.44.1.99:eth0".to_owned()),
        "stale pneigh entry removed: {pc:?}"
    );
    assert!(
        !pc.contains(&"remove_neigh_proxy:10.44.1.10:eth0".to_owned()),
        "live member's pneigh entry preserved: {pc:?}"
    );
}

#[tokio::test]
async fn member_isolation_off_removes_proxy_neigh_entries() {
    // Zone exists but does not isolate members: no entry is added, and an
    // entry left over from when isolation was on is removed.
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "PlainZone", "10.44.1.0/24", false).await;
    let member = insert_device(&h.devices, "10.44.1.10", ZONE_A).await;
    *h.existing_neigh_proxies.lock().await = vec!["10.44.1.10".to_owned()];

    as_admin(h.svc.apply_device(member)).await.unwrap();

    let pc = policy_calls(&h).await;
    assert!(
        !pc.iter().any(|c| c.starts_with("add_neigh_proxy:")),
        "no pneigh entry for a non-isolating zone: {pc:?}"
    );
    assert!(
        pc.contains(&"remove_neigh_proxy:10.44.1.10:eth0".to_owned()),
        "leftover pneigh entry removed: {pc:?}"
    );
}

#[tokio::test]
async fn reconcile_clears_legacy_interface_proxy_arp() {
    // Migration for boxes that ran a pre-#1107 daemon: startup reconcile must
    // force the interface-wide sysctl off — even when member isolation is
    // active — because pneigh entries have replaced it.
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "IsoZone", "10.44.1.0/24", true).await;
    insert_device(&h.devices, "10.44.1.10", ZONE_A).await;

    as_admin(h.svc.reconcile()).await.unwrap();

    let pc = policy_calls(&h).await;
    assert!(
        pc.contains(&"proxy_arp:eth0:false".to_owned()),
        "legacy interface-wide proxy-arp cleared on startup: {pc:?}"
    );
    assert!(
        !pc.contains(&"proxy_arp:eth0:true".to_owned()),
        "interface-wide proxy-arp never re-enabled: {pc:?}"
    );
}

#[tokio::test]
async fn dhcp_disabled_degrades_to_empty_isolation() {
    let h = build().await;
    // DHCP left off (the default). A subnetted, member-isolating zone exists but
    // must not take effect.
    insert_subnet_zone(&h.zones, ZONE_A, "IsoZone", "10.44.1.0/24", true).await;
    // A pneigh entry left over from when Wardnet owned DHCP.
    *h.existing_neigh_proxies.lock().await = vec!["10.44.1.10".to_owned()];

    as_admin(h.svc.handle_exceptions_changed()).await.unwrap();

    let rules = isolation(&h).await;
    assert_eq!(rules, ZoneIsolationRules::default(), "empty isolation");
    let pc = policy_calls(&h).await;
    assert!(
        !pc.iter().any(|c| c.starts_with("add_neigh_proxy:")),
        "no pneigh entries while degraded: {pc:?}"
    );
    assert!(
        pc.contains(&"remove_neigh_proxy:10.44.1.10:eth0".to_owned()),
        "leftover pneigh entry removed on degrade: {pc:?}"
    );
}

#[tokio::test]
async fn gateway_alias_added_and_stale_alias_removed() {
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "ZoneA", "10.44.1.0/24", false).await;
    // Pre-seed the interface with the primary IP, a base-subnet address, a
    // stale former-gateway alias (a `.1` outside the base subnet, not desired),
    // and an operator-added secondary that is NOT a first-host (`.5`).
    *h.existing_aliases.lock().await = vec![
        ("192.168.1.1".to_owned(), 24), // primary — never removed
        ("192.168.1.5".to_owned(), 24), // base-subnet addr — never removed
        ("10.44.9.1".to_owned(), 24),   // stale former gateway (.1) — removed
        ("10.0.5.5".to_owned(), 24),    // operator secondary (.5) — preserved
    ];

    as_admin(h.svc.handle_exceptions_changed()).await.unwrap();

    let pc = policy_calls(&h).await;
    // The zone's `.1` gateway is aliased onto the LAN interface.
    assert!(
        pc.contains(&"add_alias:eth0:10.44.1.1/24".to_owned()),
        "zone gateway alias added: {pc:?}"
    );
    // The stale former-gateway alias (a first-host) is removed.
    assert!(
        pc.contains(&"remove_alias:eth0:10.44.9.1/24".to_owned()),
        "stale alias removed: {pc:?}"
    );
    // The primary and base-subnet addresses are never touched.
    assert!(
        !pc.iter()
            .any(|c| c.contains("remove_alias:eth0:192.168.1.")),
        "primary/base addresses left alone: {pc:?}"
    );
    // FIX 3: an operator secondary that is not a subnet's first-host is never
    // treated as a Wardnet-managed gateway, so it is preserved.
    assert!(
        !pc.contains(&"remove_alias:eth0:10.0.5.5/24".to_owned()),
        "operator secondary (non-first-host) preserved: {pc:?}"
    );
}

#[tokio::test]
async fn identical_reconcile_cycles_rebuild_isolation_once() {
    // FIX 6: a startup burst of identical device events must collapse into one
    // real isolation rebuild + (N-1) cheap no-ops. Two back-to-back
    // apply_device cycles over an unchanged database produce exactly one
    // `apply_zone_isolation` call.
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "ZoneA", "10.44.1.0/24", false).await;
    let dev = insert_device(&h.devices, "10.44.1.10", ZONE_A).await;

    as_admin(h.svc.apply_device(dev)).await.unwrap();
    as_admin(h.svc.apply_device(dev)).await.unwrap();

    let isolation_calls = calls(&h)
        .await
        .into_iter()
        .filter(|c| c.starts_with("isolation:"))
        .count();
    assert_eq!(
        isolation_calls, 1,
        "identical reconciles must rebuild isolation exactly once"
    );
}

#[tokio::test]
async fn handle_zone_change_releases_lease_and_flushes_conntrack() {
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "ZoneA", "10.44.1.0/24", false).await;
    let dev = insert_device(&h.devices, "10.44.1.10", ZONE_A).await;

    as_admin(h.svc.handle_zone_change(dev)).await.unwrap();

    let pc = policy_calls(&h).await;
    assert!(
        pc.contains(&"flush_conntrack:10.44.1.10".to_owned()),
        "conntrack flushed for the moved device: {pc:?}"
    );
    // A full isolation rebuild followed the move.
    assert!(
        calls(&h).await.iter().any(|c| c.starts_with("isolation:")),
        "isolation recomputed on zone change"
    );
}

// -- The daemon's own LAN IP is untouchable (issue #886) ----------------------

/// The Pi's own LAN address, as a device would (wrongly) claim it.
const OWN_IP: &str = "192.168.1.1";

/// Every kernel-touching call the enforcer made that names `ip`.
async fn calls_mentioning(h: &Harness, ip: &str) -> Vec<String> {
    calls(h)
        .await
        .into_iter()
        .chain(policy_calls(h).await)
        .filter(|c| c.contains(ip))
        .collect()
}

/// Regression test for #886.
///
/// A discovery bug let a device row claim the Pi's own LAN IP. Moving it into a
/// zone drove the enforcer to act on that address — culminating in a
/// `remove_host_route` that deleted the kernel's local route for the Pi's own
/// address and blackholed the box to every client on every path. The enforcer
/// must refuse to touch its own address, whatever the database says.
#[tokio::test]
async fn handle_zone_change_never_touches_the_daemons_own_ip() {
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "Network Devices", "10.100.1.0/24", true).await;
    let dev = insert_device(&h.devices, OWN_IP, ZONE_A).await;

    as_admin(h.svc.handle_zone_change(dev)).await.unwrap();

    let touched = calls_mentioning(&h, OWN_IP).await;
    assert!(
        touched.is_empty(),
        "the enforcer must never apply rules, host routes or conntrack flushes \
         to the daemon's own LAN IP (#886), but it did: {touched:?}"
    );
}

/// The startup path must not faithfully re-apply the bad state either: a
/// surviving device row holding our own IP is inert, not re-enforced.
#[tokio::test]
async fn reconcile_never_applies_rules_to_the_daemons_own_ip() {
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "Network Devices", "10.100.1.0/24", true).await;
    let _dev = insert_device(&h.devices, OWN_IP, ZONE_A).await;

    as_admin(h.svc.reconcile()).await.unwrap();

    let touched = calls_mentioning(&h, OWN_IP).await;
    assert!(
        touched.is_empty(),
        "startup reconcile must not re-apply enforcement to the daemon's own IP \
         (#886), but it did: {touched:?}"
    );
}

/// Teardown is as dangerous as setup: `remove_device` would delete the host
/// route for the given IP, which for our own address means the local route.
#[tokio::test]
async fn remove_device_never_touches_the_daemons_own_ip() {
    let h = build().await;
    let dev = insert_device(&h.devices, OWN_IP, GUEST).await;

    as_admin(h.svc.remove_device(dev, OWN_IP)).await.unwrap();

    let touched = calls_mentioning(&h, OWN_IP).await;
    assert!(
        touched.is_empty(),
        "remove_device must not tear down state for the daemon's own IP (#886), \
         but it did: {touched:?}"
    );
}

/// An IP change *into* or *out of* our own address must be inert on both sides:
/// the old IP is torn down, the new IP is set up, and neither may be ours.
#[tokio::test]
async fn handle_ip_change_never_touches_the_daemons_own_ip() {
    let h = build().await;
    enable_dhcp(&h).await;
    let dev = insert_device(&h.devices, OWN_IP, GUEST).await;

    // A device flapping onto our address...
    as_admin(h.svc.handle_ip_change(dev, "192.168.1.77", OWN_IP))
        .await
        .unwrap();
    // ...and back off it.
    as_admin(h.svc.handle_ip_change(dev, OWN_IP, "192.168.1.77"))
        .await
        .unwrap();

    let touched = calls_mentioning(&h, OWN_IP).await;
    assert!(
        touched.is_empty(),
        "handle_ip_change must not act on the daemon's own IP as either the old \
         or the new address (#886), but it did: {touched:?}"
    );
}

/// Code-review follow-up on #886: the per-zone gateway aliases are the Pi's own
/// addresses too. A device row claiming one must be refused exactly like a row
/// claiming the primary LAN IP — flushing conntrack for a zone gateway kills
/// every live flow through that zone.
#[tokio::test]
async fn handle_zone_change_never_touches_a_zone_gateway_alias() {
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "ZoneA", "10.44.1.0/24", false).await;
    // 10.44.1.1 is ZoneA's gateway alias — an address the Pi itself holds.
    let dev = insert_device(&h.devices, "10.44.1.1", ZONE_A).await;

    as_admin(h.svc.handle_zone_change(dev)).await.unwrap();

    let touched = calls_mentioning(&h, "10.44.1.1").await;
    // The isolation rebuild legitimately names the gateway as an alias to
    // install; only per-device enforcement calls are forbidden.
    let forbidden: Vec<&String> = touched
        .iter()
        .filter(|c| {
            c.starts_with("apply:")
                || c.starts_with("remove:")
                || c.starts_with("flush_conntrack:")
                || c.contains("host_route")
        })
        .collect();
    assert!(
        forbidden.is_empty(),
        "per-device enforcement must never touch a zone gateway alias (#886): {forbidden:?}"
    );
}

/// Code-review follow-up on #886: a repaired own-IP row has an empty `last_ip`
/// until re-observed. Reconcile must skip it cleanly — no rules keyed on an
/// empty string, no error.
#[tokio::test]
async fn reconcile_skips_devices_without_a_usable_ip() {
    let h = build().await;
    let _dev = insert_device(&h.devices, "", GUEST).await;

    as_admin(h.svc.reconcile()).await.unwrap();

    let bogus: Vec<String> = calls(&h)
        .await
        .into_iter()
        .filter(|c| c.starts_with("apply::"))
        .collect();
    assert!(
        bogus.is_empty(),
        "reconcile must not apply rules keyed on an empty IP (#886): {bogus:?}"
    );
}

/// Recorded `set_switchback_targets` pushes as `<device_id>=<ip>=[<cidr>,...]`.
async fn switchback(h: &Harness) -> Vec<String> {
    h.switchback.lock().await.clone()
}

#[tokio::test]
async fn zone_scoped_casting_exception_pushes_switchback_subnets() {
    // A Family(zone)↔Entertainment(zone) casting exception (bidirectional) must
    // push each side's device the OTHER zone's subnet as a switchback target, so
    // a tunnel-bound caster can reach the far zone's LAN across the tunnel.
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "Family", "192.168.200.0/24", false).await;
    insert_subnet_zone(&h.zones, ZONE_B, "Entertainment", "192.168.201.0/24", false).await;
    let family_dev = insert_device(&h.devices, "192.168.200.10", ZONE_A).await;
    let ent_dev = insert_device(&h.devices, "192.168.201.20", ZONE_B).await;
    let now = chrono::Utc::now();
    h.exceptions
        .insert(&ZoneException {
            id: Uuid::new_v4(),
            from: ExceptionEndpoint {
                kind: ExceptionEndpointKind::Zone,
                id: ZONE_A.parse().unwrap(),
            },
            to: ExceptionEndpoint {
                kind: ExceptionEndpointKind::Zone,
                id: ZONE_B.parse().unwrap(),
            },
            service: ServiceSpec::Preset {
                set: ServiceSet::Casting,
            },
            bidirectional: true,
            created_at: now,
            updated_at: now,
        })
        .await
        .unwrap();

    as_admin(h.svc.handle_exceptions_changed()).await.unwrap();

    let sw = switchback(&h).await;
    assert!(
        sw.contains(&format!(
            "{family_dev}=192.168.200.10=[\"192.168.201.0/24\"]"
        )),
        "family device must get the Entertainment subnet: {sw:?}"
    );
    assert!(
        sw.contains(&format!("{ent_dev}=192.168.201.20=[\"192.168.200.0/24\"]")),
        "entertainment device must get the Family subnet: {sw:?}"
    );
}

#[tokio::test]
async fn device_scoped_exception_pushes_switchback_slash32() {
    // A device→device casting exception resolves each endpoint to a `/32`.
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "Family", "192.168.200.0/24", false).await;
    insert_subnet_zone(&h.zones, ZONE_B, "Entertainment", "192.168.201.0/24", false).await;
    let phone = insert_device(&h.devices, "192.168.200.10", ZONE_A).await;
    let tv = insert_device(&h.devices, "192.168.201.20", ZONE_B).await;
    let now = chrono::Utc::now();
    h.exceptions
        .insert(&ZoneException {
            id: Uuid::new_v4(),
            from: ExceptionEndpoint {
                kind: ExceptionEndpointKind::Device,
                id: phone,
            },
            to: ExceptionEndpoint {
                kind: ExceptionEndpointKind::Device,
                id: tv,
            },
            service: ServiceSpec::Preset {
                set: ServiceSet::Casting,
            },
            bidirectional: true,
            created_at: now,
            updated_at: now,
        })
        .await
        .unwrap();

    as_admin(h.svc.handle_exceptions_changed()).await.unwrap();

    let sw = switchback(&h).await;
    assert!(
        sw.contains(&format!("{phone}=192.168.200.10=[\"192.168.201.20/32\"]")),
        "phone must get the TV /32: {sw:?}"
    );
    assert!(
        sw.contains(&format!("{tv}=192.168.201.20=[\"192.168.200.10/32\"]")),
        "TV must get the phone /32: {sw:?}"
    );
}

#[tokio::test]
async fn casting_exception_populates_deduped_nat_exempt_pairs() {
    // The cross-zone exception's (from, to) CIDR pair must appear ONCE in
    // nat_exempt_pairs even though the casting preset expands to many ports
    // (each of which produces an allow with the same CIDR pair).
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "Family", "192.168.200.0/24", false).await;
    insert_subnet_zone(&h.zones, ZONE_B, "Entertainment", "192.168.201.0/24", false).await;
    let phone = insert_device(&h.devices, "192.168.200.10", ZONE_A).await;
    let tv = insert_device(&h.devices, "192.168.201.20", ZONE_B).await;
    let now = chrono::Utc::now();
    h.exceptions
        .insert(&ZoneException {
            id: Uuid::new_v4(),
            from: ExceptionEndpoint {
                kind: ExceptionEndpointKind::Device,
                id: phone,
            },
            to: ExceptionEndpoint {
                kind: ExceptionEndpointKind::Device,
                id: tv,
            },
            service: ServiceSpec::Preset {
                set: ServiceSet::Casting,
            },
            bidirectional: true,
            created_at: now,
            updated_at: now,
        })
        .await
        .unwrap();

    as_admin(h.svc.handle_exceptions_changed()).await.unwrap();

    let rules = isolation(&h).await;
    assert_eq!(
        rules.nat_exempt_pairs,
        vec![(
            "192.168.200.10/32".to_owned(),
            "192.168.201.20/32".to_owned()
        )],
        "the (from, to) pair must appear exactly once despite many casting ports: {:?}",
        rules.nat_exempt_pairs
    );
}

#[tokio::test]
async fn smart_home_exception_yields_bidirectional_allows_zone_to_zone() {
    // Issue #1098, reproducing the reported deployment: a phone in Family
    // controlling a Govee bulb in Smart Home. The preset is applied
    // ZONE-to-zone (not device-to-device) because the admin cannot reliably
    // identify the individual bulb — see ADR 0025.
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "Family", "192.168.200.0/24", false).await;
    insert_subnet_zone(&h.zones, ZONE_B, "Smart Home", "192.168.202.0/24", false).await;
    let now = chrono::Utc::now();
    h.exceptions
        .insert(&ZoneException {
            id: Uuid::new_v4(),
            from: ExceptionEndpoint {
                kind: ExceptionEndpointKind::Zone,
                id: ZONE_A.parse().unwrap(),
            },
            to: ExceptionEndpoint {
                kind: ExceptionEndpointKind::Zone,
                id: ZONE_B.parse().unwrap(),
            },
            service: ServiceSpec::Preset {
                set: ServiceSet::SmartHome,
            },
            bidirectional: true,
            created_at: now,
            updated_at: now,
        })
        .await
        .unwrap();

    as_admin(h.svc.handle_exceptions_changed()).await.unwrap();

    let rules = isolation(&h).await;
    let from = "192.168.200.0/24";
    let to = "192.168.202.0/24";
    let has = |proto: &str, port: u16| {
        rules.allows.iter().any(|a| {
            a.from_cidr == from && a.to_cidr == to && a.proto == proto && a.port_start == port
        })
    };

    // The three Govee LAN API ports — the exact flows the casting preset
    // missed, which is what made the lights unreachable in the first place.
    assert!(has("udp", 4001), "Govee discovery: {:?}", rules.allows);
    assert!(has("udp", 4002), "Govee response: {:?}", rules.allows);
    assert!(has("udp", 4003), "Govee control: {:?}", rules.allows);
    // A representative port from each of the other vendor families.
    assert!(has("tcp", 6668), "Tuya local control: {:?}", rules.allows);
    assert!(has("udp", 56700), "LIFX: {:?}", rules.allows);
    assert!(has("tcp", 9123), "ESPHome native API: {:?}", rules.allows);

    // Local HTTP/HTTPS is deliberately NOT opened by this preset: zone-scoped,
    // it would reach every HTTP listener in the peer zone.
    assert!(!has("tcp", 80), "no bare HTTP hole: {:?}", rules.allows);
    assert!(!has("tcp", 443), "no bare HTTPS hole: {:?}", rules.allows);

    // The device->client leg (Govee UDP 4002) is a fresh flow, not a conntrack
    // reply, so every allow must carry the flag the firewall renders the
    // reverse direction from.
    assert!(rules.allows.iter().all(|a| a.bidirectional));
    // Pinned literally rather than against `ServiceSet::SmartHome.ports().len()`
    // — a self-referential count cannot catch a port being added or dropped.
    assert_eq!(rules.allows.len(), 10, "{:?}", rules.allows);
}

#[tokio::test]
async fn smart_home_exception_populates_deduped_nat_exempt_pairs() {
    // Same dedup contract as casting: ten ports collapse to one CIDR pair.
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "Family", "192.168.200.0/24", false).await;
    insert_subnet_zone(&h.zones, ZONE_B, "Smart Home", "192.168.202.0/24", false).await;
    let now = chrono::Utc::now();
    h.exceptions
        .insert(&ZoneException {
            id: Uuid::new_v4(),
            from: ExceptionEndpoint {
                kind: ExceptionEndpointKind::Zone,
                id: ZONE_A.parse().unwrap(),
            },
            to: ExceptionEndpoint {
                kind: ExceptionEndpointKind::Zone,
                id: ZONE_B.parse().unwrap(),
            },
            service: ServiceSpec::Preset {
                set: ServiceSet::SmartHome,
            },
            bidirectional: true,
            created_at: now,
            updated_at: now,
        })
        .await
        .unwrap();

    as_admin(h.svc.handle_exceptions_changed()).await.unwrap();

    let rules = isolation(&h).await;
    assert_eq!(
        rules.nat_exempt_pairs,
        vec![("192.168.200.0/24".to_owned(), "192.168.202.0/24".to_owned())],
        "the (from, to) pair must appear exactly once despite ten ports: {:?}",
        rules.nat_exempt_pairs
    );
}

/// Insert a bidirectional casting exception between `ZONE_A` and `ZONE_B`.
async fn insert_zone_casting_exception(h: &Harness) {
    let now = chrono::Utc::now();
    h.exceptions
        .insert(&ZoneException {
            id: Uuid::new_v4(),
            from: ExceptionEndpoint {
                kind: ExceptionEndpointKind::Zone,
                id: ZONE_A.parse().unwrap(),
            },
            to: ExceptionEndpoint {
                kind: ExceptionEndpointKind::Zone,
                id: ZONE_B.parse().unwrap(),
            },
            service: ServiceSpec::Preset {
                set: ServiceSet::Casting,
            },
            bidirectional: true,
            created_at: now,
            updated_at: now,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn identical_reconcile_pushes_switchback_once() {
    // A startup burst of identical reconciles must push switchback exactly once
    // per device — the per-device push loop is debounced against last_switchback,
    // not re-run on every reconcile (avoiding O(N^2) over the burst).
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "Family", "192.168.200.0/24", false).await;
    insert_subnet_zone(&h.zones, ZONE_B, "Entertainment", "192.168.201.0/24", false).await;
    let family = insert_device(&h.devices, "192.168.200.10", ZONE_A).await;
    let _tv = insert_device(&h.devices, "192.168.201.20", ZONE_B).await;
    insert_zone_casting_exception(&h).await;

    as_admin(h.svc.apply_device(family)).await.unwrap();
    let after_first = h.switchback.lock().await.len();
    as_admin(h.svc.apply_device(family)).await.unwrap();
    as_admin(h.svc.apply_device(family)).await.unwrap();
    let after_repeats = h.switchback.lock().await.len();

    assert_eq!(
        after_first, 2,
        "first reconcile pushes once per device (2 devices)"
    );
    assert_eq!(
        after_first, after_repeats,
        "identical reconciles must not re-push switchback: {after_first} vs {after_repeats}"
    );
}

#[tokio::test]
async fn device_zone_change_repushes_switchback() {
    // A device changing zone alters switchback membership without changing the
    // isolation `rules`, so it must still trigger a fresh push.
    let h = build().await;
    enable_dhcp(&h).await;
    insert_subnet_zone(&h.zones, ZONE_A, "Family", "192.168.200.0/24", false).await;
    insert_subnet_zone(&h.zones, ZONE_B, "Entertainment", "192.168.201.0/24", false).await;
    let family = insert_device(&h.devices, "192.168.200.10", ZONE_A).await;
    let _tv = insert_device(&h.devices, "192.168.201.20", ZONE_B).await;
    insert_zone_casting_exception(&h).await;

    as_admin(h.svc.apply_device(family)).await.unwrap();
    let baseline = h.switchback.lock().await.len();

    // Move the family device into ZONE_B: its target CIDR flips (Family subnet
    // instead of Entertainment subnet), so the snapshot changes.
    h.devices
        .assign_zone(&family.to_string(), ZONE_B)
        .await
        .unwrap();
    as_admin(h.svc.handle_zone_change(family)).await.unwrap();

    let after = h.switchback.lock().await.len();
    assert!(
        after > baseline,
        "a device-zone change must re-push switchback: {after} vs {baseline}"
    );
}
