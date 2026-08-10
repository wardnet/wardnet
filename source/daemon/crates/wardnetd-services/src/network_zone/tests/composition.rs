//! Composition test: the zone gate in `DeviceService::set_rule` /
//! `set_rule_for_ip` (issue #735, locked-decision #1). A device in a
//! tunnel-only zone may not be routed direct — including via a `Default`
//! target that resolves to direct — but a tunnel target is accepted.

use std::future::Future;
use std::sync::Arc;

use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;
use wardnet_common::network_zone::{AllowedTargetKind, NetworkZone, ZoneProvenance, ZoneStance};
use wardnet_common::routing::RoutingTarget;
use wardnetd_data::repository::device::DeviceRow;
use wardnetd_data::repository::{
    DeviceRepository, NetworkZoneRepository, SqliteDeviceRepository, SqliteDnsEventsRepository,
    SqliteNetworkZoneRepository, SqliteSystemConfigRepository, SystemConfigRepository,
};

use crate::auth_context;
use crate::device::{DeviceService, DeviceServiceImpl};
use crate::error::AppError;
use crate::event::BroadcastEventBus;

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

async fn as_admin<F: Future>(fut: F) -> F::Output {
    auth_context::with_context(AuthContext::system(), fut).await
}

const DEVICE_IP: &str = "192.168.1.77";

/// Build a `DeviceService` over a real DB with one zone (given
/// `allowed_targets`) and a device assigned to it. When `default_policy` is
/// `Some`, it is persisted so `RoutingTarget::Default` resolution is exercised.
/// Returns the service and the device id.
async fn build_zone_device(
    allowed_targets: Vec<AllowedTargetKind>,
    default_policy: Option<&str>,
) -> (DeviceServiceImpl, Uuid) {
    let pool = test_pool().await;
    let zone_repo = Arc::new(SqliteNetworkZoneRepository::new(pool.clone()));
    let device_repo: Arc<dyn DeviceRepository> =
        Arc::new(SqliteDeviceRepository::new(pool.clone()));
    let system_config = Arc::new(SqliteSystemConfigRepository::new(pool.clone()));

    if let Some(policy) = default_policy {
        system_config.set_default_policy(policy).await.unwrap();
    }

    let now = chrono::Utc::now();
    let zone_id = Uuid::new_v4();
    zone_repo
        .insert(&NetworkZone {
            id: zone_id,
            name: "Gated".to_owned(),
            provenance: ZoneProvenance::Manual,
            isolation_stance: ZoneStance::SharedSubnet,
            allowed_targets,
            member_isolation: false,
            subnet: None,
            admin_ui_reachable: true,
            is_default: false,
            is_default_for_new: false,
            created_at: now,
            updated_at: now,
        })
        .await
        .unwrap();

    let device_id = Uuid::new_v4();
    let ts = now.to_rfc3339();
    device_repo
        .insert(&DeviceRow {
            id: device_id.to_string(),
            mac: "aa:bb:cc:dd:ee:01".to_owned(),
            hostname: None,
            manufacturer: None,
            manufacturer_source: None,
            is_randomized: false,
            device_type: "unknown".to_owned(),
            first_seen: ts.clone(),
            last_seen: ts,
            last_ip: DEVICE_IP.to_owned(),
            zone_id: zone_id.to_string(),
            connection_mode: wardnet_common::device::DeviceConnectionMode::Lan,
        })
        .await
        .unwrap();

    let svc = DeviceServiceImpl::new(
        device_repo,
        Arc::new(SqliteDnsEventsRepository::new(pool.clone())),
        zone_repo,
        system_config,
        Arc::new(BroadcastEventBus::new(16)),
    );
    (svc, device_id)
}

/// Tunnel-only zone, default `direct` policy (nothing persisted).
async fn build_with_tunnel_only_device() -> (DeviceServiceImpl, Uuid) {
    build_zone_device(vec![AllowedTargetKind::Tunnel], None).await
}

#[tokio::test]
async fn tunnel_only_zone_rejects_direct_target() {
    let (svc, device_id) = build_with_tunnel_only_device().await;
    let err = as_admin(svc.set_rule(&device_id.to_string(), RoutingTarget::Direct))
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)), "got {err:?}");
}

#[tokio::test]
async fn tunnel_only_zone_rejects_default_when_policy_is_direct() {
    // Fresh DB: no `default_policy` config → resolves to Direct → rejected.
    let (svc, device_id) = build_with_tunnel_only_device().await;
    let err = as_admin(svc.set_rule(&device_id.to_string(), RoutingTarget::Default))
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)), "got {err:?}");
}

#[tokio::test]
async fn tunnel_only_zone_accepts_tunnel_target() {
    let (svc, device_id) = build_with_tunnel_only_device().await;
    let target = RoutingTarget::Tunnel {
        tunnel_id: Uuid::new_v4(),
    };
    as_admin(svc.set_rule(&device_id.to_string(), target))
        .await
        .expect("tunnel target permitted by tunnel-only zone");
}

#[tokio::test]
async fn direct_only_zone_rejects_default_when_policy_is_a_tunnel() {
    // Direct-only zone; global policy points at a tunnel UUID, so `Default`
    // resolves to Tunnel — not permitted. Exercises the tunnel branch of
    // resolve_default_kind.
    let tunnel_uuid = Uuid::new_v4().to_string();
    let (svc, device_id) =
        build_zone_device(vec![AllowedTargetKind::Direct], Some(&tunnel_uuid)).await;
    let err = as_admin(svc.set_rule(&device_id.to_string(), RoutingTarget::Default))
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)), "got {err:?}");
}

#[tokio::test]
async fn set_rule_for_ip_is_also_zone_gated() {
    // The self-service path (by source IP) applies the same zone gate.
    let (svc, _device_id) = build_with_tunnel_only_device().await;
    let err = as_admin(svc.set_rule_for_ip(DEVICE_IP, RoutingTarget::Direct))
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Conflict(_)), "got {err:?}");
}
