use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;

use super::component::{BoxedLayer, LogComponent};

/// A captured error or warning entry.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorEntry {
    pub timestamp: DateTime<Utc>,
    pub level: String,
    pub target: String,
    pub message: String,
}

/// Service that captures and exposes recent errors and warnings from the
/// tracing pipeline.
#[async_trait]
pub trait ErrorNotifier: Send + Sync {
    /// Return the most recent errors and warnings (newest last).
    fn get_recent_errors(&self) -> Vec<ErrorEntry>;
}

/// Production implementation backed by a fixed-capacity ring buffer.
///
/// The service owns the data. The tracing layer (via [`LogComponent::tracing_layer`])
/// receives cloned `Arc` handles and calls back into the service's internal state.
pub struct ErrorNotifierService {
    entries: Arc<Mutex<VecDeque<ErrorEntry>>>,
    max_entries: usize,
    active: Arc<AtomicBool>,
}

impl ErrorNotifierService {
    /// Create a new notifier with the given buffer capacity.
    ///
    /// Starts inactive — call [`LogComponent::start`] after the subscriber
    /// is built.
    #[must_use]
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Arc::new(Mutex::new(VecDeque::with_capacity(max_entries))),
            max_entries,
            active: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[async_trait]
impl ErrorNotifier for ErrorNotifierService {
    fn get_recent_errors(&self) -> Vec<ErrorEntry> {
        self.entries.lock().unwrap().iter().cloned().collect()
    }
}

impl LogComponent for ErrorNotifierService {
    fn tracing_layer(&self) -> BoxedLayer {
        Box::new(ErrorNotifierLayer {
            entries: self.entries.clone(),
            max_entries: self.max_entries,
            active: self.active.clone(),
        })
    }

    fn start(&self) {
        self.active.store(true, Ordering::Relaxed);
    }

    fn stop(&self) {
        self.active.store(false, Ordering::Relaxed);
    }

    fn is_active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }
}

// ---------------------------------------------------------------------------
// Tracing layer
// ---------------------------------------------------------------------------

/// Thin tracing layer that pushes WARN/ERROR entries into the service's buffer.
struct ErrorNotifierLayer {
    entries: Arc<Mutex<VecDeque<ErrorEntry>>>,
    max_entries: usize,
    active: Arc<AtomicBool>,
}

/// Visitor that extracts the message field from a tracing event.
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

impl<S> tracing_subscriber::Layer<S> for ErrorNotifierLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if !self.active.load(Ordering::Relaxed) {
            return;
        }

        let metadata = event.metadata();
        let level = *metadata.level();

        // Only capture WARN and ERROR.
        if level > tracing::Level::WARN {
            return;
        }

        let mut visitor = MessageVisitor(String::new());
        event.record(&mut visitor);

        let entry = ErrorEntry {
            timestamp: Utc::now(),
            level: level.as_str().to_uppercase(),
            target: metadata.target().to_string(),
            message: visitor.0,
        };

        let mut buf = self.entries.lock().unwrap();
        if buf.len() >= self.max_entries {
            buf.pop_front();
        }
        buf.push_back(entry);
    }
}
