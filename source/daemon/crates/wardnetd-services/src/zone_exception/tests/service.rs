//! Service-layer tests for [`ZoneExceptionServiceImpl`].
//!
//! Builds real repositories (`SqliteZoneExceptionRepository`,
//! `SqliteNetworkZoneRepository`, `SqliteDeviceRepository`) over an in-memory
//! pool (the same migration set as the rest of the workspace, so the three
//! system zones are seeded) and exercises CRUD, endpoint / port validation, the
//! admin auth gate, and event emission.

use std::future::Future;
use std::sync::Arc;

use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use uuid::Uuid;
use wardnet_common::api::{CreateZoneExceptionRequest, UpdateZoneExceptionRequest};
use wardnet_common::auth::AuthContext;
use wardnet_common::event::WardnetEvent;
use wardnet_common::zone_exception::{
    ExceptionEndpoint, ExceptionEndpointKind, PortSpec, Proto, ServiceSet, ServiceSpec,
};
use wardnetd_data::repository::device::DeviceRow;
use wardnetd_data::repository::{
    DeviceRepository, SqliteDeviceRepository, SqliteNetworkZoneRepository,
    SqliteZoneExceptionRepository,
};

use crate::auth_context;
use crate::error::AppError;
use crate::event::{BroadcastEventBus, EventPublisher};
use crate::zone_exception::service::{ZoneExceptionService, ZoneExceptionServiceImpl};

use crate::tests::mac_from;

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
    svc: ZoneExceptionServiceImpl,
    devices: Arc<dyn DeviceRepository>,
    events: Arc<BroadcastEventBus>,
}

async fn build() -> Harness {
    let pool = test_pool().await;
    let exceptions = Arc::new(SqliteZoneExceptionRepository::new(pool.clone()));
    let zones = Arc::new(SqliteNetworkZoneRepository::new(pool.clone()));
    let devices: Arc<dyn DeviceRepository> = Arc::new(SqliteDeviceRepository::new(pool));
    let events = Arc::new(BroadcastEventBus::new(16));
    let svc = ZoneExceptionServiceImpl::new(exceptions, zones, devices.clone(), events.clone());
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

async fn insert_device(devices: &Arc<dyn DeviceRepository>, id: Uuid) {
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
            zone_id: GUEST.to_owned(),
            connection_mode: wardnet_common::device::DeviceConnectionMode::Lan,
        })
        .await
        .unwrap();
}

/// A valid create request: an existing device casting to the `IoT` zone via the
/// casting preset.
async fn valid_req(devices: &Arc<dyn DeviceRepository>) -> CreateZoneExceptionRequest {
    let device_id = Uuid::new_v4();
    insert_device(devices, device_id).await;
    CreateZoneExceptionRequest {
        from: ExceptionEndpoint {
            kind: ExceptionEndpointKind::Device,
            id: device_id,
        },
        to: ExceptionEndpoint {
            kind: ExceptionEndpointKind::Zone,
            id: IOT.parse().unwrap(),
        },
        service: ServiceSpec::Preset {
            set: ServiceSet::Casting,
        },
        bidirectional: true,
    }
}

// ── Auth gate ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn every_method_rejects_non_admin() {
    let h = build().await;
    assert!(matches!(
        h.svc.list_exceptions().await,
        Err(AppError::Forbidden(_))
    ));
    assert!(matches!(
        h.svc.get_exception(Uuid::new_v4()).await,
        Err(AppError::Forbidden(_))
    ));
    assert!(matches!(
        h.svc.delete_exception(Uuid::new_v4()).await,
        Err(AppError::Forbidden(_))
    ));
}

// ── Create + emit ─────────────────────────────────────────────────────────

#[tokio::test]
async fn create_persists_and_emits_event() {
    let h = build().await;
    let req = valid_req(&h.devices).await;
    let mut rx = h.events.subscribe();

    let created = as_admin(h.svc.create_exception(req)).await.unwrap();
    assert!(created.bidirectional);

    let fetched = as_admin(h.svc.get_exception(created.id)).await.unwrap();
    assert_eq!(fetched.id, created.id);

    assert!(matches!(
        rx.try_recv().unwrap(),
        WardnetEvent::ZoneExceptionsChanged { .. }
    ));
}

#[tokio::test]
async fn list_returns_created_exceptions() {
    let h = build().await;
    for _ in 0..2 {
        let req = valid_req(&h.devices).await;
        as_admin(h.svc.create_exception(req)).await.unwrap();
    }
    let all = as_admin(h.svc.list_exceptions()).await.unwrap();
    assert_eq!(all.len(), 2);
}

// ── Validation ───────────────────────────────────────────────────────────

#[tokio::test]
async fn create_rejects_missing_device_endpoint() {
    let h = build().await;
    let req = CreateZoneExceptionRequest {
        from: ExceptionEndpoint {
            kind: ExceptionEndpointKind::Device,
            id: Uuid::new_v4(), // never inserted
        },
        to: ExceptionEndpoint {
            kind: ExceptionEndpointKind::Zone,
            id: IOT.parse().unwrap(),
        },
        service: ServiceSpec::Preset {
            set: ServiceSet::Casting,
        },
        bidirectional: false,
    };
    assert!(matches!(
        as_admin(h.svc.create_exception(req)).await,
        Err(AppError::BadRequest(_))
    ));
}

#[tokio::test]
async fn create_rejects_missing_zone_endpoint() {
    let h = build().await;
    let device_id = Uuid::new_v4();
    insert_device(&h.devices, device_id).await;
    let req = CreateZoneExceptionRequest {
        from: ExceptionEndpoint {
            kind: ExceptionEndpointKind::Device,
            id: device_id,
        },
        to: ExceptionEndpoint {
            kind: ExceptionEndpointKind::Zone,
            id: Uuid::new_v4(), // no such zone
        },
        service: ServiceSpec::Preset {
            set: ServiceSet::Casting,
        },
        bidirectional: false,
    };
    assert!(matches!(
        as_admin(h.svc.create_exception(req)).await,
        Err(AppError::BadRequest(_))
    ));
}

#[tokio::test]
async fn create_rejects_from_equals_to() {
    let h = build().await;
    let endpoint = ExceptionEndpoint {
        kind: ExceptionEndpointKind::Zone,
        id: IOT.parse().unwrap(),
    };
    let req = CreateZoneExceptionRequest {
        from: endpoint.clone(),
        to: endpoint,
        service: ServiceSpec::Preset {
            set: ServiceSet::Casting,
        },
        bidirectional: false,
    };
    assert!(matches!(
        as_admin(h.svc.create_exception(req)).await,
        Err(AppError::BadRequest(_))
    ));
}

#[tokio::test]
async fn create_rejects_empty_ports() {
    let h = build().await;
    let mut req = valid_req(&h.devices).await;
    req.service = ServiceSpec::Ports { ports: vec![] };
    assert!(matches!(
        as_admin(h.svc.create_exception(req)).await,
        Err(AppError::BadRequest(_))
    ));
}

#[tokio::test]
async fn create_rejects_inverted_port_range() {
    let h = build().await;
    let mut req = valid_req(&h.devices).await;
    req.service = ServiceSpec::Ports {
        ports: vec![PortSpec {
            proto: Proto::Tcp,
            from: 900,
            to: 100,
        }],
    };
    assert!(matches!(
        as_admin(h.svc.create_exception(req)).await,
        Err(AppError::BadRequest(_))
    ));
}

#[tokio::test]
async fn create_rejects_zero_port() {
    let h = build().await;
    let mut req = valid_req(&h.devices).await;
    req.service = ServiceSpec::Ports {
        ports: vec![PortSpec {
            proto: Proto::Udp,
            from: 0,
            to: 0,
        }],
    };
    assert!(matches!(
        as_admin(h.svc.create_exception(req)).await,
        Err(AppError::BadRequest(_))
    ));
}

#[tokio::test]
async fn create_accepts_explicit_ports() {
    let h = build().await;
    let mut req = valid_req(&h.devices).await;
    req.service = ServiceSpec::Ports {
        ports: vec![PortSpec {
            proto: Proto::Tcp,
            from: 8000,
            to: 8100,
        }],
    };
    assert!(as_admin(h.svc.create_exception(req)).await.is_ok());
}

// ── Get / update / delete ──────────────────────────────────────────────────

#[tokio::test]
async fn get_unknown_is_404() {
    let h = build().await;
    assert!(matches!(
        as_admin(h.svc.get_exception(Uuid::new_v4())).await,
        Err(AppError::NotFound(_))
    ));
}

#[tokio::test]
async fn update_applies_partial_fields_and_emits() {
    let h = build().await;
    let req = valid_req(&h.devices).await;
    let created = as_admin(h.svc.create_exception(req)).await.unwrap();

    let mut rx = h.events.subscribe();
    let update = UpdateZoneExceptionRequest {
        bidirectional: Some(false),
        service: Some(ServiceSpec::Ports {
            ports: vec![PortSpec {
                proto: Proto::Tcp,
                from: 443,
                to: 443,
            }],
        }),
        ..Default::default()
    };
    let updated = as_admin(h.svc.update_exception(created.id, update))
        .await
        .unwrap();
    assert!(!updated.bidirectional);
    assert_eq!(
        updated.service,
        ServiceSpec::Ports {
            ports: vec![PortSpec {
                proto: Proto::Tcp,
                from: 443,
                to: 443,
            }]
        }
    );
    assert!(matches!(
        rx.try_recv().unwrap(),
        WardnetEvent::ZoneExceptionsChanged { .. }
    ));
}

#[tokio::test]
async fn update_rejects_invalid_new_endpoint() {
    let h = build().await;
    let req = valid_req(&h.devices).await;
    let created = as_admin(h.svc.create_exception(req)).await.unwrap();

    let update = UpdateZoneExceptionRequest {
        to: Some(ExceptionEndpoint {
            kind: ExceptionEndpointKind::Zone,
            id: Uuid::new_v4(),
        }),
        ..Default::default()
    };
    assert!(matches!(
        as_admin(h.svc.update_exception(created.id, update)).await,
        Err(AppError::BadRequest(_))
    ));
}

#[tokio::test]
async fn update_unknown_is_404() {
    let h = build().await;
    assert!(matches!(
        as_admin(
            h.svc
                .update_exception(Uuid::new_v4(), UpdateZoneExceptionRequest::default())
        )
        .await,
        Err(AppError::NotFound(_))
    ));
}

#[tokio::test]
async fn delete_removes_and_emits() {
    let h = build().await;
    let req = valid_req(&h.devices).await;
    let created = as_admin(h.svc.create_exception(req)).await.unwrap();

    let mut rx = h.events.subscribe();
    as_admin(h.svc.delete_exception(created.id)).await.unwrap();
    assert!(matches!(
        rx.try_recv().unwrap(),
        WardnetEvent::ZoneExceptionsChanged { .. }
    ));
    assert!(matches!(
        as_admin(h.svc.get_exception(created.id)).await,
        Err(AppError::NotFound(_))
    ));
}

#[tokio::test]
async fn delete_unknown_is_404() {
    let h = build().await;
    assert!(matches!(
        as_admin(h.svc.delete_exception(Uuid::new_v4())).await,
        Err(AppError::NotFound(_))
    ));
}
