//! Unit tests for `DnsLogSink` — the hot-path channel between the DNS
//! server and the persistence runner / WS subscribers.

use crate::dns::log_sink::DnsLogSink;
use crate::stats::buffer::StatsBuffer;
use crate::stats::meter::Meter;
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

#[tokio::test]
async fn new_with_stats_records_dns_query_metrics() {
    let buf = StatsBuffer::new();
    let meter = Meter::new(buf.clone());
    let (sink, _rx) = DnsLogSink::new_with_stats(&meter);

    // A forwarded query: dns.queries and dns.latency_ms are recorded;
    // dns.queries.by_domain is NOT (only for blocked).
    let mut row = sample_row("example.com");
    row.result = "forwarded".to_owned();
    row.latency_ms = 5.0;
    sink.record(row);

    let rows = buf.drain();
    let metrics: Vec<&str> = rows.iter().map(|r| r.metric.as_str()).collect();
    assert!(
        metrics.contains(&"dns.queries"),
        "dns.queries must be recorded"
    );
    assert!(
        metrics.contains(&"dns.latency_ms"),
        "dns.latency_ms must be recorded"
    );
    assert!(
        metrics.contains(&"dns.queries.by_client"),
        "dns.queries.by_client must be recorded"
    );
    assert!(
        !metrics.contains(&"dns.queries.by_domain"),
        "dns.queries.by_domain must only be recorded for blocked queries"
    );
}

#[tokio::test]
async fn new_with_stats_records_by_domain_only_for_blocked() {
    let buf = StatsBuffer::new();
    let meter = Meter::new(buf.clone());
    let (sink, _rx) = DnsLogSink::new_with_stats(&meter);

    let mut row = sample_row("ads.tracker.com");
    row.result = "blocked".to_owned();
    sink.record(row);

    let rows = buf.drain();
    let domain_row = rows.iter().find(|r| r.metric == "dns.queries.by_domain");
    assert!(
        domain_row.is_some(),
        "dns.queries.by_domain must be recorded for blocked queries"
    );
    assert!(
        domain_row.unwrap().labels.contains("ads.tracker.com"),
        "domain label must include the blocked domain"
    );
}
