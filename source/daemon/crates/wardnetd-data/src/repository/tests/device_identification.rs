use wardnet_common::device::{DeviceSignalKind, ManufacturerSource};

use super::test_pool;
use crate::repository::{
    DeviceIdentificationRepository, DeviceRepository, DeviceSignalRow,
    SqliteDeviceIdentificationRepository, SqliteDeviceRepository,
};

const DEV1: &str = "00000000-0000-0000-0000-000000000001";
const ZONE: &str = "00000000-0000-0000-0000-000000000201";

/// Insert a bare device row so signals have a valid FK target.
async fn insert_device(pool: &sqlx::SqlitePool, id: &str, mac: &str) {
    sqlx::query(
        "INSERT INTO devices (id, mac, last_ip, device_type, first_seen, last_seen, zone_id) \
         VALUES (?, ?, '192.168.1.10', 'unknown', '2026-03-07T00:00:00Z', '2026-03-07T00:00:00Z', ?)",
    )
    .bind(id)
    .bind(mac)
    .bind(ZONE)
    .execute(pool)
    .await
    .unwrap();
}

fn row(device_id: &str, kind: DeviceSignalKind, value: &str) -> DeviceSignalRow {
    DeviceSignalRow {
        device_id: device_id.to_owned(),
        kind,
        value: value.to_owned(),
        inferred: false,
    }
}

#[tokio::test]
async fn record_then_read_back_a_signal() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01").await;
    let repo = SqliteDeviceIdentificationRepository::new(pool);

    repo.record(&row(DEV1, DeviceSignalKind::MdnsService, "_govee._tcp"))
        .await
        .unwrap();

    let signals = repo.find_by_device(DEV1).await.unwrap();
    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].kind, DeviceSignalKind::MdnsService);
    assert_eq!(signals[0].value, "_govee._tcp");
}

#[tokio::test]
async fn inferred_flag_round_trips() {
    // The flag is what lets the UI show which observation produced a hedged
    // "likely <vendor>" name, so it has to survive the write/read cycle rather
    // than being write-only bookkeeping.
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01").await;
    let repo = SqliteDeviceIdentificationRepository::new(pool);

    repo.record(&DeviceSignalRow {
        inferred: true,
        ..row(DEV1, DeviceSignalKind::MdnsService, "_govee._tcp")
    })
    .await
    .unwrap();
    repo.record(&row(DEV1, DeviceSignalKind::DhcpHostname, "some-host"))
        .await
        .unwrap();

    let signals = repo.find_by_device(DEV1).await.unwrap();
    let mdns = signals
        .iter()
        .find(|s| s.kind == DeviceSignalKind::MdnsService)
        .expect("expected the mDNS signal");
    let hostname = signals
        .iter()
        .find(|s| s.kind == DeviceSignalKind::DhcpHostname)
        .expect("expected the hostname signal");
    assert!(mdns.inferred, "a catalog match must read back as inferred");
    assert!(
        !hostname.inferred,
        "a plain observation must not read back as inferred"
    );
}

#[tokio::test]
async fn re_recording_the_same_fact_does_not_duplicate() {
    // A device re-announcing an unchanged mDNS service on every renewal is the
    // normal case; it must refresh the row rather than pile up duplicates.
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01").await;
    let repo = SqliteDeviceIdentificationRepository::new(pool);

    for _ in 0..3 {
        repo.record(&row(DEV1, DeviceSignalKind::MdnsService, "_govee._tcp"))
            .await
            .unwrap();
    }

    assert_eq!(repo.find_by_device(DEV1).await.unwrap().len(), 1);
}

#[tokio::test]
async fn signals_of_different_kinds_coexist() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01").await;
    let repo = SqliteDeviceIdentificationRepository::new(pool);

    repo.record(&row(DEV1, DeviceSignalKind::DhcpHostname, "govee-lamp"))
        .await
        .unwrap();
    repo.record(&row(DEV1, DeviceSignalKind::DhcpParamList, "1,3,6"))
        .await
        .unwrap();

    assert_eq!(repo.find_by_device(DEV1).await.unwrap().len(), 2);
}

#[tokio::test]
async fn prune_keeps_only_the_newest_values_for_a_kind() {
    // Guards the unbounded-growth path: signals come from an unauthenticated
    // LAN client, and `value` is part of the natural key, so a device varying
    // its vendor class every renewal would otherwise append rows forever.
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01").await;
    let repo = SqliteDeviceIdentificationRepository::new(pool);

    for i in 0..10 {
        repo.record(&row(
            DEV1,
            DeviceSignalKind::DhcpVendorClass,
            &format!("vendor-{i}"),
        ))
        .await
        .unwrap();
    }
    // A different kind must not be touched by the prune.
    repo.record(&row(DEV1, DeviceSignalKind::DhcpHostname, "govee-lamp"))
        .await
        .unwrap();

    repo.prune_signals(DEV1, DeviceSignalKind::DhcpVendorClass, 3)
        .await
        .unwrap();

    let signals = repo.find_by_device(DEV1).await.unwrap();
    let vendor_class: Vec<_> = signals
        .iter()
        .filter(|s| s.kind == DeviceSignalKind::DhcpVendorClass)
        .collect();
    assert_eq!(
        vendor_class.len(),
        3,
        "vendor class capped at the keep count"
    );
    assert_eq!(
        signals
            .iter()
            .filter(|s| s.kind == DeviceSignalKind::DhcpHostname)
            .count(),
        1,
        "prune must be scoped to one kind"
    );
}

#[tokio::test]
async fn set_manufacturer_names_a_device_that_has_none() {
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "5c:e7:53:4e:ec:d9").await;
    let repo = SqliteDeviceIdentificationRepository::new(pool.clone());
    let devices = SqliteDeviceRepository::new(pool);

    let named = repo
        .set_manufacturer_if_absent(DEV1, "Govee", ManufacturerSource::Catalog)
        .await
        .unwrap();

    assert!(named);
    let device = devices.find_by_id(DEV1).await.unwrap().unwrap();
    assert_eq!(device.manufacturer.as_deref(), Some("Govee"));
    assert_eq!(
        device.manufacturer_source,
        Some(ManufacturerSource::Catalog)
    );
}

#[tokio::test]
async fn set_manufacturer_never_overwrites_an_existing_name() {
    // First-writer-wins, enforced in SQL. Without it, a device advertising
    // several vendors' services would flip its manufacturer on every
    // re-announcement.
    let pool = test_pool().await;
    insert_device(&pool, DEV1, "aa:bb:cc:dd:ee:01").await;
    let repo = SqliteDeviceIdentificationRepository::new(pool.clone());
    let devices = SqliteDeviceRepository::new(pool);

    assert!(
        repo.set_manufacturer_if_absent(DEV1, "Apple", ManufacturerSource::Signal)
            .await
            .unwrap()
    );
    assert!(
        !repo
            .set_manufacturer_if_absent(DEV1, "Google", ManufacturerSource::Signal)
            .await
            .unwrap(),
        "second writer must report that it changed nothing"
    );

    let device = devices.find_by_id(DEV1).await.unwrap().unwrap();
    assert_eq!(device.manufacturer.as_deref(), Some("Apple"));
}

#[tokio::test]
async fn set_manufacturer_on_a_missing_device_is_a_no_op() {
    let pool = test_pool().await;
    let repo = SqliteDeviceIdentificationRepository::new(pool);

    let named = repo
        .set_manufacturer_if_absent(DEV1, "Govee", ManufacturerSource::Catalog)
        .await
        .unwrap();

    assert!(!named, "no device, so nothing to name");
}
