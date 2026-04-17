use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use serde::Serialize;
use tokio::sync::broadcast;
use tracing::field::{Field, Visit};
use tracing_subscriber::registry::LookupSpan;

use super::component::{BoxedLayer, LogComponent};

/// A single structured log entry broadcast to WebSocket clients.
#[derive(Debug, Clone, Serialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
    /// Structured fields from the event (excludes `message`).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, String>,
    /// Fields from the current span context (e.g. HTTP method, path, status).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub span: BTreeMap<String, String>,
}

/// Service that broadcasts structured log entries to WebSocket subscribers.
#[async_trait]
pub trait LogStream: Send + Sync {
    /// Subscribe to receive live log entries.
    fn subscribe(&self) -> broadcast::Receiver<LogEntry>;
}

/// Production implementation backed by a `tokio::sync::broadcast` channel.
///
/// The service owns the channel. The tracing layer (via [`LogComponent::tracing_layer`])
/// receives a clone of the sender and publishes events.
pub struct LogStreamService {
    tx: broadcast::Sender<LogEntry>,
    active: Arc<AtomicBool>,
}

impl LogStreamService {
    /// Create a new log stream with the given channel capacity.
    ///
    /// Starts inactive — call [`LogComponent::start`] after the subscriber
    /// is built.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self {
            tx,
            active: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[async_trait]
impl LogStream for LogStreamService {
    fn subscribe(&self) -> broadcast::Receiver<LogEntry> {
        self.tx.subscribe()
    }
}

impl LogComponent for LogStreamService {
    fn tracing_layer(&self) -> BoxedLayer {
        Box::new(LogStreamLayer {
            tx: self.tx.clone(),
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

/// Tracing layer that publishes structured log events to the broadcast channel.
struct LogStreamLayer {
    tx: broadcast::Sender<LogEntry>,
    active: Arc<AtomicBool>,
}

/// Visitor that extracts all fields from a tracing event.
struct FieldVisitor {
    message: String,
    fields: BTreeMap<String, String>,
}

impl FieldVisitor {
    fn new() -> Self {
        Self {
            message: String::new(),
            fields: BTreeMap::new(),
        }
    }
}

impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}");
        } else {
            self.fields
                .insert(field.name().to_string(), format!("{value:?}"));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else {
            self.fields
                .insert(field.name().to_string(), value.to_string());
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields
            .insert(field.name().to_string(), value.to_string());
    }
}

/// Storage for span fields, attached via `Extensions`.
#[derive(Default)]
struct SpanFields(BTreeMap<String, String>);

/// Visitor that collects span fields into a `BTreeMap`.
struct SpanFieldVisitor<'a>(&'a mut BTreeMap<String, String>);

impl Visit for SpanFieldVisitor<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        self.0
            .insert(field.name().to_string(), format!("{value:?}"));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_string(), value.to_string());
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0.insert(field.name().to_string(), value.to_string());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().to_string(), value.to_string());
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0.insert(field.name().to_string(), value.to_string());
    }
}

impl<S> tracing_subscriber::Layer<S> for LogStreamLayer
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if let Some(span) = ctx.span(id) {
            let mut fields = BTreeMap::new();
            attrs.record(&mut SpanFieldVisitor(&mut fields));
            span.extensions_mut().insert(SpanFields(fields));
        }
    }

    fn on_record(
        &self,
        id: &tracing::span::Id,
        values: &tracing::span::Record<'_>,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if let Some(span) = ctx.span(id) {
            let mut ext = span.extensions_mut();
            if let Some(fields) = ext.get_mut::<SpanFields>() {
                values.record(&mut SpanFieldVisitor(&mut fields.0));
            }
        }
    }

    fn on_event(&self, event: &tracing::Event<'_>, ctx: tracing_subscriber::layer::Context<'_, S>) {
        if !self.active.load(Ordering::Relaxed) {
            return;
        }

        let metadata = event.metadata();

        let level = metadata.level().as_str().to_uppercase();
        let target = metadata.target().to_string();
        let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

        let mut visitor = FieldVisitor::new();
        event.record(&mut visitor);

        // Collect fields from the current span and its parents.
        let mut span_fields = BTreeMap::new();
        if let Some(scope) = ctx.event_scope(event) {
            for span in scope {
                let ext = span.extensions();
                if let Some(fields) = ext.get::<SpanFields>() {
                    for (k, v) in &fields.0 {
                        span_fields.insert(k.clone(), v.clone());
                    }
                }
            }
        }

        let _ = self.tx.send(LogEntry {
            timestamp,
            level,
            target,
            message: visitor.message,
            fields: visitor.fields,
            span: span_fields,
        });
    }
}
