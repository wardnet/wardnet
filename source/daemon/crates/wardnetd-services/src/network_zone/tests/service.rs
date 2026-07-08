//! Service-layer tests for [`NetworkZoneServiceImpl`].
//!
//! Builds real `SqliteNetworkZoneRepository` + `SqliteDeviceRepository` over an
//! in-memory pool (same migration set as the rest of the workspace, so the
//! three system zones are seeded) and exercises CRUD, the guard rails, the
//! exclusive default-flip, the admin auth gate, and event emission.

use std::future::Future;
use std::sync::Arc;

use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use uuid::Uuid;
use wardnet_common::api::{CreateNetworkZoneRequest, UpdateNetworkZoneRequest};
use wardnet_common::auth::AuthContext;
use wardnet_common::event::WardnetEvent;
use wardnet_common::network_zone::{AllowedTargetKind, ZoneProvenance, ZoneStance, ZoneSubnet};
use wardnet_common::routing::RoutingTarget;
use wardnetd_data::repository::device::DeviceRow;
use wardnetd_data::repository::{
    DeviceRepository, SqliteDeviceRepository, SqliteNetworkZoneRepository,
    SqliteSystemConfigRepository,
};

use crate::auth_context;
use crate::error::AppError;
use crate::event::{BroadcastEventBus, EventPublisher};
use crate::network_zone::service::{NetworkZoneService, NetworkZoneServiceImpl};

use crate::tests::mac_from;

const TRUSTED: &str = "00000000-0000-0000-0000-000000000201";
const IOT: &str = "00000000-0000-0000-0000-000000000202";
const GUEST: &str = "00000000-0000-0000-0000-000000000203";

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

struct Harness {
    svc: NetworkZoneServiceImpl,
    devices: Arc<dyn DeviceRepository>,
    events: Arc<BroadcastEventBus>,
}

async fn build() -> Harness {
    let pool = test_pool().await;
    let zones = Arc::new(SqliteNetworkZoneRepository::new(pool.clone()));
    let devices: Arc<dyn DeviceRepository> = Arc::new(SqliteDeviceRepository::new(pool.clone()));
    let system_config = Arc::new(SqliteSystemConfigRepository::new(pool));
    let events = Arc::new(BroadcastEventBus::new(16));
    let svc = NetworkZoneServiceImpl::new(zones, devices.clone(), system_config, events.clone());
    Harness {
        svc,
        devices,
        events,
    }
}

/// Run a future inside an `Admin` task-local context.
async fn as_admin<F: Future>(fut: F) -> F::Output {
    auth_context::with_context(
        AuthContext::Admin {
            admin_id: Uuid::nil(),
        },
        fut,
    )
    .await
}

fn create_req(name: &str) -> CreateNetworkZoneRequest {
    CreateNetworkZoneRequest {
        name: name.to_owned(),
        isolation_stance: ZoneStance::SharedSubnet,
        allowed_targets: vec![AllowedTargetKind::Direct, AllowedTargetKind::Tunnel],
        member_isolation: false,
        admin_ui_reachable: true,
        subnet: None,
    }
}

async fn insert_device(devices: &Arc<dyn DeviceRepository>, id: Uuid, zone_id: &str) {
    let now = chrono::Utc::now().to_rfc3339();
    devices
        .insert(&DeviceRow {
            id: id.to_string(),
            mac: mac_from(id),
            hostname: None,
            manufacturer: None,
            device_type: "unknown".to_owned(),
            first_seen: now.clone(),
            last_seen: now,
            last_ip: "192.168.1.50".to_owned(),
            zone_id: zone_id.to_owned(),
            connection_mode: wardnet_common::device::DeviceConnectionMode::Lan,
        })
        .await
        .unwrap();
}

// ── Seed / list ────────────────────────────────────────────────────────────

#[tokio::test]
async fn lists_three_seeded_system_zones() {
    let h = build().await;
    let zones = as_admin(h.svc.list_zones()).await.unwrap();
    assert_eq!(zones.len(), 3);
    let trusted = zones.iter().find(|z| z.zone.name == "Trusted").unwrap();
    assert!(trusted.zone.is_default);
    assert_eq!(trusted.zone.provenance, ZoneProvenance::System);
    let guest = zones.iter().find(|z| z.zone.name == "Guest").unwrap();
    assert!(guest.zone.is_default_for_new);
    assert!(zones.iter().any(|z| z.zone.name == "IoT"));
}

#[tokio::test]
async fn every_method_rejects_non_admin() {
    let h = build().await;
    // No auth context scope → Forbidden.
    assert!(matches!(
        h.svc.list_zones().await,
        Err(AppError::Forbidden(_))
    ));
    assert!(matches!(
        h.svc.create_zone(create_req("X")).await,
        Err(AppError::Forbidden(_))
    ));
    assert!(matches!(
        h.svc.delete_zone(IOT.parse().unwrap()).await,
        Err(AppError::Forbidden(_))
    ));
    assert!(matches!(
        h.svc.set_default(IOT.parse().unwrap()).await,
        Err(AppError::Forbidden(_))
    ));
    assert!(matches!(
        h.svc.get_quarantine_new_devices().await,
        Err(AppError::Forbidden(_))
    ));
    assert!(matches!(
        h.svc.set_quarantine_new_devices(true).await,
        Err(AppError::Forbidden(_))
    ));
}

#[tokio::test]
async fn quarantine_toggle_defaults_off_and_round_trips() {
    let h = build().await;
    // Off by default (key absent).
    assert!(!as_admin(h.svc.get_quarantine_new_devices()).await.unwrap());
    // Enable, then read back.
    as_admin(h.svc.set_quarantine_new_devices(true))
        .await
        .unwrap();
    assert!(as_admin(h.svc.get_quarantine_new_devices()).await.unwrap());
    // Disable again.
    as_admin(h.svc.set_quarantine_new_devices(false))
        .await
        .unwrap();
    assert!(!as_admin(h.svc.get_quarantine_new_devices()).await.unwrap());
}

// ── Create ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_zone_forces_manual_and_zero_members() {
    let h = build().await;
    let zone = as_admin(h.svc.create_zone(create_req("Cameras")))
        .await
        .unwrap();
    assert_eq!(zone.provenance, ZoneProvenance::Manual);
    assert!(!zone.is_default);
    assert!(!zone.is_default_for_new);
    let view = as_admin(h.svc.get_zone(zone.id)).await.unwrap();
    assert_eq!(view.member_count, 0);
}

#[tokio::test]
async fn create_zone_rejects_empty_targets() {
    let h = build().await;
    let mut req = create_req("NoTargets");
    req.allowed_targets = vec![];
    assert!(matches!(
        as_admin(h.svc.create_zone(req)).await,
        Err(AppError::BadRequest(_))
    ));
}

#[tokio::test]
async fn create_zone_rejects_bad_cidr() {
    let h = build().await;
    let mut req = create_req("BadSubnet");
    req.subnet = Some(ZoneSubnet {
        cidr: "not-a-cidr".to_owned(),
    });
    assert!(matches!(
        as_admin(h.svc.create_zone(req)).await,
        Err(AppError::BadRequest(_))
    ));
}

#[tokio::test]
async fn create_zone_rejects_public_cidr() {
    let h = build().await;
    let mut req = create_req("PublicSubnet");
    // A syntactically valid but non-RFC-1918 range must be rejected.
    req.subnet = Some(ZoneSubnet {
        cidr: "8.8.0.0/24".to_owned(),
    });
    assert!(matches!(
        as_admin(h.svc.create_zone(req)).await,
        Err(AppError::BadRequest(_))
    ));
}

#[tokio::test]
async fn create_zone_rejects_boundary_straddling_supernet() {
    let h = build().await;
    let mut req = create_req("Straddle");
    // Private base (192.168.0.0) but /15 extends into public 192.169.x — the
    // whole range must be inside one RFC 1918 block, so this is rejected.
    req.subnet = Some(ZoneSubnet {
        cidr: "192.168.0.0/15".to_owned(),
    });
    assert!(matches!(
        as_admin(h.svc.create_zone(req)).await,
        Err(AppError::BadRequest(_))
    ));
}

#[tokio::test]
async fn create_zone_accepts_the_canonical_private_12() {
    let h = build().await;
    let mut req = create_req("Private12");
    // 172.16.0.0/12 is fully within the 172.16/12 block — must be accepted.
    req.subnet = Some(ZoneSubnet {
        cidr: "172.16.0.0/12".to_owned(),
    });
    assert!(as_admin(h.svc.create_zone(req)).await.is_ok());
}

#[tokio::test]
async fn create_zone_rejects_subnet_too_small_for_pool() {
    // FIX 4: a syntactically-valid CIDR whose pool bounds are empty (a /30 has
    // no usable .10..broadcast-6 range) is rejected up front so the DHCP
    // resolver never has to silently fall back to the base scope.
    let h = build().await;
    let mut req = create_req("TinySubnet");
    req.subnet = Some(ZoneSubnet {
        cidr: "10.44.0.0/30".to_owned(),
    });
    let err = as_admin(h.svc.create_zone(req)).await;
    match err {
        Err(AppError::BadRequest(msg)) => {
            assert!(
                msg.contains("too small to host a DHCP pool"),
                "unexpected message: {msg}"
            );
        }
        other => panic!("expected BadRequest, got {other:?}"),
    }
}

#[tokio::test]
async fn create_zone_rejects_duplicate_name() {
    let h = build().await;
    // "Trusted" is a seeded name.
    assert!(matches!(
        as_admin(h.svc.create_zone(create_req("Trusted"))).await,
        Err(AppError::Conflict(_))
    ));
}

// ── Get / update ─────────────────────────────────────────────────────────

#[tokio::test]
async fn get_zone_unknown_is_404() {
    let h = build().await;
    assert!(matches!(
        as_admin(h.svc.get_zone(Uuid::new_v4())).await,
        Err(AppError::NotFound(_))
    ));
}

#[tokio::test]
async fn update_zone_applies_partial_fields() {
    let h = build().await;
    let zone = as_admin(h.svc.create_zone(create_req("Office")))
        .await
        .unwrap();
    let req = UpdateNetworkZoneRequest {
        allowed_targets: Some(vec![AllowedTargetKind::Tunnel]),
        admin_ui_reachable: Some(false),
        ..Default::default()
    };
    let updated = as_admin(h.svc.update_zone(zone.id, req)).await.unwrap();
    assert_eq!(updated.allowed_targets, vec![AllowedTargetKind::Tunnel]);
    assert!(!updated.admin_ui_reachable);
    assert_eq!(updated.name, "Office");
}

#[tokio::test]
async fn update_zone_promotes_default_flags() {
    let h = build().await;
    let zone = as_admin(h.svc.create_zone(create_req("Promote")))
        .await
        .unwrap();
    let req = UpdateNetworkZoneRequest {
        is_default: Some(true),
        is_default_for_new: Some(true),
        ..Default::default()
    };
    let updated = as_admin(h.svc.update_zone(zone.id, req)).await.unwrap();
    assert!(updated.is_default);
    assert!(updated.is_default_for_new);
    // Exclusivity: no other zone still holds either flag.
    let zones = as_admin(h.svc.list_zones()).await.unwrap();
    assert_eq!(zones.iter().filter(|z| z.zone.is_default).count(), 1);
    assert_eq!(
        zones.iter().filter(|z| z.zone.is_default_for_new).count(),
        1
    );
}

#[tokio::test]
async fn update_zone_sets_then_clears_subnet() {
    let h = build().await;
    let zone = as_admin(h.svc.create_zone(create_req("Subnetted")))
        .await
        .unwrap();
    let set = UpdateNetworkZoneRequest {
        subnet: Some(Some(ZoneSubnet {
            cidr: "10.44.0.0/24".to_owned(),
        })),
        ..Default::default()
    };
    let with_subnet = as_admin(h.svc.update_zone(zone.id, set)).await.unwrap();
    assert_eq!(with_subnet.subnet.as_ref().unwrap().cidr, "10.44.0.0/24");

    let clear = UpdateNetworkZoneRequest {
        subnet: Some(None),
        ..Default::default()
    };
    let cleared = as_admin(h.svc.update_zone(zone.id, clear)).await.unwrap();
    assert!(cleared.subnet.is_none());
}

#[tokio::test]
async fn update_zone_rejects_bad_subnet_cidr() {
    let h = build().await;
    let zone = as_admin(h.svc.create_zone(create_req("BadUpdate")))
        .await
        .unwrap();
    let req = UpdateNetworkZoneRequest {
        subnet: Some(Some(ZoneSubnet {
            cidr: "999.999/8".to_owned(),
        })),
        ..Default::default()
    };
    assert!(matches!(
        as_admin(h.svc.update_zone(zone.id, req)).await,
        Err(AppError::BadRequest(_))
    ));
}

#[tokio::test]
async fn assign_device_same_zone_is_noop() {
    let h = build().await;
    let device_id = Uuid::new_v4();
    insert_device(&h.devices, device_id, GUEST).await;
    // Assigning to the zone it is already in returns Ok without emitting.
    let mut rx = h.events.subscribe();
    as_admin(h.svc.assign_device(device_id, GUEST.parse().unwrap()))
        .await
        .unwrap();
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn update_zone_rejects_clearing_default_flag() {
    let h = build().await;
    let req = UpdateNetworkZoneRequest {
        is_default: Some(false),
        ..Default::default()
    };
    assert!(matches!(
        as_admin(h.svc.update_zone(TRUSTED.parse().unwrap(), req)).await,
        Err(AppError::BadRequest(_))
    ));
}

// ── Default flip ───────────────────────────────────────────────────────────

#[tokio::test]
async fn set_default_is_exclusive() {
    let h = build().await;
    as_admin(h.svc.set_default(IOT.parse().unwrap()))
        .await
        .unwrap();
    let zones = as_admin(h.svc.list_zones()).await.unwrap();
    let defaults: Vec<_> = zones.iter().filter(|z| z.zone.is_default).collect();
    assert_eq!(defaults.len(), 1);
    assert_eq!(defaults[0].zone.name, "IoT");
}

#[tokio::test]
async fn set_default_for_new_is_exclusive() {
    let h = build().await;
    as_admin(h.svc.set_default_for_new(IOT.parse().unwrap()))
        .await
        .unwrap();
    let zones = as_admin(h.svc.list_zones()).await.unwrap();
    let defaults: Vec<_> = zones.iter().filter(|z| z.zone.is_default_for_new).collect();
    assert_eq!(defaults.len(), 1);
    assert_eq!(defaults[0].zone.name, "IoT");
}

// ── Delete guards ──────────────────────────────────────────────────────────

#[tokio::test]
async fn delete_rejects_system_zone() {
    let h = build().await;
    assert!(matches!(
        as_admin(h.svc.delete_zone(IOT.parse().unwrap())).await,
        Err(AppError::Conflict(_))
    ));
}

#[tokio::test]
async fn delete_rejects_default_zone() {
    let h = build().await;
    // Trusted is both system and default; assert it is guarded.
    assert!(matches!(
        as_admin(h.svc.delete_zone(TRUSTED.parse().unwrap())).await,
        Err(AppError::Conflict(_))
    ));
}

#[tokio::test]
async fn delete_rejects_default_for_new_zone() {
    // A manual, empty zone that has been promoted to default-for-new must not
    // be deletable — doing so would orphan the pointer and break discovery.
    let h = build().await;
    let zone = as_admin(h.svc.create_zone(create_req("Landing")))
        .await
        .unwrap();
    as_admin(h.svc.set_default_for_new(zone.id)).await.unwrap();
    assert!(matches!(
        as_admin(h.svc.delete_zone(zone.id)).await,
        Err(AppError::Conflict(_))
    ));
}

#[tokio::test]
async fn delete_rejects_zone_with_members() {
    let h = build().await;
    let zone = as_admin(h.svc.create_zone(create_req("Temp")))
        .await
        .unwrap();
    let device_id = Uuid::new_v4();
    insert_device(&h.devices, device_id, &zone.id.to_string()).await;
    assert!(matches!(
        as_admin(h.svc.delete_zone(zone.id)).await,
        Err(AppError::Conflict(_))
    ));
}

#[tokio::test]
async fn delete_empty_manual_zone_succeeds() {
    let h = build().await;
    let zone = as_admin(h.svc.create_zone(create_req("Ephemeral")))
        .await
        .unwrap();
    as_admin(h.svc.delete_zone(zone.id)).await.unwrap();
    assert!(matches!(
        as_admin(h.svc.get_zone(zone.id)).await,
        Err(AppError::NotFound(_))
    ));
}

// ── Assign device ──────────────────────────────────────────────────────────

#[tokio::test]
async fn assign_device_moves_zone_and_emits_event() {
    let h = build().await;
    let device_id = Uuid::new_v4();
    insert_device(&h.devices, device_id, GUEST).await;

    let mut rx = h.events.subscribe();
    as_admin(h.svc.assign_device(device_id, IOT.parse().unwrap()))
        .await
        .unwrap();

    let device = h
        .devices
        .find_by_id(&device_id.to_string())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(device.zone_id.to_string(), IOT);

    let evt = rx.try_recv().unwrap();
    match evt {
        WardnetEvent::DeviceZoneChanged {
            device_id: d,
            old_zone_id,
            new_zone_id,
            ..
        } => {
            assert_eq!(d, device_id);
            assert_eq!(old_zone_id.to_string(), GUEST);
            assert_eq!(new_zone_id.to_string(), IOT);
        }
        other => panic!("expected DeviceZoneChanged, got {other:?}"),
    }
}

async fn give_tunnel_rule(devices: &Arc<dyn DeviceRepository>, device_id: Uuid) {
    let target = RoutingTarget::Tunnel {
        tunnel_id: Uuid::new_v4(),
    };
    let json = serde_json::to_string(&target).unwrap();
    devices
        .upsert_user_rule(
            &device_id.to_string(),
            &json,
            &chrono::Utc::now().to_rfc3339(),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn assign_device_rejects_move_that_would_violate_new_zone() {
    // Device has a Tunnel rule; moving it into a direct-only zone is rejected
    // rather than persisting a rule the zone forbids.
    let h = build().await;
    let direct_only = as_admin(h.svc.create_zone(CreateNetworkZoneRequest {
        name: "DirectOnly".to_owned(),
        isolation_stance: ZoneStance::SharedSubnet,
        allowed_targets: vec![AllowedTargetKind::Direct],
        member_isolation: false,
        admin_ui_reachable: true,
        subnet: None,
    }))
    .await
    .unwrap();

    let device_id = Uuid::new_v4();
    insert_device(&h.devices, device_id, GUEST).await;
    give_tunnel_rule(&h.devices, device_id).await;

    assert!(matches!(
        as_admin(h.svc.assign_device(device_id, direct_only.id)).await,
        Err(AppError::Conflict(_))
    ));
}

#[tokio::test]
async fn update_zone_rejects_narrowing_that_strands_a_member() {
    // A member holds a Tunnel rule; narrowing the zone to direct-only is
    // rejected until the rule is changed.
    let h = build().await;
    let zone = as_admin(h.svc.create_zone(create_req("Mixed")))
        .await
        .unwrap();
    let device_id = Uuid::new_v4();
    insert_device(&h.devices, device_id, &zone.id.to_string()).await;
    give_tunnel_rule(&h.devices, device_id).await;

    let req = UpdateNetworkZoneRequest {
        allowed_targets: Some(vec![AllowedTargetKind::Direct]),
        ..Default::default()
    };
    assert!(matches!(
        as_admin(h.svc.update_zone(zone.id, req)).await,
        Err(AppError::Conflict(_))
    ));
}

#[tokio::test]
async fn assign_device_unknown_device_is_404() {
    let h = build().await;
    assert!(matches!(
        as_admin(h.svc.assign_device(Uuid::new_v4(), IOT.parse().unwrap())).await,
        Err(AppError::NotFound(_))
    ));
}

#[tokio::test]
async fn assign_device_unknown_zone_is_404() {
    let h = build().await;
    let device_id = Uuid::new_v4();
    insert_device(&h.devices, device_id, GUEST).await;
    assert!(matches!(
        as_admin(h.svc.assign_device(device_id, Uuid::new_v4())).await,
        Err(AppError::NotFound(_))
    ));
}
