use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Where traffic for a device should be routed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RoutingTarget {
    Tunnel { tunnel_id: Uuid },
    Direct,
    Default,
}

impl RoutingTarget {
    /// Interpret a persisted global default-policy string into a concrete
    /// target. `"direct"` → [`Direct`](Self::Direct); a tunnel UUID →
    /// [`Tunnel`](Self::Tunnel); any other (legacy/corrupt) value falls back to
    /// `Direct`. The single source of truth shared by the routing engine
    /// (`RoutingService::resolve_target`) and the Network-Zone gate
    /// (`DeviceService`), so the two never disagree about what `Default` means.
    #[must_use]
    pub fn from_default_policy(policy: &str) -> Self {
        if policy == "direct" {
            Self::Direct
        } else if let Ok(tunnel_id) = policy.parse::<Uuid>() {
            Self::Tunnel { tunnel_id }
        } else {
            Self::Direct
        }
    }
}

/// Who created a routing rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuleCreator {
    Admin,
    User,
}

/// A routing rule that binds a device to a routing target.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RoutingRule {
    pub device_id: Uuid,
    pub target: RoutingTarget,
    pub created_by: RuleCreator,
}
