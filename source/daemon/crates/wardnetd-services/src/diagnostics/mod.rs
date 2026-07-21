//! Structured, admin-facing diagnostics.
//!
//! A diagnostic is a problem worth surfacing to the box's administrator: not a
//! raw log line, but a typed condition a component deliberately raises, paired
//! with a plain-language description and a hint at what the admin can do about
//! it. Components already publish [`WardnetEvent`]s for the interesting things
//! that happen; the [`listener`] converts the error-flavoured ones into
//! diagnostics via [`Diagnostic::from_event`] and records them in a
//! [`DiagnosticStore`], which the dashboard's recent-errors panel reads.
//!
//! This replaces scraping WARN/ERROR lines out of the tracing stream. Log
//! level is a poor proxy for "the admin should see this" — plenty of noise is
//! logged at WARN, and genuinely actionable failures often are not — and a
//! bare message string carries no code to group on and no remediation guidance.

pub mod listener;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use wardnet_common::event::WardnetEvent;

/// How serious a [`Diagnostic`] is. Deliberately separate from tracing log
/// levels: severity here is a product judgement about admin impact, not a
/// logging verbosity knob.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    /// A failure that has degraded or stopped a feature.
    Error,
    /// A condition worth attention that has not (yet) broken anything.
    Warning,
    /// Informational; surfaced for visibility, no action required.
    Info,
}

impl DiagnosticSeverity {
    /// Stable lowercase identifier, used by the HTTP API mirror.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
        }
    }
}

/// Stable identifier for a class of diagnostic.
///
/// The code is the join key between an emit site and its remediation hint (see
/// [`DiagnosticCode::hint`]), and lets the UI group, dedupe, and one day
/// deep-link to docs without parsing free-text messages. Adding a diagnostic
/// means adding a variant here, giving it a hint, and mapping the source event
/// in [`Diagnostic::from_event`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticCode {
    /// A tunnel could not be brought up.
    TunnelStartFailed,
    /// A daemon self-update failed.
    UpdateFailed,
    /// Two hosts are claiming the same DHCP address.
    DhcpConflict,
    /// A managed routing table disappeared out from under the daemon.
    RouteTableLost,
}

impl DiagnosticCode {
    /// Stable `snake_case` identifier, used by the HTTP API mirror.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TunnelStartFailed => "tunnel_start_failed",
            Self::UpdateFailed => "update_failed",
            Self::DhcpConflict => "dhcp_conflict",
            Self::RouteTableLost => "route_table_lost",
        }
    }

    /// Default remediation hint shown to the admin.
    ///
    /// Kept in one place so the wording stays consistent and reviewable; an
    /// emit site with more specific context is free to override it when
    /// constructing the [`Diagnostic`].
    #[must_use]
    pub fn hint(self) -> &'static str {
        match self {
            Self::TunnelStartFailed => {
                "Check the tunnel's endpoint, keys, and that the peer is reachable, \
                 then rebuild it. The full WireGuard error is in the daemon logs."
            }
            Self::UpdateFailed => {
                "The previous version is still running. Check connectivity and free \
                 disk space, then retry the update from Settings."
            }
            Self::DhcpConflict => {
                "Two devices are using the same address. Reserve a fixed lease for \
                 one of them, or check for a second DHCP server on the network."
            }
            Self::RouteTableLost => {
                "A managed routing table was removed unexpectedly, which can drop \
                 tunnelled traffic. If it recurs, look for other tools flushing \
                 routes; restarting the daemon rebuilds them."
            }
        }
    }
}

/// A structured, admin-facing problem report.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// When the underlying condition occurred (the source event's timestamp).
    pub timestamp: DateTime<Utc>,
    /// Stable class identifier.
    pub code: DiagnosticCode,
    /// How serious the condition is.
    pub severity: DiagnosticSeverity,
    /// The subsystem that raised it (e.g. `"tunnel"`, `"dhcp"`).
    pub component: &'static str,
    /// Plain-language description of what happened, with the specifics.
    pub message: String,
    /// What the admin can do about it; defaults to [`DiagnosticCode::hint`].
    pub hint: String,
}

impl Diagnostic {
    /// Convert an error-flavoured domain event into a diagnostic.
    ///
    /// Returns `None` for events that are not admin-actionable problems — the
    /// vast majority of the bus. Non-exhaustive by design: mapping a new event
    /// here is the entire cost of surfacing another diagnostic.
    #[must_use]
    pub fn from_event(event: &WardnetEvent) -> Option<Self> {
        match event {
            WardnetEvent::TunnelStartFailed {
                interface_name,
                error,
                timestamp,
                ..
            } => Some(Self::new(
                *timestamp,
                DiagnosticCode::TunnelStartFailed,
                DiagnosticSeverity::Error,
                "tunnel",
                format!("Tunnel {interface_name} failed to start: {error}"),
            )),
            WardnetEvent::UpdateFailed {
                target_version,
                error,
                timestamp,
                ..
            } => Some(Self::new(
                *timestamp,
                DiagnosticCode::UpdateFailed,
                DiagnosticSeverity::Error,
                "update",
                format!("Update to {target_version} failed: {error}"),
            )),
            WardnetEvent::DhcpConflictDetected {
                mac,
                ip,
                details,
                timestamp,
            } => Some(Self::new(
                *timestamp,
                DiagnosticCode::DhcpConflict,
                DiagnosticSeverity::Warning,
                "dhcp",
                format!("Address conflict for {ip} (MAC {mac}): {details}"),
            )),
            WardnetEvent::RouteTableLost { table, timestamp } => Some(Self::new(
                *timestamp,
                DiagnosticCode::RouteTableLost,
                DiagnosticSeverity::Error,
                "routing",
                format!("Managed routing table {table} was lost"),
            )),
            _ => None,
        }
    }

    /// Build a diagnostic, defaulting the hint to the code's catalogue entry.
    fn new(
        timestamp: DateTime<Utc>,
        code: DiagnosticCode,
        severity: DiagnosticSeverity,
        component: &'static str,
        message: String,
    ) -> Self {
        Self {
            timestamp,
            code,
            severity,
            component,
            message,
            hint: code.hint().to_owned(),
        }
    }
}

/// Read side of the recent-diagnostics buffer, exposed to the HTTP API.
pub trait RecentDiagnostics: Send + Sync {
    /// Return the buffered diagnostics, oldest first.
    fn recent(&self) -> Vec<Diagnostic>;
}

/// Write side of the buffer, held by the [`listener`].
pub trait DiagnosticSink: Send + Sync {
    /// Append a diagnostic, evicting the oldest if the buffer is full.
    fn record(&self, diagnostic: Diagnostic);
}

/// Fixed-capacity ring buffer of recent diagnostics.
///
/// Cheap to clone — clones share the same underlying buffer, so the read
/// handle held by the log service and the write handle held by the listener
/// observe the same entries.
#[derive(Clone)]
pub struct DiagnosticStore {
    entries: Arc<Mutex<VecDeque<Diagnostic>>>,
    capacity: usize,
}

impl DiagnosticStore {
    /// Create a store retaining up to `capacity` diagnostics (minimum 1).
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: Arc::new(Mutex::new(VecDeque::with_capacity(capacity))),
            capacity: capacity.max(1),
        }
    }
}

impl RecentDiagnostics for DiagnosticStore {
    fn recent(&self) -> Vec<Diagnostic> {
        self.entries.lock().unwrap().iter().cloned().collect()
    }
}

impl DiagnosticSink for DiagnosticStore {
    fn record(&self, diagnostic: Diagnostic) {
        let mut buf = self.entries.lock().unwrap();
        if buf.len() >= self.capacity {
            buf.pop_front();
        }
        buf.push_back(diagnostic);
    }
}

#[cfg(test)]
mod tests;
