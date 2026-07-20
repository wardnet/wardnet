//! Hot-path channel for DNS query log events.
//!
//! The DNS server's query pipeline fires one event per query into this
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
//! | `dns.queries.by_client` | `{client, device_id?}` | Counter per client; `device_id` present when attributed at query time |
//! | `dns.queries.by_device` | `{device_id}` | Counter per device; recorded only when the query is attributed to a known device |
//! | `dns.blocked.by_tracker` | `{company}` | Counter; blocked queries whose domain is a recognised tracker (see [`wardnet_common::trackers`]) |

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
    by_device: Counter,
    by_tracker: Counter,
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
                by_device: meter.counter("dns.queries.by_device"),
                by_tracker: meter.counter("dns.blocked.by_tracker"),
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
        // Build the broadcast event only when a live-stream viewer is
        // connected. With none — the normal state — `send` would fail and
        // discard it anyway, so `row_to_event`'s handful of string clones
        // would be pure waste on every query. Computed here, before `row` is
        // moved into the persist send below.
        let event = (self.stream_tx.receiver_count() > 0).then(|| row_to_event(&row));
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
        // Closed/no-subscriber broadcast errors are normal and ignored. A
        // subscriber that raced in after the `receiver_count` check above just
        // misses this one event — live-stream is best-effort.
        if let Some(event) = event {
            let _ = self.stream_tx.send(event);
        }
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

/// Record the DNS stats metrics from a single query log row.
///
/// Label strings are sorted JSON objects as required by the stats schema.
/// `dns.queries.by_domain` and `dns.blocked.by_tracker` are only recorded for
/// blocked queries so those per-domain / per-company counters stay bounded.
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

        // Attribute the block to its operating company when the domain is a
        // recognised tracker. The catalogue is small and lookups are cheap;
        // unrecognised blocks (custom rules, niche lists) simply go
        // uncategorised and never touch this counter, keeping it bounded to
        // the handful of companies in the catalogue.
        if let Some(company) = wardnet_common::trackers::company_for_domain(&row.domain) {
            let company = company.replace('"', r#"\""#);
            inst.by_tracker
                .add(&format!(r#"{{"company":"{company}"}}"#), 1.0);
        }
    }

    let client = row.client_ip.replace('"', r#"\""#);
    // `device_id` is present only when the DNS server resolved the querying
    // device at query time — top-clients groups by it (falling back to
    // `client` for unknown sources) so attribution survives DHCP churn.
    // Keys stay sorted as the stats schema requires (client < device_id).
    let labels = match &row.device_id {
        Some(device_id) => {
            // Per-device series keyed on the stable device id (bounded by the
            // number of devices on the network, so cardinality is safe). This
            // powers the per-device timeseries; unlike `by_client` it does not
            // fragment when a device's IP changes.
            inst.by_device
                .add(&format!(r#"{{"device_id":"{device_id}"}}"#), 1.0);
            format!(r#"{{"client":"{client}","device_id":"{device_id}"}}"#)
        }
        None => format!(r#"{{"client":"{client}"}}"#),
    };
    inst.by_client.add(&labels, 1.0);
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
