//! Unit tests for `DnsLogSink` — the hot-path channel between the DNS
//! server and the persistence runner / WS subscribers.

use crate::dns::log_sink::DnsLogSink;
use wardnetd_data::repository::QueryLogRow;

fn sample_row(domain: &str) -> QueryLogRow {
    QueryLogRow {
        timestamp: "2026-05-05T00:00:00Z".to_owned(),
        client_ip: "10.0.0.1".to_owned(),
        domain: domain.to_owned(),
        query_type: "A".to_owned(),
        result: "forwarded".to_owned(),
        upstream: None,
        latency_ms: 1.0,
        device_id: None,
    }
}

#[tokio::test]
async fn record_pushes_to_persist_and_broadcast() {
    let (sink, mut rx) = DnsLogSink::new();
    let mut sub = sink.subscribe();
    sink.record(sample_row("example.com"));

    let row = rx.recv().await.expect("persist channel delivered row");
    assert_eq!(row.domain, "example.com");

    let event = sub.recv().await.expect("broadcast delivered event");
    assert_eq!(event.domain, "example.com");

    assert_eq!(sink.dropped_count(), 0);
}

#[tokio::test]
async fn record_increments_dropped_when_persist_full() {
    // Capacity 1 so the second record fills the channel.
    let (sink, _rx) = DnsLogSink::with_capacities(1, 16);
    sink.record(sample_row("a.com"));
    sink.record(sample_row("b.com"));
    sink.record(sample_row("c.com"));

    assert_eq!(sink.dropped_count(), 2);
}

#[tokio::test]
async fn take_dropped_resets_counter() {
    let (sink, _rx) = DnsLogSink::with_capacities(1, 16);
    sink.record(sample_row("a.com"));
    sink.record(sample_row("b.com"));
    assert_eq!(sink.dropped_count(), 1);
    let taken = sink.take_dropped();
    assert_eq!(taken, 1);
    assert_eq!(sink.dropped_count(), 0);
}

#[tokio::test]
async fn broadcast_send_is_fire_and_forget_with_no_subscribers() {
    let (sink, mut rx) = DnsLogSink::new();
    // Nobody calls `sink.subscribe()` — broadcast send returns Err but
    // record() must swallow it and still push to persist.
    sink.record(sample_row("nosub.com"));
    let row = rx.recv().await.expect("persist channel still works");
    assert_eq!(row.domain, "nosub.com");
}
