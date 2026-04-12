use std::collections::BTreeMap;
use std::fmt;

use serde::Serialize;
use tokio::sync::broadcast;
use tracing::field::{Field, Visit};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

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

/// Sender half of the log broadcast channel.
///
/// Held by [`BroadcastLayer`] (inside the tracing subscriber) and by
/// [`AppState`](crate::state::AppState) so the WebSocket handler can call
/// [`subscribe`](Self::subscribe).
#[derive(Clone)]
pub struct LogBroadcaster {
    tx: broadcast::Sender<LogEntry>,
}

impl LogBroadcaster {
    /// Create a new broadcaster with the given channel capacity.
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    /// Subscribe to receive log entries.
    pub fn subscribe(&self) -> broadcast::Receiver<LogEntry> {
        self.tx.subscribe()
    }

    /// Send a log entry to all subscribers (best-effort, no error on lag).
    fn send(&self, entry: LogEntry) {
        let _ = self.tx.send(entry);
    }
}

/// Tracing subscriber layer that broadcasts log events to WebSocket clients.
pub struct BroadcastLayer {
    broadcaster: LogBroadcaster,
}

impl BroadcastLayer {
    /// Create a new layer backed by the given broadcaster.
    pub fn new(broadcaster: LogBroadcaster) -> Self {
        Self { broadcaster }
    }
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

impl<S> Layer<S> for BroadcastLayer
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        ctx: Context<'_, S>,
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
        ctx: Context<'_, S>,
    ) {
        if let Some(span) = ctx.span(id) {
            let mut ext = span.extensions_mut();
            if let Some(fields) = ext.get_mut::<SpanFields>() {
                values.record(&mut SpanFieldVisitor(&mut fields.0));
            }
        }
    }

    fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
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

        self.broadcaster.send(LogEntry {
            timestamp,
            level,
            target,
            message: visitor.message,
            fields: visitor.fields,
            span: span_fields,
        });
    }
}
