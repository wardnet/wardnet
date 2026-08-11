//! Host-independent tests for the route monitor's table predicate.
//!
//! The netlink multicast socket has no mockable boundary (it needs a real
//! kernel), but the decision that matters is pure: which `RTM_DELROUTE`
//! messages count as a lost *Wardnet* table. Getting that wrong in the
//! permissive direction republishes the kernel's own routing churn as a
//! Wardnet fault — a live Pi logged `detected route deletion in
//! wardnet-managed table table=254` for an ordinary change to `main`.

use std::sync::Mutex;

use rtnetlink::packet_route::route::{RouteAttribute, RouteHeader, RouteMessage};
use rtnetlink::packet_route::{AddressFamily, RouteNetlinkMessage};
use tokio::sync::broadcast;
use wardnet_common::event::WardnetEvent;
use wardnetd_services::event::EventPublisher;

use crate::route_monitor::handle_message;

/// The kernel's reserved tables. netlink-packet-route exports only `MAIN`, so
/// libc is the canonical source for the other three.
const RT_TABLE_COMPAT: u32 = libc::RT_TABLE_COMPAT as u32;
const RT_TABLE_DEFAULT: u32 = libc::RT_TABLE_DEFAULT as u32;
const RT_TABLE_MAIN: u32 = RouteHeader::RT_TABLE_MAIN as u32;
const RT_TABLE_LOCAL: u32 = libc::RT_TABLE_LOCAL as u32;

/// Records every published event so a test can assert on what the monitor
/// decided. The shipped `StubEventPublisher` drops them on the floor.
#[derive(Default)]
struct RecordingEventPublisher {
    events: Mutex<Vec<WardnetEvent>>,
}

impl RecordingEventPublisher {
    /// The tables this publisher was told were lost, in publish order.
    fn lost_tables(&self) -> Vec<u32> {
        self.events
            .lock()
            .expect("events mutex poisoned")
            .iter()
            .filter_map(|e| match e {
                WardnetEvent::RouteTableLost { table, .. } => Some(*table),
                _ => None,
            })
            .collect()
    }
}

impl EventPublisher for RecordingEventPublisher {
    fn publish(&self, event: WardnetEvent) {
        self.events
            .lock()
            .expect("events mutex poisoned")
            .push(event);
    }

    fn subscribe(&self) -> broadcast::Receiver<WardnetEvent> {
        let (tx, rx) = broadcast::channel(1);
        drop(tx);
        rx
    }
}

/// An `RTM_DELROUTE` for `table`. The number rides both `header.table` and the
/// `Table` attribute, mirroring what the kernel sends for a table that fits in
/// a `u8` — `route_table` prefers the attribute either way.
fn del_route(table: u32) -> RouteNetlinkMessage {
    let mut msg = RouteMessage::default();
    msg.header = RouteHeader {
        address_family: AddressFamily::Inet,
        table: u8::try_from(table).expect("test tables all fit in a u8"),
        ..RouteHeader::default()
    };
    msg.attributes = vec![RouteAttribute::Table(table)];
    RouteNetlinkMessage::DelRoute(msg)
}

/// Feed one deletion to the monitor and report the tables it declared lost.
fn lost_tables_for(table: u32) -> Vec<u32> {
    let events = RecordingEventPublisher::default();
    handle_message(del_route(table), &events);
    events.lost_tables()
}

#[test]
fn kernel_main_table_deletion_is_ignored() {
    // Wardnet *does* file routes in `main` — `add_host_route` sets no table id,
    // so its /32 device host routes land there. They are still not what this
    // event describes: `handle_route_table_lost` only re-applies rules whose
    // `table == Some(254)`, which no device rule ever has, so a `RouteTableLost`
    // for `main` can recover nothing and only raises a spurious diagnostic.
    assert!(
        lost_tables_for(RT_TABLE_MAIN).is_empty(),
        "a deletion in the kernel's main table (254) must not be reported as a lost \
         Wardnet table — the event is per-table and carries no route, so it cannot \
         describe the loss of a host route filed there"
    );
}

#[test]
fn kernel_reserved_table_deletions_are_ignored() {
    // The kernel reserves four tables, not three: `compat` (252) as well as
    // `default`, `main` and `local`. 252 matters twice over — it is also the
    // value the kernel stamps into the u8 `header.table` for any table id too
    // wide to fit, so a deletion in someone else's table 1000 that arrives
    // without an RTA_TABLE attribute resolves to 252.
    for table in [RT_TABLE_COMPAT, RT_TABLE_DEFAULT, RT_TABLE_LOCAL] {
        assert!(
            lost_tables_for(table).is_empty(),
            "table {table} is kernel-reserved and must not be reported as a lost Wardnet table"
        );
    }
}

#[test]
fn wardnet_table_deletion_is_reported() {
    // `table_for_index` maps tunnel index 0 -> 100 and index 2 -> 102; a live
    // Pi carries `lookup 102`. These are the deletions the monitor exists for.
    for table in [100, 102] {
        assert_eq!(
            lost_tables_for(table),
            vec![table],
            "table {table} is Wardnet-managed and its loss must be published"
        );
    }
}

#[test]
fn wardnet_table_range_boundaries_are_respected() {
    assert_eq!(
        lost_tables_for(251),
        vec![251],
        "251 is the last table below the kernel-reserved block and is still ours"
    );
    assert!(
        lost_tables_for(99).is_empty(),
        "99 is below the Wardnet range and belongs to whoever else is using it"
    );
}

#[test]
fn non_deletion_messages_are_ignored() {
    let events = RecordingEventPublisher::default();
    let RouteNetlinkMessage::DelRoute(route) = del_route(102) else {
        unreachable!("del_route always builds a DelRoute")
    };

    handle_message(RouteNetlinkMessage::NewRoute(route), &events);

    assert!(
        events.lost_tables().is_empty(),
        "a route being added is not a route being lost"
    );
}
