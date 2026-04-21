//! Shared DTOs for the auto-update subsystem.
//!
//! Types used across the daemon, SDK, and web UI to describe update state,
//! release metadata, and history entries.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Update release channel.
///
/// `Stable` tracks only full semver releases (e.g. `0.1.2`). `Beta` also
/// considers pre-release builds. `Beta` is v2 scope; the v1 daemon still
/// accepts the value but the runner only checks `Stable`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum UpdateChannel {
    #[default]
    Stable,
    Beta,
}

impl UpdateChannel {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
        }
    }

    /// Parse a string form. Named `parse_opt` to avoid shadowing
    /// `std::str::FromStr::from_str`.
    #[must_use]
    pub fn parse_opt(value: &str) -> Option<Self> {
        match value {
            "stable" => Some(Self::Stable),
            "beta" => Some(Self::Beta),
            _ => None,
        }
    }
}

/// Phase of an in-flight update install.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum InstallPhase {
    /// No install in progress.
    #[default]
    Idle,
    /// Resolving the manifest for the target channel.
    Checking,
    /// Downloading the release tarball.
    Downloading { bytes: u64, total: Option<u64> },
    /// Verifying SHA-256 and (if enabled) minisign signature.
    Verifying,
    /// Extracting and staging the binary on disk.
    Staging,
    /// Atomically swapping the binary.
    Swapping,
    /// Swap completed, daemon will restart now.
    RestartPending,
    /// The new binary is running; clearing pending marker.
    Applied,
    /// Install failed — message contains the reason.
    Failed { reason: String },
}

/// Release metadata fetched from the release source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Release {
    /// Semver version string (e.g. `0.1.2`).
    pub version: String,
    /// Fully-qualified HTTPS URL of the tarball asset.
    pub tarball_url: String,
    /// Fully-qualified HTTPS URL of the `.sha256` sidecar.
    pub sha256_url: String,
    /// Fully-qualified HTTPS URL of the `.minisig` signature (None if signing
    /// is disabled server-side; v1 production builds must always include it).
    pub minisig_url: Option<String>,
    /// When the release was published upstream.
    pub published_at: Option<DateTime<Utc>>,
    /// Short release notes in plain text (optional).
    pub notes: Option<String>,
}

/// A single entry in the persistent update history.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateHistoryEntry {
    pub id: i64,
    pub from_version: String,
    pub to_version: String,
    pub phase: String,
    pub status: UpdateHistoryStatus,
    pub error: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

/// Final status of a historical update attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum UpdateHistoryStatus {
    Started,
    Succeeded,
    Failed,
    RolledBack,
}

impl UpdateHistoryStatus {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::RolledBack => "rolled_back",
        }
    }

    /// Parse a string form. Named `parse_opt` to avoid shadowing
    /// `std::str::FromStr::from_str`.
    #[must_use]
    pub fn parse_opt(value: &str) -> Option<Self> {
        match value {
            "started" => Some(Self::Started),
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            "rolled_back" => Some(Self::RolledBack),
            _ => None,
        }
    }
}

/// Handle returned when an install is kicked off.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct InstallHandle {
    pub install_id: Uuid,
    pub target_version: String,
}

/// Snapshot of the update subsystem's current state.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct UpdateStatus {
    /// Currently running daemon version.
    pub current_version: String,
    /// Latest version known for the active channel (if any check has run).
    pub latest_version: Option<String>,
    /// Whether a newer version than the running one is available.
    pub update_available: bool,
    /// Whether auto-update is enabled.
    pub auto_update_enabled: bool,
    /// Active release channel.
    pub channel: UpdateChannel,
    /// ISO-8601 UTC timestamp of the last successful check, if any.
    pub last_check_at: Option<DateTime<Utc>>,
    /// ISO-8601 UTC timestamp of the most recent install attempt.
    pub last_install_at: Option<DateTime<Utc>>,
    /// Current install phase.
    pub install_phase: InstallPhase,
    /// Version the in-flight install is targeting (if any).
    pub pending_version: Option<String>,
    /// Whether a `.old` binary is present that could be rolled back to.
    pub rollback_available: bool,
}
