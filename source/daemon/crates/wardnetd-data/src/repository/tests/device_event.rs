use chrono::{DateTime, Duration, TimeZone, Utc};
use wardnet_common::device_event::DeviceEventKind;

use super::test_pool;
use crate::repository::device_event::{DeviceEventRepository, NewDeviceEvent};
use crate::repository::sqlite::SqliteDeviceEventRepository;

const DEVICE: &str = "3207490c-4c4b-4ab9-b173-7bda16975bc1";
const OTHER_DEVICE: &str = "14e479f1-98b7-4ba8-a097-97f3ac2a1c64";
const MAC: &str = "8c:86:dd:3d:0f:96";
const OTHER_MAC: &str = "80:69:1a:75:e1:58";

fn at(hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 6, hour, minute, 0).unwrap()
}

async fn repo() -> SqliteDeviceEventRepository {
    SqliteDeviceEventRepository::new(test_pool().await)
}

fn event<'a>(
    device_id: &'a str,
    mac: &'a str,
    kind: DeviceEventKind,
    created_at: DateTime<Utc>,
) -> NewDeviceEvent<'a> {
    NewDeviceEvent {
        device_id,
        mac,
        kind,
        details: None,
        created_at,
    }
}

#[tokio::test]
async fn a_recorded_event_is_returned_for_its_device() {
    let repo = repo().await;

    repo.record(NewDeviceEvent {
        device_id: DEVICE,
        mac: MAC,
        kind: DeviceEventKind::IpChanged,
        details: Some(r#"{"old_ip":"192.168.100.23","new_ip":"192.168.100.41"}"#),
        created_at: at(10, 0),
    })
    .await
    .unwrap();

    let events = repo
        .list_for_device(DEVICE, at(9, 0), at(11, 0))
        .await
        .unwrap();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, DeviceEventKind::IpChanged);
    assert_eq!(events[0].mac, MAC);
    assert_eq!(events[0].created_at, at(10, 0));
    assert_eq!(
        events[0].details.as_deref(),
        Some(r#"{"old_ip":"192.168.100.23","new_ip":"192.168.100.41"}"#)
    );
}

/// The timeline asks for a window, so events either side of it must not leak in
/// — otherwise a "last hour" view silently shows a month.
#[tokio::test]
async fn listing_is_bounded_by_the_requested_window_and_device() {
    let repo = repo().await;

    repo.record(event(DEVICE, MAC, DeviceEventKind::Gone, at(8, 0)))
        .await
        .unwrap();
    repo.record(event(DEVICE, MAC, DeviceEventKind::Returned, at(10, 0)))
        .await
        .unwrap();
    repo.record(event(DEVICE, MAC, DeviceEventKind::Gone, at(23, 0)))
        .await
        .unwrap();
    repo.record(event(
        OTHER_DEVICE,
        OTHER_MAC,
        DeviceEventKind::Discovered,
        at(10, 30),
    ))
    .await
    .unwrap();

    let events = repo
        .list_for_device(DEVICE, at(9, 0), at(11, 0))
        .await
        .unwrap();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, DeviceEventKind::Returned);
}

#[tokio::test]
async fn events_are_returned_oldest_first() {
    let repo = repo().await;

    repo.record(event(DEVICE, MAC, DeviceEventKind::Returned, at(10, 30)))
        .await
        .unwrap();
    repo.record(event(DEVICE, MAC, DeviceEventKind::Gone, at(10, 0)))
        .await
        .unwrap();

    let events = repo
        .list_for_device(DEVICE, at(9, 0), at(11, 0))
        .await
        .unwrap();

    let kinds: Vec<_> = events.iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        vec![DeviceEventKind::Gone, DeviceEventKind::Returned]
    );
}

/// What the address-churn detector reads. It counts *transitions* for one
/// station, so it must not be diluted by other kinds or other MACs.
#[tokio::test]
async fn counting_for_a_mac_is_scoped_to_kind_mac_and_window() {
    let repo = repo().await;

    for minute in 0..5 {
        repo.record(event(
            DEVICE,
            MAC,
            DeviceEventKind::IpChanged,
            at(10, minute),
        ))
        .await
        .unwrap();
    }
    // A different kind for the same MAC.
    repo.record(event(DEVICE, MAC, DeviceEventKind::Gone, at(10, 6)))
        .await
        .unwrap();
    // The same kind for a different MAC.
    repo.record(event(
        OTHER_DEVICE,
        OTHER_MAC,
        DeviceEventKind::IpChanged,
        at(10, 7),
    ))
    .await
    .unwrap();
    // The same kind for the same MAC, but before the window.
    repo.record(event(DEVICE, MAC, DeviceEventKind::IpChanged, at(7, 0)))
        .await
        .unwrap();

    let count = repo
        .count_for_mac_since(MAC, DeviceEventKind::IpChanged, at(9, 0))
        .await
        .unwrap();

    assert_eq!(count, 5);
}

#[tokio::test]
async fn pruning_deletes_events_older_than_the_age_cap() {
    let repo = repo().await;

    repo.record(event(DEVICE, MAC, DeviceEventKind::Gone, at(1, 0)))
        .await
        .unwrap();
    repo.record(event(DEVICE, MAC, DeviceEventKind::Returned, at(20, 0)))
        .await
        .unwrap();

    let deleted = repo.prune(at(10, 0), 5_000).await.unwrap();

    assert_eq!(deleted, 1);
    let remaining = repo
        .list_for_device(DEVICE, at(0, 0), at(23, 59))
        .await
        .unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].kind, DeviceEventKind::Returned);
}

/// The cap that bounds the pathological case: a device churning far faster than
/// the age cap can contain must not be able to grow without limit, and the rows
/// kept must be the newest ones.
#[tokio::test]
async fn pruning_keeps_only_the_newest_rows_per_device() {
    let repo = repo().await;

    for minute in 0..10 {
        repo.record(event(
            DEVICE,
            MAC,
            DeviceEventKind::IpChanged,
            at(10, minute),
        ))
        .await
        .unwrap();
    }
    // A second device must be capped independently, not collectively.
    for minute in 0..10 {
        repo.record(event(
            OTHER_DEVICE,
            OTHER_MAC,
            DeviceEventKind::IpChanged,
            at(10, minute),
        ))
        .await
        .unwrap();
    }

    let deleted = repo.prune(at(0, 0), 4).await.unwrap();

    assert_eq!(deleted, 12);
    let kept = repo
        .list_for_device(DEVICE, at(0, 0), at(23, 59))
        .await
        .unwrap();
    assert_eq!(kept.len(), 4);
    assert_eq!(kept.first().unwrap().created_at, at(10, 6));
    assert_eq!(kept.last().unwrap().created_at, at(10, 9));

    let other_kept = repo
        .list_for_device(OTHER_DEVICE, at(0, 0), at(23, 59))
        .await
        .unwrap();
    assert_eq!(other_kept.len(), 4);
}

/// A row written by a newer daemon must not take the timeline down after a
/// downgrade.
#[tokio::test]
async fn an_unknown_kind_is_skipped_rather_than_failing_the_query() {
    let repo = repo().await;
    let pool = test_pool().await;
    let repo_on_pool = SqliteDeviceEventRepository::new(pool.clone());
    drop(repo);

    repo_on_pool
        .record(event(DEVICE, MAC, DeviceEventKind::Gone, at(10, 0)))
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO device_events (device_id, mac, kind, details, created_at) \
         VALUES (?, ?, 'teleported', NULL, ?)",
    )
    .bind(DEVICE)
    .bind(MAC)
    .bind(at(10, 30).timestamp())
    .execute(&pool)
    .await
    .unwrap();

    let events = repo_on_pool
        .list_for_device(DEVICE, at(9, 0), at(11, 0))
        .await
        .unwrap();

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, DeviceEventKind::Gone);
}

#[tokio::test]
async fn retention_window_is_expressed_in_days() {
    // Guards the unit of the constant: a value read as seconds would prune
    // everything older than 30 seconds.
    let thirty_days = Duration::days(crate::repository::DEVICE_EVENT_RETENTION_DAYS);
    assert_eq!(thirty_days.num_days(), 30);
}
