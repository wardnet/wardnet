#![allow(clippy::float_cmp)]

use crate::stats::buffer::StatsBuffer;

#[test]
fn record_counter_accumulates() {
    let buf = StatsBuffer::new();
    buf.record("m", "{}", 1.0, "counter");
    buf.record("m", "{}", 2.0, "counter");
    let rows = buf.drain();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].value, 3.0);
    assert_eq!(rows[0].kind, "counter");
}

#[test]
fn record_gauge_overwrites() {
    let buf = StatsBuffer::new();
    buf.record("m", "{}", 1.0, "gauge");
    buf.record("m", "{}", 5.0, "gauge");
    let rows = buf.drain();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].value, 5.0);
    assert_eq!(rows[0].kind, "gauge");
}

#[test]
fn drain_clears_buffer() {
    let buf = StatsBuffer::new();
    buf.record("m", "{}", 1.0, "counter");
    let first = buf.drain();
    assert_eq!(first.len(), 1);
    let second = buf.drain();
    assert!(second.is_empty());
}

#[test]
fn drain_empty_buffer_returns_empty_vec() {
    let buf = StatsBuffer::new();
    assert!(buf.drain().is_empty());
}

#[test]
fn different_label_sets_are_separate_entries() {
    let buf = StatsBuffer::new();
    buf.record("m", r#"{"outcome":"blocked"}"#, 1.0, "counter");
    buf.record("m", r#"{"outcome":"forwarded"}"#, 2.0, "counter");
    let rows = buf.drain();
    assert_eq!(rows.len(), 2);
}

#[test]
fn drain_row_carries_correct_fields() {
    let buf = StatsBuffer::new();
    buf.record("dns.queries", r#"{"outcome":"blocked"}"#, 3.0, "counter");
    let rows = buf.drain();
    assert_eq!(rows[0].metric, "dns.queries");
    assert_eq!(rows[0].labels, r#"{"outcome":"blocked"}"#);
    assert_eq!(rows[0].value, 3.0);
    assert_eq!(rows[0].kind, "counter");
}

#[test]
fn bucket_ts_is_minute_aligned() {
    let buf = StatsBuffer::new();
    buf.record("m", "{}", 1.0, "counter");
    let rows = buf.drain();
    assert_eq!(
        rows[0].bucket_ts % 60,
        0,
        "bucket_ts must be divisible by 60"
    );
}
