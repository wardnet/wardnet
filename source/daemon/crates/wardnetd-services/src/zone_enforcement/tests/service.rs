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
use std::net::IpAddr;
use std::sync::Arc;

use arc_swap::ArcSwap;
use async_trait::async_trait;
use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use tokio::sync::Mutex;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;
use wardnet_common::dns::UpstreamId;
use wardnet_common::network_zone::{AllowedTargetKind, NetworkZone, ZoneProvenance, ZoneStance};
use wardnet_common::routing::RoutingTarget;
use wardnetd_data::repository::device::DeviceRow;
use wardnetd_data::repository::{
    DeviceRepository, NetworkZoneRepository, SqliteDeviceRepository, SqliteNetworkZoneRepository,
    SqliteSystemConfigRepository, SystemConfigRepository,
};

use crate::auth_context;
use crate::error::AppError;
use crate::routing::RoutingService;
use crate::routing::firewall::{FirewallManager, ZoneRules};
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
    async fn check_tools_available(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn destroy_wardnet_table(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

// -- No-op PolicyRouter (only flush_conntrack is exercised) ------------------

#[derive(Default)]
struct NoopPolicy;

#[async_trait]
impl PolicyRouter for NoopPolicy {
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
    async fn flush_conntrack(&self, _src_ip: &str) -> anyhow::Result<()> {
        Ok(())
    }
    async fn flush_route_cache(&self) -> anyhow::Result<()> {
        Ok(())
    }
    async fn check_tools_available(&self) -> anyhow::Result<()> {
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
    fn dns_upstream_snapshot(&self) -> Arc<ArcSwap<HashMap<IpAddr, UpstreamId>>> {
        unimplemented!()
    }
    async fn rebuild_dns_upstream_snapshot(&self) -> Result<(), AppError> {
        unimplemented!()
    }
}

// -- Harness -----------------------------------------------------------------

struct Harness {
    svc: ZoneEnforcementServiceImpl,
    zones: Arc<dyn NetworkZoneRepository>,
    devices: Arc<dyn DeviceRepository>,
    system_config: Arc<dyn SystemConfigRepository>,
    fw_calls: Arc<Mutex<Vec<String>>>,
    fw_zone_ips: Arc<Mutex<Vec<String>>>,
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
        Arc::new(SqliteSystemConfigRepository::new(pool));

    let fw_calls = Arc::new(Mutex::new(Vec::new()));
    let fw_zone_ips = Arc::new(Mutex::new(Vec::new()));
    let firewall: Arc<dyn FirewallManager> = Arc::new(RecordingFirewall {
        calls: fw_calls.clone(),
        zone_rule_ips: fw_zone_ips.clone(),
    });
    let policy_router: Arc<dyn PolicyRouter> = Arc::new(NoopPolicy);
    let clamps = Arc::new(Mutex::new(Vec::new()));
    let routing: Arc<dyn RoutingService> = Arc::new(RecordingRouting {
        clamps: clamps.clone(),
    });

    let svc = ZoneEnforcementServiceImpl::new(
        zones.clone(),
        devices.clone(),
        system_config.clone(),
        firewall,
        policy_router,
        routing,
        LAN_IFACE.to_owned(),
    );

    Harness {
        svc,
        zones,
        devices,
        system_config,
        fw_calls,
        fw_zone_ips,
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

    assert_eq!(calls(&h).await, vec!["remove:192.168.1.80".to_owned()]);
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
