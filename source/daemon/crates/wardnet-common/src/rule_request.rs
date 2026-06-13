//! Device rule requests — a lightweight "ask the admin" inbox.
//!
//! A household user (identified by device IP, no login) can request that a
//! domain be blocked or allowed. Admins list pending requests and approve or
//! reject them. Approval records the decision only; applying the actual DNS
//! filter rule is a manual admin step (auto-apply is a deferred follow-up).

use serde::{Deserialize, Serialize};

/// What the user is asking for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuleRequestKind {
    /// Add the domain to a blocklist / custom block rule.
    Block,
    /// Allow a currently-blocked domain (allowlist entry).
    Allow,
}

impl RuleRequestKind {
    /// Stable database/string representation.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::Allow => "allow",
        }
    }

    /// Parse from the database string; unknown values fall back to `Block`.
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s {
            "allow" => Self::Allow,
            _ => Self::Block,
        }
    }
}

/// Lifecycle state of a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuleRequestStatus {
    /// Awaiting an admin decision.
    Pending,
    /// Admin accepted the request.
    Approved,
    /// Admin declined the request.
    Rejected,
}

impl RuleRequestStatus {
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

/// A single rule request row.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DeviceRuleRequest {
    pub id: String,
    pub device_id: String,
    pub kind: RuleRequestKind,
    pub domain: String,
    pub reason: Option<String>,
    pub status: RuleRequestStatus,
    pub created_at: String,
    pub decided_at: Option<String>,
    pub decided_by: Option<String>,
}

/// Request body for `POST /api/devices/me/rule-requests`.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CreateRuleRequestRequest {
    pub kind: RuleRequestKind,
    pub domain: String,
    pub reason: Option<String>,
}

/// Request body for `PATCH /api/rule-requests/{id}` (admin decision). Only
/// `approved` / `rejected` are accepted — a request cannot be set back to
/// `pending`.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DecideRuleRequestRequest {
    pub status: RuleRequestStatus,
}
