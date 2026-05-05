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

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::{broadcast, mpsc};
use wardnet_common::api::QueryLogEvent;
use wardnetd_data::repository::QueryLogRow;

/// Default mpsc buffer for queued query log entries.
pub const DEFAULT_MPSC_CAPACITY: usize = 5_000;

/// Default broadcast capacity for live-stream subscribers.
pub const DEFAULT_BROADCAST_CAPACITY: usize = 256;

/// Hot-path sink shared between the DNS server (producer) and the
/// persistence runner + WS subscribers (consumers).
pub struct DnsLogSink {
    persist_tx: mpsc::Sender<QueryLogRow>,
    stream_tx: broadcast::Sender<QueryLogEvent>,
    dropped_entries: AtomicU64,
}

impl DnsLogSink {
    /// Build a new sink with default capacities, returning the sink and
    /// the mpsc receiver that the persistence runner will drain.
    #[must_use]
    pub fn new() -> (Arc<Self>, mpsc::Receiver<QueryLogRow>) {
        Self::with_capacities(DEFAULT_MPSC_CAPACITY, DEFAULT_BROADCAST_CAPACITY)
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
            stream_tx,
            dropped_entries: AtomicU64::new(0),
        });
        (sink, persist_rx)
    }

    /// Record a query log entry. Non-blocking: if the persistence buffer
    /// is full the entry is dropped and the dropped-counter incremented.
    /// The broadcast send is fire-and-forget; if no live-stream
    /// subscribers exist `send` returns `Err` which we ignore.
    pub fn record(&self, row: QueryLogRow) {
        let event = row_to_event(&row);
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
}

/// Project a `QueryLogRow` into the wire-format event sent over the WS.
#[must_use]
pub fn row_to_event(row: &QueryLogRow) -> QueryLogEvent {
    QueryLogEvent {
        timestamp: row.timestamp.clone(),
        client_ip: row.client_ip.clone(),
        domain: row.domain.clone(),
        query_type: row.query_type.clone(),
        result: row.result.clone(),
        upstream: row.upstream.clone(),
        latency_ms: row.latency_ms,
        device_id: row.device_id.clone(),
    }
}
