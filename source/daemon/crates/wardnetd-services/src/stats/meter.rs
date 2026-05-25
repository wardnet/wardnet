use std::sync::Arc;

use super::buffer::StatsBuffer;

/// Factory for creating [`Counter`] and [`Gauge`] instruments that all write
/// into the same [`StatsBuffer`].
pub struct Meter {
    buffer: Arc<StatsBuffer>,
}

impl Meter {
    #[must_use]
    pub fn new(buffer: Arc<StatsBuffer>) -> Self {
        Self { buffer }
    }

    /// Create a counter instrument for the given metric name.
    #[must_use]
    pub fn counter(&self, metric: impl Into<String>) -> Counter {
        Counter {
            metric: metric.into(),
            buffer: self.buffer.clone(),
        }
    }

    /// Create a gauge instrument for the given metric name.
    #[must_use]
    pub fn gauge(&self, metric: impl Into<String>) -> Gauge {
        Gauge {
            metric: metric.into(),
            buffer: self.buffer.clone(),
        }
    }

    /// Expose the underlying buffer so the flush runner can drain it.
    #[must_use]
    pub fn buffer(&self) -> Arc<StatsBuffer> {
        self.buffer.clone()
    }
}

/// A monotonically increasing counter backed by [`StatsBuffer`].
///
/// Each [`add`](Counter::add) call accumulates into the per-minute bucket for
/// the `(metric, labels)` pair.
pub struct Counter {
    metric: String,
    buffer: Arc<StatsBuffer>,
}

impl Counter {
    /// Increment the counter by `delta` for the given label set.
    ///
    /// `labels` must be a sorted JSON object string, e.g. `{"outcome":"blocked"}`.
    pub fn add(&self, labels: &str, delta: f64) {
        self.buffer.record(&self.metric, labels, delta, "counter");
    }
}

/// A gauge instrument backed by [`StatsBuffer`].
///
/// Each [`set`](Gauge::set) call overwrites the current-minute bucket value
/// for the `(metric, labels)` pair.
pub struct Gauge {
    metric: String,
    buffer: Arc<StatsBuffer>,
}

impl Gauge {
    /// Set the gauge to `value` for the given label set.
    ///
    /// `labels` must be a sorted JSON object string, e.g. `{"outcome":"forwarded"}`.
    pub fn set(&self, labels: &str, value: f64) {
        self.buffer.record(&self.metric, labels, value, "gauge");
    }
}
