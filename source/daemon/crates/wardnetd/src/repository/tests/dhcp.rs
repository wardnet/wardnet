use super::test_pool;
use crate::repository::dhcp::{DhcpLeaseLogRow, DhcpLeaseRow, DhcpReservationRow};
use crate::repository::{DhcpRepository, SqliteDhcpRepository};
use chrono::Utc;
use wardnet_types::dhcp::{DhcpLeaseEventType, DhcpLeaseStatus};

fn sample_lease_row(id: &str, mac: &str, ip: &str) -> DhcpLeaseRow {
    let now = Utc::now();
    let end = now + chrono::Duration::hours(24);
    DhcpLeaseRow {
        id: id.to_owned(),
        mac_address: mac.to_owned(),
        ip_address: ip.to_owned(),
        hostname: Some("test-host".to_owned()),
        lease_start: now.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        lease_end: end.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        status: "active".to_owned(),
        device_id: None,
    }
}

fn sample_reservation_row(id: &str, mac: &str, ip: &str) -> DhcpReservationRow {
    DhcpReservationRow {
        id: id.to_owned(),
        mac_address: mac.to_owned(),
        ip_address: ip.to_owned(),
        hostname: Some("reserved-host".to_owned()),
        description: Some("Test reservation".to_owned()),
    }
}

#[tokio::test]
async fn insert_and_find_lease_by_id() {
    let pool = test_pool().await;
    let repo = SqliteDhcpRepository::new(pool);
    let id = "00000000-0000-0000-0000-000000000001";

    repo.insert_lease(&sample_lease_row(id, "AA:BB:CC:DD:EE:01", "192.168.1.100"))
        .await
        .unwrap();

    let lease = repo.find_lease_by_id(id).await.unwrap().unwrap();
    assert_eq!(lease.id.to_string(), id);
    assert_eq!(lease.mac_address, "AA:BB:CC:DD:EE:01");
    assert_eq!(lease.ip_address.to_string(), "192.168.1.100");
    assert_eq!(lease.hostname, Some("test-host".to_owned()));
    assert_eq!(lease.status, DhcpLeaseStatus::Active);
    assert!(lease.device_id.is_none());
}

#[tokio::test]
async fn find_lease_by_id_returns_none_for_missing() {
    let pool = test_pool().await;
    let repo = SqliteDhcpRepository::new(pool);

    let result = repo
        .find_lease_by_id("00000000-0000-0000-0000-000000000099")
        .await
        .unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn find_active_lease_by_mac() {
    let pool = test_pool().await;
    let repo = SqliteDhcpRepository::new(pool);

    repo.insert_lease(&sample_lease_row(
        "00000000-0000-0000-0000-000000000001",
        "AA:BB:CC:DD:EE:01",
        "192.168.1.100",
    ))
    .await
    .unwrap();

    let lease = repo
        .find_active_lease_by_mac("AA:BB:CC:DD:EE:01")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(lease.mac_address, "AA:BB:CC:DD:EE:01");

    // Non-existent MAC returns None.
    let missing = repo
        .find_active_lease_by_mac("FF:FF:FF:FF:FF:FF")
        .await
        .unwrap();
    assert!(missing.is_none());
}

#[tokio::test]
async fn find_active_lease_by_ip() {
    let pool = test_pool().await;
    let repo = SqliteDhcpRepository::new(pool);

    repo.insert_lease(&sample_lease_row(
        "00000000-0000-0000-0000-000000000001",
        "AA:BB:CC:DD:EE:01",
        "192.168.1.100",
    ))
    .await
    .unwrap();

    let lease = repo
        .find_active_lease_by_ip("192.168.1.100")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(lease.ip_address.to_string(), "192.168.1.100");

    // Non-existent IP returns None.
    let missing = repo.find_active_lease_by_ip("10.0.0.1").await.unwrap();
    assert!(missing.is_none());
}

#[tokio::test]
async fn list_active_leases() {
    let pool = test_pool().await;
    let repo = SqliteDhcpRepository::new(pool);

    repo.insert_lease(&sample_lease_row(
        "00000000-0000-0000-0000-000000000001",
        "AA:BB:CC:DD:EE:01",
        "192.168.1.100",
    ))
    .await
    .unwrap();
    repo.insert_lease(&sample_lease_row(
        "00000000-0000-0000-0000-000000000002",
        "AA:BB:CC:DD:EE:02",
        "192.168.1.101",
    ))
    .await
    .unwrap();

    // Mark the second lease as expired.
    repo.update_lease_status("00000000-0000-0000-0000-000000000002", "expired")
        .await
        .unwrap();

    let active = repo.list_active_leases().await.unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].mac_address, "AA:BB:CC:DD:EE:01");
}

#[tokio::test]
async fn update_lease_status() {
    let pool = test_pool().await;
    let repo = SqliteDhcpRepository::new(pool);
    let id = "00000000-0000-0000-0000-000000000001";

    repo.insert_lease(&sample_lease_row(id, "AA:BB:CC:DD:EE:01", "192.168.1.100"))
        .await
        .unwrap();
    repo.update_lease_status(id, "released").await.unwrap();

    let lease = repo.find_lease_by_id(id).await.unwrap().unwrap();
    assert_eq!(lease.status, DhcpLeaseStatus::Released);
}

#[tokio::test]
async fn renew_lease() {
    let pool = test_pool().await;
    let repo = SqliteDhcpRepository::new(pool);
    let id = "00000000-0000-0000-0000-000000000001";

    repo.insert_lease(&sample_lease_row(id, "AA:BB:CC:DD:EE:01", "192.168.1.100"))
        .await
        .unwrap();

    let new_end = "2099-12-31T23:59:59Z";
    repo.renew_lease(id, new_end).await.unwrap();

    let lease = repo.find_lease_by_id(id).await.unwrap().unwrap();
    assert_eq!(
        lease.lease_end.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        new_end
    );
}

#[tokio::test]
async fn expire_stale_leases() {
    let pool = test_pool().await;
    let repo = SqliteDhcpRepository::new(pool);

    // Insert a lease that already expired (lease_end in the past).
    let mut stale = sample_lease_row(
        "00000000-0000-0000-0000-000000000001",
        "AA:BB:CC:DD:EE:01",
        "192.168.1.100",
    );
    stale.lease_end = "2020-01-01T00:00:00Z".to_owned();

    // Insert a lease that is still active (lease_end far in the future).
    let fresh = sample_lease_row(
        "00000000-0000-0000-0000-000000000002",
        "AA:BB:CC:DD:EE:02",
        "192.168.1.101",
    );

    repo.insert_lease(&stale).await.unwrap();
    repo.insert_lease(&fresh).await.unwrap();

    let expired_count = repo.expire_stale_leases().await.unwrap();
    assert_eq!(expired_count, 1);

    // The stale lease should now be expired.
    let lease1 = repo
        .find_lease_by_id("00000000-0000-0000-0000-000000000001")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(lease1.status, DhcpLeaseStatus::Expired);

    // The fresh lease should still be active.
    let lease2 = repo
        .find_lease_by_id("00000000-0000-0000-0000-000000000002")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(lease2.status, DhcpLeaseStatus::Active);
}

#[tokio::test]
async fn insert_and_list_reservations() {
    let pool = test_pool().await;
    let repo = SqliteDhcpRepository::new(pool);

    repo.insert_reservation(&sample_reservation_row(
        "00000000-0000-0000-0000-000000000001",
        "AA:BB:CC:DD:EE:01",
        "192.168.1.50",
    ))
    .await
    .unwrap();
    repo.insert_reservation(&sample_reservation_row(
        "00000000-0000-0000-0000-000000000002",
        "AA:BB:CC:DD:EE:02",
        "192.168.1.51",
    ))
    .await
    .unwrap();

    let reservations = repo.list_reservations().await.unwrap();
    assert_eq!(reservations.len(), 2);
}

#[tokio::test]
async fn delete_reservation() {
    let pool = test_pool().await;
    let repo = SqliteDhcpRepository::new(pool);
    let id = "00000000-0000-0000-0000-000000000001";

    repo.insert_reservation(&sample_reservation_row(
        id,
        "AA:BB:CC:DD:EE:01",
        "192.168.1.50",
    ))
    .await
    .unwrap();
    repo.delete_reservation(id).await.unwrap();

    let reservations = repo.list_reservations().await.unwrap();
    assert!(reservations.is_empty());
}

#[tokio::test]
async fn find_reservation_by_mac() {
    let pool = test_pool().await;
    let repo = SqliteDhcpRepository::new(pool);

    repo.insert_reservation(&sample_reservation_row(
        "00000000-0000-0000-0000-000000000001",
        "AA:BB:CC:DD:EE:01",
        "192.168.1.50",
    ))
    .await
    .unwrap();

    let res = repo
        .find_reservation_by_mac("AA:BB:CC:DD:EE:01")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(res.mac_address, "AA:BB:CC:DD:EE:01");
    assert_eq!(res.ip_address.to_string(), "192.168.1.50");
    assert_eq!(res.hostname, Some("reserved-host".to_owned()));
    assert_eq!(res.description, Some("Test reservation".to_owned()));

    // Non-existent MAC returns None.
    let missing = repo
        .find_reservation_by_mac("FF:FF:FF:FF:FF:FF")
        .await
        .unwrap();
    assert!(missing.is_none());
}

#[tokio::test]
async fn find_reservation_by_ip() {
    let pool = test_pool().await;
    let repo = SqliteDhcpRepository::new(pool);

    repo.insert_reservation(&sample_reservation_row(
        "00000000-0000-0000-0000-000000000001",
        "AA:BB:CC:DD:EE:01",
        "192.168.1.50",
    ))
    .await
    .unwrap();

    let res = repo
        .find_reservation_by_ip("192.168.1.50")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(res.ip_address.to_string(), "192.168.1.50");
    assert_eq!(res.mac_address, "AA:BB:CC:DD:EE:01");

    // Non-existent IP returns None.
    let missing = repo.find_reservation_by_ip("10.0.0.1").await.unwrap();
    assert!(missing.is_none());
}

#[tokio::test]
async fn insert_and_find_lease_logs() {
    let pool = test_pool().await;
    let repo = SqliteDhcpRepository::new(pool);
    let lease_id = "00000000-0000-0000-0000-000000000001";

    // Insert a lease first (the log references it by ID).
    repo.insert_lease(&sample_lease_row(
        lease_id,
        "AA:BB:CC:DD:EE:01",
        "192.168.1.100",
    ))
    .await
    .unwrap();

    repo.insert_lease_log(&DhcpLeaseLogRow {
        lease_id: lease_id.to_owned(),
        event_type: "assigned".to_owned(),
        details: Some("Initial assignment".to_owned()),
    })
    .await
    .unwrap();

    repo.insert_lease_log(&DhcpLeaseLogRow {
        lease_id: lease_id.to_owned(),
        event_type: "renewed".to_owned(),
        details: None,
    })
    .await
    .unwrap();

    let logs = repo.find_lease_logs(lease_id).await.unwrap();
    assert_eq!(logs.len(), 2);
    assert_eq!(logs[0].event_type, DhcpLeaseEventType::Assigned);
    assert_eq!(logs[0].details, Some("Initial assignment".to_owned()));
    assert_eq!(logs[1].event_type, DhcpLeaseEventType::Renewed);
    assert!(logs[1].details.is_none());
}

#[tokio::test]
async fn find_lease_by_id_released_status() {
    let pool = test_pool().await;
    let repo = SqliteDhcpRepository::new(pool);
    let id = "00000000-0000-0000-0000-000000000003";

    let mut row = sample_lease_row(id, "AA:BB:CC:DD:EE:03", "192.168.1.103");
    row.status = "released".to_owned();
    repo.insert_lease(&row).await.unwrap();

    let lease = repo.find_lease_by_id(id).await.unwrap().unwrap();
    assert_eq!(lease.status, DhcpLeaseStatus::Released);
}

#[tokio::test]
async fn find_lease_by_id_expired_status() {
    let pool = test_pool().await;
    let repo = SqliteDhcpRepository::new(pool);
    let id = "00000000-0000-0000-0000-000000000004";

    let mut row = sample_lease_row(id, "AA:BB:CC:DD:EE:04", "192.168.1.104");
    row.status = "expired".to_owned();
    repo.insert_lease(&row).await.unwrap();

    let lease = repo.find_lease_by_id(id).await.unwrap().unwrap();
    assert_eq!(lease.status, DhcpLeaseStatus::Expired);
}

#[tokio::test]
async fn insert_and_find_lease_log_all_event_types() {
    let pool = test_pool().await;
    let repo = SqliteDhcpRepository::new(pool);
    let lease_id = "00000000-0000-0000-0000-000000000010";

    repo.insert_lease(&sample_lease_row(
        lease_id,
        "AA:BB:CC:DD:EE:10",
        "192.168.1.110",
    ))
    .await
    .unwrap();

    // Insert all known event types.
    for event_type in &["assigned", "renewed", "released", "expired", "conflict"] {
        repo.insert_lease_log(&DhcpLeaseLogRow {
            lease_id: lease_id.to_owned(),
            event_type: (*event_type).to_owned(),
            details: Some(format!("test {event_type}")),
        })
        .await
        .unwrap();
    }

    let logs = repo.find_lease_logs(lease_id).await.unwrap();
    assert_eq!(logs.len(), 5);
    assert_eq!(logs[0].event_type, DhcpLeaseEventType::Assigned);
    assert_eq!(logs[1].event_type, DhcpLeaseEventType::Renewed);
    assert_eq!(logs[2].event_type, DhcpLeaseEventType::Released);
    assert_eq!(logs[3].event_type, DhcpLeaseEventType::Expired);
    assert_eq!(logs[4].event_type, DhcpLeaseEventType::Conflict);
}

#[tokio::test]
async fn find_lease_logs_empty_for_no_logs() {
    let pool = test_pool().await;
    let repo = SqliteDhcpRepository::new(pool);

    let logs = repo
        .find_lease_logs("00000000-0000-0000-0000-000000000099")
        .await
        .unwrap();
    assert!(logs.is_empty());
}

#[tokio::test]
async fn expire_stale_leases_none_to_expire() {
    let pool = test_pool().await;
    let repo = SqliteDhcpRepository::new(pool);

    // Insert only a fresh lease.
    repo.insert_lease(&sample_lease_row(
        "00000000-0000-0000-0000-000000000005",
        "AA:BB:CC:DD:EE:05",
        "192.168.1.105",
    ))
    .await
    .unwrap();

    let count = repo.expire_stale_leases().await.unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn delete_reservation_nonexistent() {
    let pool = test_pool().await;
    let repo = SqliteDhcpRepository::new(pool);

    // Deleting a non-existent reservation should succeed (no-op in SQL).
    repo.delete_reservation("00000000-0000-0000-0000-000000000099")
        .await
        .unwrap();

    let reservations = repo.list_reservations().await.unwrap();
    assert!(reservations.is_empty());
}

#[tokio::test]
async fn reservation_without_optional_fields() {
    let pool = test_pool().await;
    let repo = SqliteDhcpRepository::new(pool);
    let id = "00000000-0000-0000-0000-000000000006";

    let row = DhcpReservationRow {
        id: id.to_owned(),
        mac_address: "AA:BB:CC:DD:EE:06".to_owned(),
        ip_address: "192.168.1.60".to_owned(),
        hostname: None,
        description: None,
    };
    repo.insert_reservation(&row).await.unwrap();

    let res = repo
        .find_reservation_by_mac("AA:BB:CC:DD:EE:06")
        .await
        .unwrap()
        .unwrap();
    assert!(res.hostname.is_none());
    assert!(res.description.is_none());
}
