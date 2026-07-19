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
    async fn add_ip_rule(&self, _src_ip: &str, _table: u32) -> anyhow::Result<()> {
        Ok(())
    }
    async fn remove_ip_rule(&self, _src_ip: &str, _table: u32) -> anyhow::Result<()> {
        Ok(())
    }
    async fn list_wardnet_rules(&self) -> anyhow::Result<Vec<(String, u32)>> {
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

// -- Recording RoutingService (only apply_rule_for_device is exercised) ------

#[derive(Default)]
struct RecordingRouting {
    /// `<device_id>=<target-debug>` for each clamp callback.
    clamps: Arc<Mutex<Vec<String>>>,
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
    clamps: Arc<Mutex<Vec<String>>>,
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
    let policy_router: Arc<dyn PolicyRouter> = Arc::new(RecordingPolicy {
        calls: policy_calls.clone(),
        existing_aliases: existing_aliases.clone(),
    });
    let clamps = Arc::new(Mutex::new(Vec::new()));
    let routing: Arc<dyn RoutingService> = Arc::new(RecordingRouting {
        clamps: clamps.clone(),
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
        clamps,
    }
}

async fn as_admin<F: Future>(fut: F) -> F::Output {
    auth_context::with_context(
        AuthContext::Admin {
            admin_id: Uuid::nil(),
        },
        fut,
    )
    .await
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

#[tokio::test]
async fn member_isolation_zone_sets_proxy_arp_and_host_route() {
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
        pc.contains(&"proxy_arp:eth0:true".to_owned()),
        "proxy-arp enabled: {pc:?}"
    );
    assert!(
        pc.contains(&"add_host_route:10.44.1.10:eth0".to_owned()),
        "member host route added: {pc:?}"
    );
}

#[tokio::test]
async fn dhcp_disabled_degrades_to_empty_isolation() {
    let h = build().await;
    // DHCP left off (the default). A subnetted, member-isolating zone exists but
    // must not take effect.
    insert_subnet_zone(&h.zones, ZONE_A, "IsoZone", "10.44.1.0/24", true).await;

    as_admin(h.svc.handle_exceptions_changed()).await.unwrap();

    let rules = isolation(&h).await;
    assert_eq!(rules, ZoneIsolationRules::default(), "empty isolation");
    assert!(
        policy_calls(&h)
            .await
            .contains(&"proxy_arp:eth0:false".to_owned()),
        "proxy-arp disabled on degrade"
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
