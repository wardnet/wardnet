use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use wardnetd_data::repository::IntradayStatRow;

/// In-memory accumulator for pre-aggregated stats before they are flushed
/// to `stats_intraday`.
///
/// Counter entries accumulate (sum); gauge entries overwrite. All pending
/// increments are bucketed to the current UTC minute at drain time.
pub struct StatsBuffer {
    inner: Mutex<HashMap<(String, String), (f64, String)>>,
}

impl StatsBuffer {
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(HashMap::new()),
        })
    }

    /// Record a measurement.
    ///
    /// - `kind = "counter"` — value accumulates with existing entry.
    /// - `kind = "gauge"` — value overwrites existing entry.
    /// - `labels` must be a sorted JSON object string (e.g. `{"outcome":"blocked"}`).
    pub fn record(&self, metric: &str, labels: &str, value: f64, kind: &str) {
        let mut guard = self.inner.lock().expect("stats buffer lock poisoned");
        let key = (metric.to_owned(), labels.to_owned());
        match guard.get_mut(&key) {
            Some(entry) if entry.1 == "counter" => entry.0 += value,
            Some(entry) => entry.0 = value, // gauge: overwrite
            None => {
                guard.insert(key, (value, kind.to_owned()));
            }
        }
    }

    /// Drain all pending entries into [`IntradayStatRow`]s, clearing the buffer.
    ///
    /// All rows share the same `bucket_ts` = current UTC time truncated to the
    /// minute boundary (Unix seconds).
    #[must_use]
    pub fn drain(&self) -> Vec<IntradayStatRow> {
        let bucket_ts = current_minute_ts();
        let mut guard = self.inner.lock().expect("stats buffer lock poisoned");
        let entries = std::mem::take(&mut *guard);
        entries
            .into_iter()
            .map(|((metric, labels), (value, kind))| IntradayStatRow {
                metric,
                labels,
                bucket_ts,
                value,
                kind,
            })
            .collect()
    }
}

impl Default for StatsBuffer {
    fn default() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }
}

fn current_minute_ts() -> i64 {
    let now = chrono::Utc::now().timestamp();
    now - (now % 60)
}
