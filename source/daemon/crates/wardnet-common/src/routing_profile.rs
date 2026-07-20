//! DTOs and matching helpers for the routing-profile subsystem.
//!
//! A **Routing Profile** groups per-domain routing rules under a single name.
//! Each rule pairs a domain pattern with a target: a specific tunnel, or
//! `direct` to carve the domain out of whatever tunnel the device is otherwise
//! bound to. Devices are assigned one or more profiles in priority order; when
//! several assigned profiles match a domain, the one earliest in the device's
//! order wins.
//!
//! This is the routing sibling of the [`crate::dns_filter`] module: same profile
//! catalog + rule + device-assignment shape, but the rule's outcome is a routing
//! decision rather than a filter action.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A named bundle of domain routing rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RoutingProfile {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Where a matched domain's traffic should go.
///
/// Deliberately narrower than [`crate::routing::RoutingTarget`]: a domain rule
/// can only send traffic to a tunnel or force it direct. `Default` has no
/// meaning for a per-domain override, so it is not representable here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DomainRoutingTarget {
    Tunnel { tunnel_id: Uuid },
    Direct,
}

impl DomainRoutingTarget {
    /// Persisted `(target_kind, tunnel_id)` pair. `tunnel_id` is only set for
    /// the tunnel target.
    #[must_use]
    pub fn to_storage(&self) -> (&'static str, Option<Uuid>) {
        match self {
            Self::Tunnel { tunnel_id } => ("tunnel", Some(*tunnel_id)),
            Self::Direct => ("direct", None),
        }
    }

    /// Reconstruct from the persisted `(target_kind, tunnel_id)` pair. Returns
    /// `None` for an unrecognised kind or a tunnel target missing its id.
    #[must_use]
    pub fn from_storage(kind: &str, tunnel_id: Option<Uuid>) -> Option<Self> {
        match kind {
            "tunnel" => tunnel_id.map(|tunnel_id| Self::Tunnel { tunnel_id }),
            "direct" => Some(Self::Direct),
            _ => None,
        }
    }
}

/// A single domain routing rule within a profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DomainRoutingRule {
    pub id: Uuid,
    pub profile_id: Uuid,
    /// Domain pattern. Either an exact name (`netflix.com`) or a wildcard
    /// (`*.netflix.com`) that also covers the apex — see [`domain_matches`].
    pub pattern: String,
    pub target: DomainRoutingTarget,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A device's ordered profile assignment. The list order is the priority: the
/// first profile that matches a resolved domain decides its route.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DeviceRoutingProfiles {
    pub device_id: Uuid,
    pub profile_ids: Vec<Uuid>,
}

/// Normalise a domain for comparison: lowercase and strip a trailing dot.
fn normalise(domain: &str) -> &str {
    domain.trim_end_matches('.')
}

/// Whether `name` is matched by `pattern`.
///
/// A wildcard pattern `*.example.com` matches both the apex `example.com` and
/// any subdomain (`www.example.com`, `a.b.example.com`) — so a single rule
/// covers a whole service. A bare pattern `example.com` matches that name
/// exactly. Matching is case-insensitive and ignores a trailing dot on either
/// side.
#[must_use]
pub fn domain_matches(pattern: &str, name: &str) -> bool {
    let pattern = normalise(pattern);
    let name = normalise(name);
    if let Some(base) = pattern.strip_prefix("*.") {
        if base.is_empty() {
            return false;
        }
        name.eq_ignore_ascii_case(base)
            || name.len() > base.len()
                && name.as_bytes()[name.len() - base.len() - 1] == b'.'
                && name[name.len() - base.len()..].eq_ignore_ascii_case(base)
    } else {
        name.eq_ignore_ascii_case(pattern)
    }
}

/// A comparable specificity score for a pattern. Higher is more specific.
///
/// Used only to break ties *within a single profile* (across profiles, the
/// device's assignment order decides). An exact match is more specific than a
/// wildcard, and more labels beat fewer, so `foo.example.com` outranks
/// `*.example.com` outranks `*.com`.
#[must_use]
pub fn pattern_specificity(pattern: &str) -> u32 {
    let pattern = normalise(pattern);
    let (base, exact_bonus) = pattern
        .strip_prefix("*.")
        .map_or((pattern, 1), |base| (base, 0));
    let labels = u32::try_from(base.split('.').filter(|l| !l.is_empty()).count())
        .unwrap_or(u32::MAX / 2);
    labels * 2 + exact_bonus
}

/// Validate a domain pattern for a routing rule. Returns a human-readable error
/// describing the first problem, or `Ok(())`.
///
/// Accepts an optional leading `*.` wildcard followed by a normal dotted name
/// of `[a-z0-9-]` labels. Rejects empty patterns, bare `*`, embedded
/// wildcards, and empty labels.
pub fn validate_pattern(pattern: &str) -> Result<(), String> {
    let trimmed = pattern.trim();
    if trimmed.is_empty() {
        return Err("pattern must not be empty".to_owned());
    }
    let body = trimmed.strip_prefix("*.").unwrap_or(trimmed);
    if body.is_empty() {
        return Err("pattern must name a domain, not a bare wildcard".to_owned());
    }
    for label in body.split('.') {
        if label.is_empty() {
            return Err(format!("pattern '{pattern}' has an empty label"));
        }
        if !label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(format!(
                "pattern '{pattern}' contains an invalid label '{label}'"
            ));
        }
    }
    Ok(())
}
