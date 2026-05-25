#![allow(clippy::float_cmp)]

use crate::stats::buffer::StatsBuffer;
use crate::stats::meter::Meter;

#[test]
fn counter_add_accumulates_in_buffer() {
    let buf = StatsBuffer::new();
    let meter = Meter::new(buf.clone());
    let counter = meter.counter("hits");
    counter.add("{}", 1.0);
    counter.add("{}", 2.0);
    let rows = buf.drain();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].value, 3.0);
    assert_eq!(rows[0].kind, "counter");
    assert_eq!(rows[0].metric, "hits");
}

#[test]
fn gauge_set_overwrites_in_buffer() {
    let buf = StatsBuffer::new();
    let meter = Meter::new(buf.clone());
    let gauge = meter.gauge("latency");
    gauge.set("{}", 100.0);
    gauge.set("{}", 50.0);
    let rows = buf.drain();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].value, 50.0);
    assert_eq!(rows[0].kind, "gauge");
    assert_eq!(rows[0].metric, "latency");
}

#[test]
fn meter_buffer_returns_shared_buffer_arc() {
    let buf = StatsBuffer::new();
    let meter = Meter::new(buf.clone());
    let returned = meter.buffer();
    buf.record("x", "{}", 1.0, "counter");
    let rows = returned.drain();
    assert_eq!(
        rows.len(),
        1,
        "buffer() must return the same Arc as the one passed to Meter::new"
    );
}
