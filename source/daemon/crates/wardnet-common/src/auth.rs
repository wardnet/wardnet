use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Access role for API requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Admin,
    Public,
}

/// Identity and authorization context for the current request.
///
/// Set by API middleware and made available to services via
/// `tokio::task_local!`. Admin endpoints populate [`Admin`](Self::Admin),
/// unauthenticated self-service endpoints populate [`Device`](Self::Device)
/// with the caller's MAC address, and requests with no identified caller
/// use [`Anonymous`](Self::Anonymous).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthContext {
    /// Authenticated admin user.
    Admin {
        /// The UUID of the authenticated admin account.
        admin_id: Uuid,
    },
    /// Self-service caller identified by their device MAC address.
    Device {
        /// The MAC address of the caller's device.
        mac: String,
    },
    /// No identity resolved (e.g. unknown IP, public info endpoints).
    Anonymous,
}

impl AuthContext {
    /// Returns `true` if the context represents an admin.
    #[must_use]
    pub fn is_admin(&self) -> bool {
        matches!(self, Self::Admin { .. })
    }

    /// Returns the device MAC if this is a [`Device`](Self::Device) context.
    #[must_use]
    pub fn device_mac(&self) -> Option<&str> {
        match self {
            Self::Device { mac } => Some(mac),
            _ => None,
        }
    }
}

/// An authenticated admin session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Uuid,
    pub admin_id: Uuid,
    pub token_hash: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

/// A stored API key record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyRecord {
    pub id: Uuid,
    pub label: String,
    pub key_hash: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}
