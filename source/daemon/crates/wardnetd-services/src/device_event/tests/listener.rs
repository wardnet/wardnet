//! The bus-to-timeline mapping, exercised without a bus or a database.

use chrono::{TimeZone, Utc};
use uuid::Uuid;
use wardnet_common::device_event::DeviceEventKind;
use wardnet_common::event::WardnetEvent;

use super::{IP, MAC};
use crate::device_event::listener::intent_from_event;
use crate::device_event::service::{DeviceRef, IntentKind};
use chrono::Utc;

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 6, 10, 0, 0).unwrap()
}

/// An arrival is deliberately left undecided here: the bus publishes
/// `DeviceDiscovered` for both a first sighting and a return, so only the
/// recorded history can tell them apart.
#[test]
fn a_discovery_maps_to_an_undecided_arrival() {
    let device_id = Uuid::new_v4();
    let intent = intent_from_event(&WardnetEvent::DeviceDiscovered {
        device_id,
        mac: MAC.to_owned(),
        ip: IP.to_owned(),
        hostname: None,
        timestamp: now(),
    })
    .unwrap();

    assert_eq!(intent.device, DeviceRef::Id(device_id));
    assert_eq!(intent.mac.as_deref(), Some(MAC));
    assert_eq!(intent.kind, IntentKind::Arrival);
    assert_eq!(intent.at, now());
}

#[test]
fn an_ip_change_records_both_addresses() {
    let intent = intent_from_event(&WardnetEvent::DeviceIpChanged {
        device_id: Uuid::new_v4(),
        mac: MAC.to_owned(),
        old_ip: "192.168.100.23".to_owned(),
        new_ip: "192.168.100.41".to_owned(),
        timestamp: now(),
    })
    .unwrap();

    assert_eq!(intent.kind, IntentKind::Fixed(DeviceEventKind::IpChanged));
    let details = intent.details.unwrap();
    assert_eq!(details["old_ip"], "192.168.100.23");
    assert_eq!(details["new_ip"], "192.168.100.41");
}

#[test]
fn a_departure_maps_to_gone() {
    let intent = intent_from_event(&WardnetEvent::DeviceGone {
        device_id: Uuid::new_v4(),
        mac: MAC.to_owned(),
        last_ip: IP.to_owned(),
        timestamp: now(),
    })
    .unwrap();

    assert_eq!(intent.kind, IntentKind::Fixed(DeviceEventKind::Gone));
}

/// The zone event carries no mac, so the intent must leave it for the service
/// rather than inventing one.
#[test]
fn a_zone_change_defers_mac_resolution() {
    let intent = intent_from_event(&WardnetEvent::DeviceZoneChanged {
        device_id: Uuid::new_v4(),
        old_zone_id: Uuid::new_v4(),
        new_zone_id: Uuid::new_v4(),
        timestamp: now(),
    })
    .unwrap();

    assert_eq!(intent.kind, IntentKind::Fixed(DeviceEventKind::ZoneChanged));
    assert!(intent.mac.is_none());
}

/// A flush is keyed by address because that is what a firewall rule is keyed
/// by; the service resolves it to a device.
#[test]
fn a_conntrack_flush_is_keyed_by_address() {
    let intent = intent_from_event(&WardnetEvent::DeviceConntrackFlushed {
        device_ip: IP.to_owned(),
        reason: "zone rules applied".to_owned(),
        timestamp: now(),
    })
    .unwrap();

    assert_eq!(intent.device, DeviceRef::Ip(IP.to_owned()));
    assert_eq!(
        intent.kind,
        IntentKind::Fixed(DeviceEventKind::ConntrackFlushed)
    );
    assert_eq!(intent.details.unwrap()["reason"], "zone rules applied");
}

/// The timeline is observational, not an audit of the whole bus.
#[test]
fn unrelated_events_are_not_recorded() {
    assert!(intent_from_event(&WardnetEvent::ZoneExceptionsChanged { timestamp: now() }).is_none());
    assert!(
        intent_from_event(&WardnetEvent::DeviceAdminLocked {
            device_id: Uuid::new_v4(),
            locked: true,
            timestamp: now(),
        })
        .is_none()
    );
}

// ---------------------------------------------------------------------------
// The listener end to end: bus in, timeline row out
// ---------------------------------------------------------------------------

use std::sync::Arc;

use super::{as_admin, service_with_device};
use crate::device_event::listener::DeviceEventListener;
use crate::event::{BroadcastEventBus, EventPublisher};

fn window() -> (chrono::DateTime<Utc>, chrono::DateTime<Utc>) {
    (
        now() - chrono::Duration::hours(1),
        now() + chrono::Duration::hours(1),
    )
}

/// Poll until the listener has recorded `expected` events, or give up.
///
/// The listener runs on its own task, so a bare assert would race it.
async fn wait_for_events(
    service: &Arc<dyn crate::device_event::DeviceEventService>,
    device_id: Uuid,
    expected: usize,
) -> Vec<wardnet_common::device_event::DeviceEvent> {
    let (from, to) = window();
    for _ in 0..100 {
        let events = as_admin(service.list_for_device(device_id, from, to))
            .await
            .unwrap();
        if events.len() >= expected {
            return events;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    as_admin(service.list_for_device(device_id, from, to))
        .await
        .unwrap()
}

#[tokio::test]
async fn an_event_on_the_bus_becomes_a_timeline_row() {
    let device_id = Uuid::new_v4();
    let service = service_with_device(device_id).await;
    let events: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(64));
    let listener = DeviceEventListener::start(&events, service.clone(), &tracing::Span::current());

    events.publish(WardnetEvent::DeviceIpChanged {
        device_id,
        mac: MAC.to_owned(),
        old_ip: "192.168.100.23".to_owned(),
        new_ip: "192.168.100.41".to_owned(),
        timestamp: now(),
    });

    let recorded = wait_for_events(&service, device_id, 1).await;
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].kind, DeviceEventKind::IpChanged);

    listener.shutdown().await;
}

/// The classification the bus cannot make: an arrival after a departure is a
/// return, decided from what the listener has already recorded.
#[tokio::test]
async fn the_listener_classifies_an_arrival_from_recorded_history() {
    let device_id = Uuid::new_v4();
    let service = service_with_device(device_id).await;
    let events: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(64));
    let listener = DeviceEventListener::start(&events, service.clone(), &tracing::Span::current());

    events.publish(WardnetEvent::DeviceDiscovered {
        device_id,
        mac: MAC.to_owned(),
        ip: IP.to_owned(),
        hostname: None,
        timestamp: now(),
    });
    wait_for_events(&service, device_id, 1).await;

    events.publish(WardnetEvent::DeviceGone {
        device_id,
        mac: MAC.to_owned(),
        last_ip: IP.to_owned(),
        timestamp: now(),
    });
    wait_for_events(&service, device_id, 2).await;

    events.publish(WardnetEvent::DeviceDiscovered {
        device_id,
        mac: MAC.to_owned(),
        ip: IP.to_owned(),
        hostname: None,
        timestamp: now(),
    });
    let recorded = wait_for_events(&service, device_id, 3).await;

    let kinds: Vec<_> = recorded.iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        vec![
            DeviceEventKind::Discovered,
            DeviceEventKind::Gone,
            DeviceEventKind::Returned,
        ]
    );

    listener.shutdown().await;
}

/// The timeline is observational, so an event it does not recognise must leave
/// no trace rather than becoming a mystery row.
#[tokio::test]
async fn an_unrelated_event_records_nothing() {
    let device_id = Uuid::new_v4();
    let service = service_with_device(device_id).await;
    let events: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(64));
    let listener = DeviceEventListener::start(&events, service.clone(), &tracing::Span::current());

    events.publish(WardnetEvent::ZoneExceptionsChanged { timestamp: now() });
    events.publish(WardnetEvent::DeviceIpChanged {
        device_id,
        mac: MAC.to_owned(),
        old_ip: "192.168.100.23".to_owned(),
        new_ip: "192.168.100.41".to_owned(),
        timestamp: now(),
    });

    // The recognised event is the fence: once it lands, the ignored one has
    // certainly been through the loop too.
    let recorded = wait_for_events(&service, device_id, 1).await;
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].kind, DeviceEventKind::IpChanged);

    listener.shutdown().await;
}

/// A conntrack flush is keyed by address; the listener resolves it to whichever
/// device holds that address.
#[tokio::test]
async fn a_flush_is_resolved_to_the_device_holding_the_address() {
    let device_id = Uuid::new_v4();
    let service = service_with_device(device_id).await;
    let events: Arc<dyn EventPublisher> = Arc::new(BroadcastEventBus::new(64));
    let listener = DeviceEventListener::start(&events, service.clone(), &tracing::Span::current());

    events.publish(WardnetEvent::DeviceConntrackFlushed {
        device_ip: IP.to_owned(),
        reason: "zone rules applied".to_owned(),
        timestamp: now(),
    });

    let recorded = wait_for_events(&service, device_id, 1).await;
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].kind, DeviceEventKind::ConntrackFlushed);
    assert_eq!(recorded[0].mac, MAC);

    listener.shutdown().await;
}
