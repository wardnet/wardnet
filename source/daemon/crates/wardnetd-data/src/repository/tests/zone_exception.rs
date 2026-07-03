//! Repository + migration tests for cross-zone exceptions (issue #737).

use wardnet_common::zone_exception::{
    ExceptionEndpoint, ExceptionEndpointKind, PortSpec, Proto, ServiceSet, ServiceSpec,
    ZoneException,
};

use super::test_pool;
use crate::repository::ZoneExceptionRepository;
use crate::repository::sqlite::SqliteZoneExceptionRepository;

fn exception(service: ServiceSpec, bidirectional: bool) -> ZoneException {
    let now = chrono::Utc::now();
    ZoneException {
        id: uuid::Uuid::new_v4(),
        from: ExceptionEndpoint {
            kind: ExceptionEndpointKind::Device,
            id: uuid::Uuid::new_v4(),
        },
        to: ExceptionEndpoint {
            kind: ExceptionEndpointKind::Zone,
            id: uuid::Uuid::new_v4(),
        },
        service,
        bidirectional,
        created_at: now,
        updated_at: now,
    }
}

#[tokio::test]
async fn insert_round_trips_preset_and_endpoint_kinds() {
    let pool = test_pool().await;
    let repo = SqliteZoneExceptionRepository::new(pool);
    let e = exception(
        ServiceSpec::Preset {
            set: ServiceSet::Casting,
        },
        true,
    );
    repo.insert(&e).await.unwrap();

    let fetched = repo.find_by_id(&e.id.to_string()).await.unwrap().unwrap();
    assert_eq!(fetched.from.kind, ExceptionEndpointKind::Device);
    assert_eq!(fetched.from.id, e.from.id);
    assert_eq!(fetched.to.kind, ExceptionEndpointKind::Zone);
    assert_eq!(fetched.to.id, e.to.id);
    assert!(fetched.bidirectional);
    assert_eq!(
        fetched.service,
        ServiceSpec::Preset {
            set: ServiceSet::Casting
        }
    );
}

#[tokio::test]
async fn insert_round_trips_explicit_ports() {
    let pool = test_pool().await;
    let repo = SqliteZoneExceptionRepository::new(pool);
    let ports = vec![
        PortSpec {
            proto: Proto::Tcp,
            from: 8000,
            to: 8100,
        },
        PortSpec {
            proto: Proto::Udp,
            from: 5353,
            to: 5353,
        },
    ];
    let e = exception(
        ServiceSpec::Ports {
            ports: ports.clone(),
        },
        false,
    );
    repo.insert(&e).await.unwrap();

    let fetched = repo.find_by_id(&e.id.to_string()).await.unwrap().unwrap();
    assert!(!fetched.bidirectional);
    assert_eq!(fetched.service, ServiceSpec::Ports { ports });
}

#[tokio::test]
async fn update_and_delete() {
    let pool = test_pool().await;
    let repo = SqliteZoneExceptionRepository::new(pool);
    let mut e = exception(
        ServiceSpec::Preset {
            set: ServiceSet::Casting,
        },
        false,
    );
    repo.insert(&e).await.unwrap();

    e.bidirectional = true;
    e.service = ServiceSpec::Ports {
        ports: vec![PortSpec {
            proto: Proto::Tcp,
            from: 443,
            to: 443,
        }],
    };
    repo.update(&e).await.unwrap();

    let fetched = repo.find_by_id(&e.id.to_string()).await.unwrap().unwrap();
    assert!(fetched.bidirectional);
    assert_eq!(
        fetched.service,
        ServiceSpec::Ports {
            ports: vec![PortSpec {
                proto: Proto::Tcp,
                from: 443,
                to: 443,
            }]
        }
    );

    repo.delete(&e.id.to_string()).await.unwrap();
    assert!(repo.find_by_id(&e.id.to_string()).await.unwrap().is_none());
}

#[tokio::test]
async fn find_all_returns_every_row() {
    let pool = test_pool().await;
    let repo = SqliteZoneExceptionRepository::new(pool);
    for _ in 0..3 {
        repo.insert(&exception(
            ServiceSpec::Preset {
                set: ServiceSet::Casting,
            },
            false,
        ))
        .await
        .unwrap();
    }
    assert_eq!(repo.find_all().await.unwrap().len(), 3);
}
