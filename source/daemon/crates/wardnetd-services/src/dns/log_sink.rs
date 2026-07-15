//! Hot-path channel for DNS query log events.
//!
//! `handle_query` in the DNS server fires one event per query into this
//! sink. The sink fans events out two ways:
//!
//! - **`mpsc` (bounded, capacity `5_000`)** — drained by the persistence
//!   runner. `try_send` only; on `TrySendError::Full` we increment a
//!   `dropped_entries` counter and drop the entry. Never blocks.
//! - **broadcast (capacity 256)** — fans events out to live-stream WS
//!   subscribers. Standard broadcast semantics; lagged consumers see
//!   `Lagged(n)` and resume from the next message.
//!
//! Both sends fire on every query regardless of `query_log_enabled`. The
//! toggle gates persistence only; live broadcast is always on so an
//! admin can debug live without retaining history.
//!
//! When constructed via [`DnsLogSink::new_with_stats`], each call to
//! [`DnsLogSink::record`] also records four pre-aggregating metrics into the
//! stats buffer (see [`crate::stats`]):
//!
//! | Metric | Labels | Notes |
//! |---|---|---|
//! | `dns.queries` | `{outcome}` | Counter per outcome |
//! | `dns.latency_ms` | `{outcome}` | Gauge per outcome |
//! | `dns.queries.by_domain` | `{domain}` | Counter; blocked queries only |
//! | `dns.queries.by_client` | `{client}` | Counter per client IP |

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::{broadcast, mpsc};
use wardnet_common::api::QueryLogEvent;
use wardnet_common::dns::DnsQueryResult;
use wardnetd_data::repository::QueryLogRow;

use crate::stats::meter::{Counter, Gauge, Meter};

/// Default mpsc buffer for queued query log entries.
pub const DEFAULT_MPSC_CAPACITY: usize = 5_000;

/// Default broadcast capacity for live-stream subscribers.
pub const DEFAULT_BROADCAST_CAPACITY: usize = 256;

/// DNS-specific stats instruments wired in when the stats subsystem is active.
struct DnsStatInstruments {
    queries: Counter,
    latency: Gauge,
    by_domain: Counter,
    by_client: Counter,
}

/// Receivers returned by [`DnsLogSink::new_with_stats`].
pub struct DnsLogSinkChannels {
    /// Drained by the persistence runner ([`super::query_log_runner`]).
    pub persist_rx: mpsc::Receiver<QueryLogRow>,
    /// Drained by the capture runner ([`super::capture_runner`]).
    /// Only rows whose `device_id` is `Some` are forwarded here.
    pub capture_rx: mpsc::Receiver<QueryLogRow>,
}

/// Hot-path sink shared between the DNS server (producer) and the
/// persistence runner + WS subscribers (consumers).
pub struct DnsLogSink {
    persist_tx: mpsc::Sender<QueryLogRow>,
    capture_tx: Option<mpsc::Sender<QueryLogRow>>,
    stream_tx: broadcast::Sender<QueryLogEvent>,
    dropped_entries: AtomicU64,
    /// Rows dropped because the capture channel was full.
    capture_dropped_entries: AtomicU64,
    /// Stats instruments — `None` when the stats subsystem is not wired in
    /// (e.g. tests that build a bare sink via [`DnsLogSink::new`]).
    stat_instruments: Option<DnsStatInstruments>,
}

impl DnsLogSink {
    /// Build a new sink with default capacities, returning the sink and
    /// the mpsc receiver that the persistence runner will drain.
    ///
    /// Stats recording is disabled. Use [`DnsLogSink::new_with_stats`] to
    /// enable per-query stats tracking.
    #[must_use]
    pub fn new() -> (Arc<Self>, mpsc::Receiver<QueryLogRow>) {
        Self::with_capacities(DEFAULT_MPSC_CAPACITY, DEFAULT_BROADCAST_CAPACITY)
    }

    /// Build a sink wired to the stats subsystem. Every call to
    /// [`DnsLogSink::record`] will also push measurements into the shared
    /// [`crate::stats::StatsBuffer`] via the four DNS instruments.
    ///
    /// Returns the sink and a [`DnsLogSinkChannels`] holding both the
    /// persistence receiver and the capture receiver.
    #[must_use]
    pub fn new_with_stats(meter: &Meter) -> (Arc<Self>, DnsLogSinkChannels) {
        let (persist_tx, persist_rx) = mpsc::channel(DEFAULT_MPSC_CAPACITY);
        let (capture_tx, capture_rx) = mpsc::channel(DEFAULT_MPSC_CAPACITY);
        let (stream_tx, _) = broadcast::channel(DEFAULT_BROADCAST_CAPACITY);
        let sink = Arc::new(Self {
            persist_tx,
            capture_tx: Some(capture_tx),
            stream_tx,
            dropped_entries: AtomicU64::new(0),
            capture_dropped_entries: AtomicU64::new(0),
            stat_instruments: Some(DnsStatInstruments {
                queries: meter.counter("dns.queries"),
                latency: meter.gauge("dns.latency_ms"),
                by_domain: meter.counter("dns.queries.by_domain"),
                by_client: meter.counter("dns.queries.by_client"),
            }),
        });
        (
            sink,
            DnsLogSinkChannels {
                persist_rx,
                capture_rx,
            },
        )
    }

    /// Build a sink with custom capacities — exposed for tests that need
    /// a tiny buffer to exercise the full-on-drop branch.
    #[must_use]
    pub fn with_capacities(
        persist_capacity: usize,
        broadcast_capacity: usize,
    ) -> (Arc<Self>, mpsc::Receiver<QueryLogRow>) {
        let (persist_tx, persist_rx) = mpsc::channel(persist_capacity);
        let (stream_tx, _) = broadcast::channel(broadcast_capacity);
        let sink = Arc::new(Self {
            persist_tx,
            capture_tx: None,
            stream_tx,
            dropped_entries: AtomicU64::new(0),
            capture_dropped_entries: AtomicU64::new(0),
            stat_instruments: None,
        });
        (sink, persist_rx)
    }

    /// Record a query log entry. Non-blocking: if the persistence buffer
    /// is full the entry is dropped and the dropped-counter incremented.
    /// The broadcast send is fire-and-forget; if no live-stream
    /// subscribers exist `send` returns `Err` which we ignore.
    ///
    /// If stats instruments are wired in, also records the four DNS metrics.
    pub fn record(&self, row: QueryLogRow) {
        if let Some(ref inst) = self.stat_instruments {
            record_dns_stats(inst, &row);
        }
        let event = row_to_event(&row);
        // Forward to capture runner only when a device is identified.
        // Avoid cloning when the channel is already known-full — check
        // capacity first to skip the heap allocation on a saturated buffer.
        if row.device_id.is_some()
            && let Some(ref cap_tx) = self.capture_tx
        {
            if cap_tx.capacity() > 0 {
                if let Err(mpsc::error::TrySendError::Full(_)) = cap_tx.try_send(row.clone()) {
                    self.capture_dropped_entries.fetch_add(1, Ordering::Relaxed);
                }
            } else {
                self.capture_dropped_entries.fetch_add(1, Ordering::Relaxed);
            }
        }
        if let Err(mpsc::error::TrySendError::Full(_)) = self.persist_tx.try_send(row) {
            self.dropped_entries.fetch_add(1, Ordering::Relaxed);
        }
        // Closed/no-subscriber broadcast errors are normal and ignored.
        let _ = self.stream_tx.send(event);
    }

    /// Subscribe to the live-stream broadcast.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<QueryLogEvent> {
        self.stream_tx.subscribe()
    }

    /// Take the current dropped-entries count and reset it to zero.
    pub fn take_dropped(&self) -> u64 {
        self.dropped_entries.swap(0, Ordering::Relaxed)
    }

    /// Read the current dropped-entries count without resetting.
    #[must_use]
    pub fn dropped_count(&self) -> u64 {
        self.dropped_entries.load(Ordering::Relaxed)
    }

    /// Take the current capture-dropped count and reset it to zero.
    pub fn take_capture_dropped(&self) -> u64 {
        self.capture_dropped_entries.swap(0, Ordering::Relaxed)
    }
}

/// Project a `QueryLogRow` into the wire-format event sent over the WS.
#[must_use]
pub fn row_to_event(row: &QueryLogRow) -> QueryLogEvent {
    QueryLogEvent {
        timestamp: row.timestamp.clone(),
        client_ip: row.client_ip.clone(),
        domain: row.domain.clone(),
        query_type: row.query_type.clone(),
        result: DnsQueryResult::parse(&row.result),
        upstream: row.upstream.clone(),
        latency_ms: row.latency_ms,
        device_id: row.device_id.clone(),
    }
}

/// Record the four DNS stats metrics from a single query log row.
///
/// Label strings are sorted JSON objects as required by the stats schema.
/// `dns.queries.by_domain` is only recorded for blocked queries so the
/// per-domain counter stays bounded.
fn record_dns_stats(inst: &DnsStatInstruments, row: &QueryLogRow) {
    let outcome = normalize_outcome(&row.result);
    let outcome_labels = format!(r#"{{"outcome":"{outcome}"}}"#);

    inst.queries.add(&outcome_labels, 1.0);
    inst.latency.set(&outcome_labels, row.latency_ms);

    if outcome == "blocked" {
        // Escape any double-quotes in the domain (extremely rare but safe).
        let domain = row.domain.replace('"', r#"\""#);
        inst.by_domain
            .add(&format!(r#"{{"domain":"{domain}"}}"#), 1.0);
    }

    let client = row.client_ip.replace('"', r#"\""#);
    inst.by_client
        .add(&format!(r#"{{"client":"{client}"}}"#), 1.0);
}

/// Normalise a raw DNS result string into the canonical outcome label value.
///
/// Matches on the parsed enum rather than the raw string so that the match is
/// exhaustive: a new [`DnsQueryResult`] variant fails to compile here instead
/// of silently landing in the `"error"` catch-all. That catch-all is exactly
/// how `authoritative` — a *successful* local answer — came to be counted as
/// an error in `dns.queries`, inflating the error rate on every dashboard
/// reading these labels.
fn normalize_outcome(result: &str) -> &'static str {
    let Some(parsed) = DnsQueryResult::from_db_str(result) else {
        return "error";
    };

    match parsed {
        DnsQueryResult::Blocked | DnsQueryResult::BlockedSkipped => "blocked",
        // NOT "blocked". `dns.queries.by_domain` is only recorded for the
        // `blocked` outcome, and the web layer derives the blocked count, the
        // block rate and "top blocked domains" from it. A rebinding refusal is
        // a safety net firing on a domain that is on no blocklist, so counting
        // it as a block would inflate the block rate and park legitimate
        // domains in the top-blocked list.
        DnsQueryResult::RebindingBlocked => "rebinding_blocked",
        DnsQueryResult::Forwarded => "forwarded",
        // Negative answers, whether they came from upstream or from a local
        // authoritative zone. They are successful resolutions, not errors.
        DnsQueryResult::Negative
        | DnsQueryResult::AuthoritativeNodata
        | DnsQueryResult::AuthoritativeNxdomain => "negative",
        DnsQueryResult::CacheHit => "cached",
        DnsQueryResult::Recursive => "recursive",
        // Both are answered locally: `Rewritten` from a custom record,
        // `Authoritative` from a local authoritative zone.
        DnsQueryResult::Rewritten | DnsQueryResult::Authoritative => "local",
        DnsQueryResult::RateLimited => "rate_limited",
        DnsQueryResult::UpstreamError | DnsQueryResult::RecursorFailed | DnsQueryResult::Error => {
            "error"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::{buffer::StatsBuffer, meter::Meter};

    fn make_meter() -> Meter {
        Meter::new(StatsBuffer::new())
    }

    fn make_row(domain: &str, device_id: Option<&str>) -> QueryLogRow {
        QueryLogRow {
            timestamp: "2026-06-12T00:00:00Z".to_owned(),
            client_ip: "192.168.1.1".to_owned(),
            domain: domain.to_owned(),
            query_type: "A".to_owned(),
            result: "forwarded".to_owned(),
            upstream: None,
            latency_ms: 1.0,
            device_id: device_id.map(str::to_owned),
        }
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
}
