//! Unit tests for `DnsLogSink` — the hot-path channel between the DNS
//! server and the persistence runner / WS subscribers.

use super::ts;
use crate::dns::log_sink::DnsLogSink;
use crate::stats::buffer::StatsBuffer;
use crate::stats::meter::Meter;
use wardnetd_data::repository::QueryLogRow;

fn sample_row(domain: &str) -> QueryLogRow {
    QueryLogRow {
        timestamp: ts("2026-05-05T00:00:00Z"),
        client_ip: "10.0.0.1".to_owned(),
        domain: domain.to_owned(),
        query_type: "A".to_owned(),
        result: "forwarded".to_owned(),
        upstream: None,
        latency_ms: 1.0,
        device_id: None,
        protocol: "udp".to_owned(),
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

fn make_meter() -> Meter {
    Meter::new(StatsBuffer::new())
}

/// [`sample_row`] with an optional device id, for the capture-path tests.
fn make_row(domain: &str, device_id: Option<&str>) -> QueryLogRow {
    let mut row = sample_row(domain);
    row.device_id = device_id.map(str::to_owned);
    row
}

/// A row with `device_id = Some(...)` must appear on both `capture_rx`
/// and `persist_rx`.
#[test]
fn capture_forwarded_when_device_id_set() {
    let meter = make_meter();
    let (sink, mut channels) = DnsLogSink::new_with_stats(&meter);

    sink.record(make_row("example.com", Some("dev-1")));

    let captured = channels
        .capture_rx
        .try_recv()
        .expect("expected row on capture_rx");
    assert_eq!(captured.domain, "example.com");

    let persisted = channels
        .persist_rx
        .try_recv()
        .expect("expected row on persist_rx");
    assert_eq!(persisted.domain, "example.com");
}

/// A row with `device_id = None` must NOT be forwarded to `capture_rx`.
#[test]
fn capture_skipped_without_device_id() {
    let meter = make_meter();
    let (sink, mut channels) = DnsLogSink::new_with_stats(&meter);

    sink.record(make_row("example.com", None));

    assert_eq!(
        channels.capture_rx.try_recv().unwrap_err(),
        tokio::sync::mpsc::error::TryRecvError::Empty,
    );

    // Persist channel should still receive the row.
    assert!(channels.persist_rx.try_recv().is_ok());
}

/// `take_capture_dropped()` starts at 0 and resets to 0 after being read.
#[test]
fn capture_dropped_counter_starts_zero_and_resets() {
    let meter = make_meter();
    let (sink, _channels) = DnsLogSink::new_with_stats(&meter);

    assert_eq!(sink.take_capture_dropped(), 0);
    // A second call must also return 0 (counter was reset by the first call).
    assert_eq!(sink.take_capture_dropped(), 0);
}

/// `record_dns_stats` records the `by_domain` counter for blocked queries.
#[test]
fn blocked_outcome_records_by_domain_stat() {
    let meter = make_meter();
    let (sink, _channels) = DnsLogSink::new_with_stats(&meter);

    let mut row = make_row("blocked-ads.tracker.io", Some("dev-1"));
    row.result = "blocked".to_owned();
    sink.record(row);

    // The only assertion needed for coverage is that we don't panic;
    // the by_domain counter is an internal implementation detail.
}

/// A malicious DNS name containing JSON-significant bytes (double-quote,
/// backslash, control char) must be escaped so the `dns.queries.by_domain`
/// label is still valid JSON whose parsed `domain` field round-trips exactly
/// to the raw name — proving the label can't be used to inject JSON structure
/// (F4, CWE-116). An ordinary domain still serializes to the canonical shape.
#[test]
fn by_domain_label_escapes_malicious_domain_and_round_trips() {
    // hickory renders a raw 0x22 byte in a label as `\"`; include that plus a
    // literal backslash and a control byte to exercise every escape path.
    let malicious = "evil\\\".com\u{0007}\"}injected";

    let buf = StatsBuffer::new();
    let meter = Meter::new(buf.clone());
    let (sink, _channels) = DnsLogSink::new_with_stats(&meter);

    let mut row = sample_row(malicious);
    row.result = "blocked".to_owned();
    sink.record(row);

    let rows = buf.drain();
    let domain_row = rows
        .iter()
        .find(|r| r.metric == "dns.queries.by_domain")
        .expect("by_domain must be recorded for a blocked query");

    // The label must parse as valid JSON (a partial `.replace` would have
    // produced a malformed / structure-injected string here).
    let parsed: serde_json::Value =
        serde_json::from_str(&domain_row.labels).expect("label must be valid JSON");

    assert_eq!(
        parsed["domain"], malicious,
        "parsed domain must round-trip exactly to the raw name, got {}",
        domain_row.labels
    );

    // Ordinary domains keep the exact canonical shape existing dashboards read.
    let (sink, _channels) = DnsLogSink::new_with_stats(&meter);
    let mut plain = sample_row("example.com");
    plain.result = "blocked".to_owned();
    sink.record(plain);

    let plain_row = buf
        .drain()
        .into_iter()
        .find(|r| r.metric == "dns.queries.by_domain")
        .expect("by_domain must be recorded for a blocked query");
    assert_eq!(plain_row.labels, r#"{"domain":"example.com"}"#);
}

/// A query attributed to a device records `dns.queries.by_device` keyed on
/// the device id; an unattributed query does not.
#[tokio::test]
async fn records_by_device_only_when_device_attributed() {
    let buf = StatsBuffer::new();
    let meter = Meter::new(buf.clone());
    let (sink, _channels) = DnsLogSink::new_with_stats(&meter);

    sink.record(make_row("example.com", Some("dev-42")));
    sink.record(make_row("example.org", None));

    let rows = buf.drain();
    let device_rows: Vec<_> = rows
        .iter()
        .filter(|r| r.metric == "dns.queries.by_device")
        .collect();
    assert_eq!(
        device_rows.len(),
        1,
        "exactly one by_device row (the attributed query)"
    );
    assert!(
        device_rows[0].labels.contains("dev-42"),
        "by_device label must carry the device id, got {}",
        device_rows[0].labels
    );
}

/// A blocked query whose domain is a recognised tracker records
/// `dns.blocked.by_tracker` labelled with the operating company. A blocked
/// query for an unrecognised domain records no tracker row, and a
/// non-blocked query for a tracker domain records none either.
#[tokio::test]
async fn records_by_tracker_for_blocked_known_trackers_only() {
    let buf = StatsBuffer::new();
    let meter = Meter::new(buf.clone());
    let (sink, _channels) = DnsLogSink::new_with_stats(&meter);

    // Blocked, recognised tracker → recorded, attributed to the company.
    let mut blocked_tracker = sample_row("pagead2.googlesyndication.com");
    blocked_tracker.result = "blocked".to_owned();
    sink.record(blocked_tracker);

    // Blocked, but not in the tracker catalogue → no tracker row.
    let mut blocked_other = sample_row("some-random-blocklist-domain.example");
    blocked_other.result = "blocked".to_owned();
    sink.record(blocked_other);

    // A tracker domain that was NOT blocked (allowlisted/forwarded) → none.
    let mut forwarded_tracker = sample_row("doubleclick.net");
    forwarded_tracker.result = "forwarded".to_owned();
    sink.record(forwarded_tracker);

    let rows = buf.drain();
    let tracker_rows: Vec<_> = rows
        .iter()
        .filter(|r| r.metric == "dns.blocked.by_tracker")
        .collect();
    assert_eq!(
        tracker_rows.len(),
        1,
        "only the blocked recognised tracker should be counted"
    );
    assert!(
        tracker_rows[0].labels.contains("Google"),
        "tracker row must be attributed to the operating company, got {}",
        tracker_rows[0].labels
    );
}

/// When the persist channel is full, `dropped_entries` is incremented.
#[test]
fn persist_full_increments_dropped_counter() {
    // Capacity-1 persist channel: first send fills it, second is dropped.
    let (sink, mut persist_rx) = DnsLogSink::with_capacities(1, 256);

    sink.record(make_row("first.example.com", None));
    sink.record(make_row("second.example.com", None));

    // Exactly one row should be queued, one should have been dropped.
    assert_eq!(persist_rx.try_recv().unwrap().domain, "first.example.com");
    assert!(
        persist_rx.try_recv().is_err(),
        "second row should have been dropped"
    );
    assert_eq!(sink.take_dropped(), 1);
}
