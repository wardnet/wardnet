use std::collections::VecDeque;
use std::sync::Mutex;

use serde::Serialize;

/// Maximum number of recent errors/warnings kept in memory.
const MAX_ENTRIES: usize = 15;

/// A recent error or warning entry.
#[derive(Debug, Clone, Serialize)]
pub struct RecentError {
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
}

/// Thread-safe ring buffer of recent errors and warnings.
///
/// Populated by [`RecentErrorsLayer`] (a tracing subscriber layer) and
/// read by the API via [`get_recent`](Self::get_recent).
#[derive(Clone)]
pub struct RecentErrors {
    entries: std::sync::Arc<Mutex<VecDeque<RecentError>>>,
}

impl RecentErrors {
    /// Create a new empty buffer.
    pub fn new() -> Self {
        Self {
            entries: std::sync::Arc::new(Mutex::new(VecDeque::with_capacity(MAX_ENTRIES))),
        }
    }

    /// Push a new entry, evicting the oldest if at capacity.
    fn push(&self, entry: RecentError) {
        let mut buf = self.entries.lock().unwrap();
        if buf.len() >= MAX_ENTRIES {
            buf.pop_front();
        }
        buf.push_back(entry);
    }

    /// Return all recent entries (newest last).
    pub fn get_recent(&self) -> Vec<RecentError> {
        self.entries.lock().unwrap().iter().cloned().collect()
    }
}

/// Tracing subscriber layer that captures WARN and ERROR events into a
/// [`RecentErrors`] ring buffer.
pub struct RecentErrorsLayer {
    buffer: RecentErrors,
}

impl RecentErrorsLayer {
    /// Create a new layer backed by the given buffer.
    pub fn new(buffer: RecentErrors) -> Self {
        Self { buffer }
    }
}

/// Visitor that extracts the message field.
struct MessageVisitor(String);

impl tracing::field::Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{value:?}");
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.0 = value.to_string();
        }
    }
}

impl<S> tracing_subscriber::Layer<S> for RecentErrorsLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let metadata = event.metadata();
        let level = *metadata.level();

        // Only capture WARN and ERROR.
        if level > tracing::Level::WARN {
            return;
        }

        let mut visitor = MessageVisitor(String::new());
        event.record(&mut visitor);

        self.buffer.push(RecentError {
            timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            level: level.as_str().to_uppercase(),
            target: metadata.target().to_string(),
            message: visitor.0,
        });
    }
}
