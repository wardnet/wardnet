//! Device access requests — the household "ask the admin" inbox.
//!
//! A household member (identified by device IP, no login — the user PWA stays
//! device-keyed, see ADR-0031) asks an admin for something their device cannot
//! grant itself: a domain blocked or allowed, or **Private DNS** access.
//!
//! What approval *does* is per kind, dispatched through the approver registry
//! in `wardnetd-services` rather than branched on here. A kind with no
//! registered approver is record-only — which is the state `Allow` / `Block`
//! are in: the admin still applies the DNS filter rule by hand. `PrivateDns`
//! has an approver, so approving one mints the grant.

use serde::{Deserialize, Serialize};

/// What the member is asking for.
///
/// `Allow` / `Block` carry a `domain`; `PrivateDns` carries no payload at all —
/// the request is fully determined by which device asked. The database enforces
/// that pairing with a `CHECK`, so a kind added here must decide which side of
/// it the new variant falls on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AccessRequestKind {
    /// Add the domain to a blocklist / custom block rule.
    Block,
    /// Allow a currently-blocked domain (allowlist entry).
    Allow,
    /// Grant this device Private DNS (issue #919, ADR-0029).
    PrivateDns,
}

impl AccessRequestKind {
    /// Stable database/string representation.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::Allow => "allow",
            Self::PrivateDns => "private_dns",
        }
    }

    /// Parse from the database string; unknown values fall back to `Block`.
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s {
            "allow" => Self::Allow,
            "private_dns" => Self::PrivateDns,
            _ => Self::Block,
        }
    }

    /// True when this kind's request names a domain.
    ///
    /// The single source of truth for the payload pairing the database `CHECK`
    /// also encodes — validation reads it rather than re-listing the variants.
    #[must_use]
    pub fn requires_domain(&self) -> bool {
        match self {
            Self::Block | Self::Allow => true,
            Self::PrivateDns => false,
        }
    }
}

/// Lifecycle state of a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AccessRequestStatus {
    /// Awaiting an admin decision.
    Pending,
    /// Admin accepted the request.
    Approved,
    /// Admin declined the request.
    Rejected,
}

impl AccessRequestStatus {
    /// Stable database/string representation.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
        }
    }

    /// Parse from the database string; unknown values fall back to `Pending`.
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s {
            "approved" => Self::Approved,
            "rejected" => Self::Rejected,
            _ => Self::Pending,
        }
    }

    /// True for a terminal admin decision (not `Pending`).
    #[must_use]
    pub fn is_decision(&self) -> bool {
        matches!(self, Self::Approved | Self::Rejected)
    }
}

/// A single access-request row.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DeviceAccessRequest {
    pub id: String,
    pub device_id: String,
    pub kind: AccessRequestKind,
    /// `None` for kinds that name no domain — see [`AccessRequestKind::requires_domain`].
    pub domain: Option<String>,
    pub reason: Option<String>,
    pub status: AccessRequestStatus,
    pub created_at: String,
    pub decided_at: Option<String>,
    pub decided_by: Option<String>,
}

/// Request body for `POST /api/devices/me/access-requests`.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateAccessRequestRequest {
    pub kind: AccessRequestKind,
    /// Required for `allow` / `block`, and rejected for kinds that take no
    /// domain — the service validates against
    /// [`AccessRequestKind::requires_domain`].
    pub domain: Option<String>,
    pub reason: Option<String>,
}

/// Kind-specific parameters an admin supplies when **approving**.
///
/// Approval is not uniform across kinds, so the decision body carries a typed
/// payload rather than a bag of optional fields. `PrivateDns` needs none — the
/// grant is fully determined by the requesting device — but the variant exists
/// so the wire shape is explicit rather than "absent means Private DNS".
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ApprovalParams {
    /// Approving a `private_dns` request takes no parameters.
    PrivateDns,
}

/// Request body for `PATCH /api/access-requests/{id}` (admin decision). Only
/// `approved` / `rejected` are accepted — a request cannot be set back to
/// `pending`.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DecideAccessRequestRequest {
    pub status: AccessRequestStatus,
    /// Kind-specific approval parameters. Ignored when rejecting.
    #[serde(default)]
    pub approval: Option<ApprovalParams>,
}
