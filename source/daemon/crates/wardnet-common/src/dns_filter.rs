//! DTOs for the per-device DNS filtering subsystem.
//!
//! A **DNS Filter Profile** groups blocklists, an allowlist, and custom
//! filter rules under a single name. Devices opt into one or more profiles;
//! when a device has no explicit profiles the global default profiles
//! apply. Three profiles are seeded as `builtin`: "Ad Blocking",
//! "Parental Controls", and "Malware & Phishing".
//!
//! See `.agents/architecture.md` for the layered design and
//! `docs/issues/221.md` for the full plan that motivates this module.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A named bundle of DNS filter sources (blocklists, allowlist, custom rules).
///
/// Builtin profiles cannot be deleted — the API responds with `409 Conflict`
/// when an admin tries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DnsFilterProfile {
    pub id: Uuid,
    pub name: String,
    pub builtin: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Per-device DNS filtering settings.
///
/// `enabled = false` is the kill switch; the device's queries skip filtering
/// entirely. When `enabled = true` and `profile_ids` is empty, the device
/// inherits the global default profiles from [`DnsFilterConfig`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DeviceDnsFilterSettings {
    pub device_id: Uuid,
    pub enabled: bool,
    /// Explicit profile assignments. Empty means "follow the default profiles".
    pub profile_ids: Vec<Uuid>,
    pub updated_at: DateTime<Utc>,
}

impl DeviceDnsFilterSettings {
    /// Default state for a device that has never been configured: filtering
    /// on, no explicit profile assignment.
    #[must_use]
    pub fn default_for(device_id: Uuid) -> Self {
        Self {
            device_id,
            enabled: true,
            profile_ids: Vec::new(),
            updated_at: Utc::now(),
        }
    }
}

/// Global DNS filtering configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DnsFilterConfig {
    /// Global emergency stop. When `false`, every query short-circuits to
    /// `Pass` regardless of profile state.
    pub enabled: bool,
    /// Profiles applied to devices with no explicit assignment. Empty means
    /// unassigned devices skip filtering. Multiple profiles stack — a domain
    /// blocked in any of them is blocked. Treat as a set: the order across
    /// the get/set roundtrip is not preserved.
    pub default_profile_ids: Vec<Uuid>,
}

impl Default for DnsFilterConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_profile_ids: Vec::new(),
        }
    }
}
