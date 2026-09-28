//! Device events: the observational record of how a device *behaved*.
//!
//! Every other per-device store answers how a device is **configured**. This
//! one answers what happened to it — when it appeared, when it changed address,
//! when it left, and when its policy was rebound underneath it.
//!
//! The kinds below are a closed catalogue. Each corresponds to a
//! [`crate::event::WardnetEvent`] the daemon already publishes, so recording one
//! is a listener concern rather than a new collection point in the hot path.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// What happened to a device.
///
/// Deliberately observational: every variant is something we *saw*, never a
/// judgement about whether it was a problem. Judgement belongs to the anomaly
/// subsystem, which reads these rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeviceEventKind {
    /// Seen for the first time — the device row was created.
    Discovered,
    /// Seen again after having been marked gone.
    Returned,
    /// Stopped answering and was marked absent.
    Gone,
    /// Claimed a different address than the one we had recorded.
    IpChanged,
    /// Moved between network zones.
    ZoneChanged,
    /// Its routing target was rebound.
    RoutingChanged,
    /// Its conntrack entries were flushed, dropping established flows.
    ///
    /// Recorded because a flush is *felt* by the device as connections dying
    /// mid-stream, yet nothing about the device caused it — without this the
    /// timeline shows an unexplained gap in traffic.
    ConntrackFlushed,
}

impl DeviceEventKind {
    /// Every catalogue entry, for exhaustive tests and API enumeration.
    pub const ALL: &'static [Self] = &[
        Self::Discovered,
        Self::Returned,
        Self::Gone,
        Self::IpChanged,
        Self::ZoneChanged,
        Self::RoutingChanged,
        Self::ConntrackFlushed,
    ];

    /// Stable `snake_case` identifier. This is the wire form and the stored
    /// `kind` column — changing one is a breaking change.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Discovered => "discovered",
            Self::Returned => "returned",
            Self::Gone => "gone",
            Self::IpChanged => "ip_changed",
            Self::ZoneChanged => "zone_changed",
            Self::RoutingChanged => "routing_changed",
            Self::ConntrackFlushed => "conntrack_flushed",
        }
    }

    /// Parse the stored/wire form back into a catalogue entry.
    ///
    /// `None` for an unknown slug, which is what a row written by a newer
    /// daemon looks like after a downgrade. Callers skip such rows rather than
    /// failing the query — one unreadable event must not take the whole
    /// timeline down with it.
    #[must_use]
    pub fn from_slug(value: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|k| k.as_str() == value)
    }
}

/// One recorded observation about a device.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DeviceEvent {
    pub id: i64,
    pub device_id: String,
    /// Denormalised so the event stays readable after the device row is pruned.
    pub mac: String,
    pub kind: DeviceEventKind,
    /// Kind-specific JSON payload, or `None`.
    pub details: Option<String>,
    pub created_at: DateTime<Utc>,
}
