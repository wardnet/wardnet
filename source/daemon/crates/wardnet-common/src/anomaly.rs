//! Anomalies: typed, admin-facing conditions with an open/resolved lifecycle.
//!
//! An anomaly is a problem worth surfacing to the box's administrator: not a
//! raw log line, but a typed condition paired with a plain-language
//! description and a hint at what the admin can do about it.
//!
//! What separates an anomaly from a log line is that it has a *lifetime*. It
//! is opened once, stays open while the condition holds, and resolves when it
//! stops holding — which is what makes alerting edge-triggered. A blocklist
//! that has been failing for three days has one anomaly, not one per retry.
//!
//! Every anomaly carries an [`AnomalyType`] from the catalogue below, and each
//! type has exactly one detector behind it (see the `anomaly` module in
//! `wardnetd-services`). The type is the join key between an emit site, its
//! remediation hint, and the detector that decides whether the condition is
//! still true.
//!
//! Anomalies are also how *per-entity* errors are surfaced. An open anomaly
//! whose [`Anomaly::subject_id`] is a tunnel id **is** that tunnel's current
//! error — there is deliberately no `last_error` column on the entity. One
//! writer, one lifecycle, one place to look.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// How serious an [`Anomaly`] is. Deliberately separate from tracing log
/// levels: severity here is a product judgement about admin impact, not a
/// logging verbosity knob.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnomalySeverity {
    /// A failure that has degraded or stopped a feature.
    Error,
    /// A condition worth attention that has not (yet) broken anything.
    Warning,
    /// Informational; surfaced for visibility, no action required.
    Info,
}

impl AnomalySeverity {
    /// Stable lowercase identifier, used by the HTTP API mirror.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
        }
    }
}

/// The hardcoded anomaly catalogue.
///
/// Adding an anomaly means adding a variant here, giving it a severity,
/// component, hint and landing page, and registering a detector for it in
/// `wardnetd_services::anomaly::AnomalyDetectorRegistry`. The registry is
/// keyed by this type, so a variant without a detector is a wiring bug rather
/// than a silently inert entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnomalyType {
    /// A tunnel could not be brought up.
    TunnelStartFailed,
    /// A tunnel is configured and present but not carrying traffic — no
    /// handshake, or its kernel interface vanished.
    TunnelUnhealthy,
    /// A daemon self-update failed.
    UpdateFailed,
    /// Two hosts are claiming the same DHCP address.
    DhcpConflict,
    /// A managed routing table disappeared out from under the daemon.
    RouteTableLost,
    /// A blocklist has failed to refresh often enough to be considered stale.
    BlocklistRefreshFailing,
}

impl AnomalyType {
    /// Every catalogue entry, for registry construction and exhaustive tests.
    pub const ALL: &'static [Self] = &[
        Self::TunnelStartFailed,
        Self::TunnelUnhealthy,
        Self::UpdateFailed,
        Self::DhcpConflict,
        Self::RouteTableLost,
        Self::BlocklistRefreshFailing,
    ];

    /// Stable `snake_case` identifier. This is the wire form: it is the
    /// notification `kind`, the stored `anomaly_type` column, and what the
    /// SDKs match on — changing one is a breaking change.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TunnelStartFailed => "tunnel_start_failed",
            Self::TunnelUnhealthy => "tunnel_unhealthy",
            Self::UpdateFailed => "update_failed",
            Self::DhcpConflict => "dhcp_conflict",
            Self::RouteTableLost => "route_table_lost",
            Self::BlocklistRefreshFailing => "blocklist_refresh_failing",
        }
    }

    /// Parse the stored/wire form back into a catalogue entry.
    ///
    /// Returns `None` for an unknown slug, which happens when a row written by
    /// a newer daemon is read after a downgrade. Callers skip such rows rather
    /// than failing the whole query — an unreadable anomaly must not take the
    /// dashboard down with it. Deliberately not `FromStr`: that trait's
    /// `Err` would have to be a type nobody wants to construct or match on for
    /// what is really an "unknown, skip it" signal.
    #[must_use]
    pub fn from_slug(value: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|t| t.as_str() == value)
    }

    /// How serious this class of anomaly is.
    #[must_use]
    pub const fn severity(self) -> AnomalySeverity {
        match self {
            Self::TunnelStartFailed
            | Self::UpdateFailed
            | Self::RouteTableLost
            | Self::BlocklistRefreshFailing => AnomalySeverity::Error,
            Self::TunnelUnhealthy | Self::DhcpConflict => AnomalySeverity::Warning,
        }
    }

    /// The subsystem that owns this class of anomaly.
    #[must_use]
    pub const fn component(self) -> &'static str {
        match self {
            Self::TunnelStartFailed | Self::TunnelUnhealthy => "tunnel",
            Self::UpdateFailed => "update",
            Self::DhcpConflict => "dhcp",
            Self::RouteTableLost => "routing",
            Self::BlocklistRefreshFailing => "dns",
        }
    }

    /// The coarse, app-agnostic landing page for this class of anomaly.
    ///
    /// This is what the push notification carries, and the fallback the web
    /// surfaces use when they cannot resolve a precise per-subject route. It
    /// is deliberately not a deep link: the exact route for a subject differs
    /// between the desktop admin site and the mobile PWA, so that resolution
    /// belongs in the client, not here.
    #[must_use]
    pub const fn url(self) -> &'static str {
        match self {
            Self::TunnelStartFailed | Self::TunnelUnhealthy => "/tunnels",
            Self::UpdateFailed => "/settings",
            Self::DhcpConflict => "/dhcp",
            Self::RouteTableLost => "/routing",
            Self::BlocklistRefreshFailing => "/dns/filter",
        }
    }

    /// Default remediation hint shown to the admin.
    ///
    /// Kept in one place so the wording stays consistent and reviewable.
    #[must_use]
    pub const fn hint(self) -> &'static str {
        match self {
            Self::TunnelStartFailed => {
                "Check the tunnel's endpoint, keys, and that the peer is reachable, \
                 then rebuild it. The full WireGuard error is in the daemon logs."
            }
            Self::TunnelUnhealthy => {
                "The tunnel is configured but not passing traffic. Check that the peer \
                 is up and reachable, then rebuild the tunnel. Devices routed through \
                 it have no internet until it recovers."
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
            Self::BlocklistRefreshFailing => {
                "The list is still enforcing its last good download, but it is going \
                 stale. Check the URL is still valid and reachable from the gateway, \
                 then refresh it from Ad Blocking."
            }
        }
    }
}

/// An open or resolved anomaly.
///
/// `severity`, `component` and `hint` are deliberately *not* stored — they are
/// derived from [`Anomaly::anomaly_type`] so the catalogue stays the single
/// source of truth and a reworded hint applies retroactively.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anomaly {
    pub id: Uuid,
    pub anomaly_type: AnomalyType,
    /// The entity this anomaly is about — a tunnel id, a blocklist id — or
    /// `None` when it concerns the box as a whole. Together with the type this
    /// is the deduplication key: at most one *open* anomaly per pair.
    pub subject_id: Option<String>,
    /// Plain-language description of what happened, with the specifics.
    pub message: String,
    /// Detector-authored payload, read back by that detector's `reevaluate`
    /// and used by the web surfaces to build a deep link to the subject.
    ///
    /// Best-effort: no key is guaranteed beyond those the owning detector
    /// documents. Never put a secret in here — it is served to admin clients.
    pub details: Option<serde_json::Value>,
    pub opened_at: DateTime<Utc>,
    /// When the condition was last observed. Drives `stale_after` expiry for
    /// types with no authoritative "is it still true?" check.
    pub last_seen_at: DateTime<Utc>,
    /// How many times the condition has been observed since it opened.
    pub occurrences: u32,
    /// `None` while the anomaly is open.
    pub resolved_at: Option<DateTime<Utc>>,
}

impl Anomaly {
    /// Whether the underlying condition is still believed to hold.
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.resolved_at.is_none()
    }
}

/// A single observation of a condition, submitted to the anomaly service.
///
/// Deliberately not an [`Anomaly`]: a report has no identity and no lifecycle.
/// The service decides whether it opens a new anomaly or merely touches one
/// that is already open, which is what keeps alerting edge-triggered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnomalyReport {
    pub anomaly_type: AnomalyType,
    pub subject_id: Option<String>,
    pub message: String,
    pub details: Option<serde_json::Value>,
    pub observed_at: DateTime<Utc>,
}

impl AnomalyReport {
    /// Build a report observed now, with no subject and no details.
    #[must_use]
    pub fn new(anomaly_type: AnomalyType, message: impl Into<String>) -> Self {
        Self {
            anomaly_type,
            subject_id: None,
            message: message.into(),
            details: None,
            observed_at: Utc::now(),
        }
    }

    /// Attach the entity this report is about.
    #[must_use]
    pub fn with_subject(mut self, subject_id: impl Into<String>) -> Self {
        self.subject_id = Some(subject_id.into());
        self
    }

    /// Attach the detector-authored payload.
    #[must_use]
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }

    /// Override the observation time. Use when replaying a domain event, so
    /// the anomaly carries when the condition happened rather than when the
    /// listener got round to it.
    #[must_use]
    pub const fn observed_at(mut self, at: DateTime<Utc>) -> Self {
        self.observed_at = at;
        self
    }
}

/// A detector's verdict on whether a previously-opened anomaly still holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnomalyStatus {
    Open,
    Resolved,
}

/// Which anomalies a query should return.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnomalyQueryStatus {
    /// Only anomalies whose condition still holds. The default: the dashboard
    /// asks "what is wrong right now", not "what has ever been wrong".
    #[default]
    Open,
    /// Only anomalies that have since resolved.
    Resolved,
    All,
}

/// Filter for an anomaly listing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AnomalyFilter {
    pub status: AnomalyQueryStatus,
    /// Restrict to anomalies about one entity. This is how a tunnel card asks
    /// "what is currently wrong with *me*".
    pub subject_id: Option<String>,
}

/// Outcome of a reevaluation pass over the open anomalies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ReevaluateSummary {
    /// How many open anomalies were examined.
    pub evaluated: u32,
    /// How many of those were closed as no longer holding.
    pub resolved: u32,
}
