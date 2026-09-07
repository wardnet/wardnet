//! The bus-to-timeline mapping, exercised without a bus or a database.

use chrono::{TimeZone, Utc};
use uuid::Uuid;
use wardnet_common::device_event::DeviceEventKind;
use wardnet_common::event::WardnetEvent;

use super::{IP, MAC};
use crate::device_event::listener::intent_from_event;
use crate::device_event::service::{DeviceRef, IntentKind};

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
