//! Resolution the pure mapping cannot do: whether an arrival is a return, and
//! the pieces the bus did not carry.

use chrono::{TimeZone, Utc};
use uuid::Uuid;
use wardnet_common::device_event::DeviceEventKind;

use super::{IP, MAC, as_admin, service_with_device};
use crate::device_event::service::{DeviceEventIntent, DeviceRef, IntentKind};

fn at(hour: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 6, hour, 0, 0).unwrap()
}

fn intent(device: DeviceRef, kind: IntentKind, hour: u32) -> DeviceEventIntent {
    DeviceEventIntent {
        device,
        mac: None,
        kind,
        details: None,
        at: at(hour),
    }
}

#[tokio::test]
async fn a_first_arrival_is_recorded_as_discovered() {
    let id = Uuid::new_v4();
    let service = service_with_device(id).await;

    as_admin(service.record(intent(DeviceRef::Id(id), IntentKind::Arrival, 10)))
        .await
        .unwrap();

    let events = as_admin(service.list_for_device(id, at(9), at(11)))
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, DeviceEventKind::Discovered);
}

/// The distinction the bus throws away: an arrival that follows a departure is
/// a return, and the worked example on the timeline turns on being able to say
/// "it never departed".
#[tokio::test]
async fn an_arrival_after_a_departure_is_recorded_as_returned() {
    let id = Uuid::new_v4();
    let service = service_with_device(id).await;

    as_admin(service.record(intent(DeviceRef::Id(id), IntentKind::Arrival, 8)))
        .await
        .unwrap();
    as_admin(service.record(intent(
        DeviceRef::Id(id),
        IntentKind::Fixed(DeviceEventKind::Gone),
        9,
    )))
    .await
    .unwrap();
    as_admin(service.record(intent(DeviceRef::Id(id), IntentKind::Arrival, 10)))
        .await
        .unwrap();

    let events = as_admin(service.list_for_device(id, at(7), at(11)))
        .await
        .unwrap();
    let kinds: Vec<_> = events.iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        vec![
            DeviceEventKind::Discovered,
            DeviceEventKind::Gone,
            DeviceEventKind::Returned,
        ]
    );
}

/// A device seen repeatedly without departing must not accumulate "returned"
/// entries — that would make a stable device look like it was flapping.
#[tokio::test]
async fn a_repeat_arrival_without_a_departure_is_still_discovered() {
    let id = Uuid::new_v4();
    let service = service_with_device(id).await;

    as_admin(service.record(intent(DeviceRef::Id(id), IntentKind::Arrival, 8)))
        .await
        .unwrap();
    as_admin(service.record(intent(DeviceRef::Id(id), IntentKind::Arrival, 10)))
        .await
        .unwrap();

    let events = as_admin(service.list_for_device(id, at(7), at(11)))
        .await
        .unwrap();
    assert!(events.iter().all(|e| e.kind == DeviceEventKind::Discovered));
}

#[tokio::test]
async fn a_mac_the_event_omitted_is_resolved_from_the_device() {
    let id = Uuid::new_v4();
    let service = service_with_device(id).await;

    as_admin(service.record(intent(
        DeviceRef::Id(id),
        IntentKind::Fixed(DeviceEventKind::ZoneChanged),
        10,
    )))
    .await
    .unwrap();

    let events = as_admin(service.list_for_device(id, at(9), at(11)))
        .await
        .unwrap();
    assert_eq!(events[0].mac, MAC);
}

#[tokio::test]
async fn a_flush_is_attributed_to_the_device_holding_that_address() {
    let id = Uuid::new_v4();
    let service = service_with_device(id).await;

    as_admin(service.record(intent(
        DeviceRef::Ip(IP.to_owned()),
        IntentKind::Fixed(DeviceEventKind::ConntrackFlushed),
        10,
    )))
    .await
    .unwrap();

    let events = as_admin(service.list_for_device(id, at(9), at(11)))
        .await
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, DeviceEventKind::ConntrackFlushed);
    assert_eq!(events[0].mac, MAC);
}

/// A flush can race a device being pruned. Dropping the row is preferable to a
/// listener that logs an error every time.
#[tokio::test]
async fn an_event_for_an_unknown_device_is_dropped_rather_than_failing() {
    let id = Uuid::new_v4();
    let service = service_with_device(id).await;

    let result = as_admin(service.record(intent(
        DeviceRef::Ip("10.0.0.99".to_owned()),
        IntentKind::Fixed(DeviceEventKind::ConntrackFlushed),
        10,
    )))
    .await;

    assert!(result.is_ok());
    let events = as_admin(service.list_for_device(id, at(9), at(11)))
        .await
        .unwrap();
    assert!(events.is_empty());
}

#[tokio::test]
async fn recording_requires_an_admin_context() {
    let id = Uuid::new_v4();
    let service = service_with_device(id).await;

    let result = service
        .record(intent(DeviceRef::Id(id), IntentKind::Arrival, 10))
        .await;

    assert!(
        result.is_err(),
        "an unauthenticated caller must be rejected"
    );
}

#[tokio::test]
async fn counting_a_macs_events_feeds_the_churn_detector() {
    let id = Uuid::new_v4();
    let service = service_with_device(id).await;

    for hour in 8..12 {
        as_admin(service.record(intent(
            DeviceRef::Id(id),
            IntentKind::Fixed(DeviceEventKind::IpChanged),
            hour,
        )))
        .await
        .unwrap();
    }

    let count = as_admin(service.count_for_mac_since(MAC, DeviceEventKind::IpChanged, at(7)))
        .await
        .unwrap();

    assert_eq!(count, 4);
}
