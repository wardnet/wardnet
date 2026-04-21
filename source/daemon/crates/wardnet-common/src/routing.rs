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
