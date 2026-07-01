//! Repository + migration tests for Network Zones (issue #735).

use std::sync::Arc;

use wardnet_common::network_zone::{AllowedTargetKind, NetworkZone, ZoneProvenance, ZoneStance};

use super::test_pool;
use crate::repository::NetworkZoneRepository;
use crate::repository::sqlite::SqliteNetworkZoneRepository;

fn zone(name: &str, targets: Vec<AllowedTargetKind>) -> NetworkZone {
    let now = chrono::Utc::now();
    NetworkZone {
        id: uuid::Uuid::new_v4(),
        name: name.to_owned(),
        provenance: ZoneProvenance::Manual,
        isolation_stance: ZoneStance::SharedSubnet,
        allowed_targets: targets,
        member_isolation: false,
        subnet: None,
        admin_ui_reachable: true,
        is_default: false,
        is_default_for_new: false,
        created_at: now,
        updated_at: now,
    }
}

#[tokio::test]
async fn migration_seeds_exactly_three_system_zones() {
    let pool = test_pool().await;
    let repo = SqliteNetworkZoneRepository::new(pool);

    let zones = repo.find_all().await.unwrap();
    let names: Vec<_> = zones.iter().map(|z| z.name.as_str()).collect();
    assert_eq!(zones.len(), 3);
    assert!(names.contains(&"Trusted"));
    assert!(names.contains(&"IoT"));
    assert!(names.contains(&"Guest"));
    assert!(zones.iter().all(|z| z.provenance == ZoneProvenance::System));

    let default = repo.find_default().await.unwrap();
    assert_eq!(default.name, "Trusted");
    let default_for_new = repo.find_default_for_new().await.unwrap();
    assert_eq!(default_for_new.name, "Guest");
}

#[tokio::test]
async fn migration_leaves_devices_table_empty() {
    let pool = test_pool().await;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn insert_round_trips_allowed_targets_and_subnet() {
    let pool = test_pool().await;
    let repo = SqliteNetworkZoneRepository::new(pool);
    let z = zone("Cameras", vec![AllowedTargetKind::Tunnel]);
    repo.insert(&z).await.unwrap();

    let fetched = repo.find_by_id(&z.id.to_string()).await.unwrap().unwrap();
    assert_eq!(fetched.name, "Cameras");
    assert_eq!(fetched.allowed_targets, vec![AllowedTargetKind::Tunnel]);
    assert_eq!(fetched.provenance, ZoneProvenance::Manual);
    assert!(fetched.subnet.is_none());
}

#[tokio::test]
async fn set_default_is_transactional_and_exclusive() {
    let pool = test_pool().await;
    let repo = SqliteNetworkZoneRepository::new(pool);
    let iot = repo
        .find_all()
        .await
        .unwrap()
        .into_iter()
        .find(|z| z.name == "IoT")
        .unwrap();

    repo.set_default(&iot.id.to_string()).await.unwrap();

    let all = repo.find_all().await.unwrap();
    let defaults: Vec<_> = all.iter().filter(|z| z.is_default).collect();
    assert_eq!(defaults.len(), 1);
    assert_eq!(defaults[0].name, "IoT");
}

#[tokio::test]
async fn count_members_reflects_zone_assignment() {
    let pool = test_pool().await;
    let repo = Arc::new(SqliteNetworkZoneRepository::new(pool));
    let guest = repo.find_default_for_new().await.unwrap();
    // No devices inserted → zero members.
    assert_eq!(repo.count_members(&guest.id.to_string()).await.unwrap(), 0);
}
