use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A permitted routing-target *kind* a zone allows. Coarse: any tunnel, or direct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AllowedTargetKind {
    Direct,
    Tunnel,
}

/// Cross-zone isolation rung of the guarantee ladder (epic #244). Only the rungs
/// we have issues for exist; VLAN is a documented non-goal (ADR), not a variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ZoneStance {
    /// nftables egress + admin-UI gating only; peer isolation delegated to the AP. (CI-2 #736)
    SharedSubnet,
    /// Per-device /32 + proxy-ARP; requires wardnet-DHCP-mode. (CI-3 #737)
    IsolateMembers,
}

/// Whether a zone was seeded by the daemon or created by an admin.
/// Mirrors the existing DNS "Zone provenance" (system | manual).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ZoneProvenance {
    System,
    Manual,
}

/// A per-zone subnet (Phase 2 / DHCP-mode). Recorded-only in #735; always None on seeds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ZoneSubnet {
    /// CIDR, e.g. "10.44.0.0/24". Validated when set; unused by any Phase-1 code path.
    pub cidr: String,
}

/// A Network Zone: a named policy bucket a device belongs to (exactly one) that
/// gates the device's allowed routing targets, its reachability of the Pi's
/// admin surfaces, and (Phase 2+) its network isolation. See epic #244 and the
/// `adr-network-zone-isolation` ADR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[allow(clippy::struct_excessive_bools)]
pub struct NetworkZone {
    pub id: Uuid,
    pub name: String,
    pub provenance: ZoneProvenance,
    pub isolation_stance: ZoneStance,
    /// Routing-target kinds a device in this zone may pick. Empty is rejected on write.
    pub allowed_targets: Vec<AllowedTargetKind>,
    /// Within an isolate-members zone, also isolate same-zone peers. (CI-3 #737)
    pub member_isolation: bool,
    /// Per-zone subnet (CI-3 #737). None for all seeds.
    pub subnet: Option<ZoneSubnet>,
    /// May devices in this zone reach the Pi's admin surfaces? (intent-only in Phase 1)
    pub admin_ui_reachable: bool,
    /// The protected "home"/anchor zone. Exactly one true. Deletion-guarded.
    pub is_default: bool,
    /// Where freshly-discovered devices are assigned. Exactly one true.
    pub is_default_for_new: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl NetworkZone {
    /// Does this zone permit the given resolved concrete target kind?
    #[must_use]
    pub fn permits_kind(&self, kind: AllowedTargetKind) -> bool {
        self.allowed_targets.contains(&kind)
    }
}
