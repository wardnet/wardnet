//! Repository for the routing-profile subsystem.
//!
//! Owns CRUD over the three tables seeded by the
//! `20260721000000_routing_profiles.sql` migration: `routing_profiles` (named
//! profiles), `routing_profile_rules` (domain → target rules, FK onto a
//! profile), and `routing_device_profile` (the ordered many-to-many device →
//! profile assignment).
//!
//! Mirrors [`crate::repository::dns_local::DnsLocalRepository`]'s
//! one-cohesive-trait shape, and the assignment methods mirror
//! `DnsFilterRepository::set_device_profiles`, adding a device-local `position`
//! so a device's profiles carry a priority order.

use async_trait::async_trait;
use uuid::Uuid;

use wardnet_common::routing_profile::{DomainRoutingRule, DomainRoutingTarget, RoutingProfile};

/// Insert struct for a routing profile.
#[derive(Debug, Clone)]
pub struct RoutingProfileRow {
    pub id: String,
    pub name: String,
}

/// Partial update for a profile. `None` leaves a field unchanged.
#[derive(Debug, Clone, Default)]
pub struct RoutingProfileUpdate {
    pub name: Option<String>,
}

/// Insert struct for a domain routing rule.
#[derive(Debug, Clone)]
pub struct RoutingRuleRow {
    pub id: String,
    pub profile_id: Uuid,
    pub pattern: String,
    pub target: DomainRoutingTarget,
    pub enabled: bool,
}

/// Partial update for a domain routing rule. `None` leaves a field unchanged.
#[derive(Debug, Clone, Default)]
pub struct RoutingRuleUpdate {
    pub pattern: Option<String>,
    pub target: Option<DomainRoutingTarget>,
    pub enabled: Option<bool>,
}

/// A device's ordered routing-profile assignment, as needed to compile the
/// per-device routing contexts the DNS pipeline consults.
#[derive(Debug, Clone)]
pub struct DeviceAssignment {
    pub device_id: Uuid,
    /// Assigned profile ids in priority order (lowest `position` first).
    pub profile_ids: Vec<Uuid>,
}

#[async_trait]
pub trait RoutingProfileRepository: Send + Sync {
    // ── Profiles ────────────────────────────────────────────────────────

    async fn list_profiles(&self) -> anyhow::Result<Vec<RoutingProfile>>;
    async fn get_profile(&self, id: Uuid) -> anyhow::Result<Option<RoutingProfile>>;
    async fn create_profile(&self, row: &RoutingProfileRow) -> anyhow::Result<RoutingProfile>;
    /// Update mutable fields on a profile. Returns the updated profile, or
    /// `None` if no row matched.
    async fn update_profile(
        &self,
        id: Uuid,
        update: &RoutingProfileUpdate,
    ) -> anyhow::Result<Option<RoutingProfile>>;
    /// Delete a profile. Returns `Ok(false)` if no row matched. Its rules and
    /// device assignments cascade away (`ON DELETE CASCADE`).
    async fn delete_profile(&self, id: Uuid) -> anyhow::Result<bool>;

    // ── Rules ───────────────────────────────────────────────────────────

    async fn list_rules(&self, profile_id: Uuid) -> anyhow::Result<Vec<DomainRoutingRule>>;
    /// Every rule across every profile — used by the runtime to compile the
    /// per-device routing view in one pass.
    async fn list_all_rules(&self) -> anyhow::Result<Vec<DomainRoutingRule>>;
    async fn get_rule(&self, id: Uuid) -> anyhow::Result<Option<DomainRoutingRule>>;
    async fn create_rule(&self, row: &RoutingRuleRow) -> anyhow::Result<DomainRoutingRule>;
    async fn update_rule(
        &self,
        id: Uuid,
        update: &RoutingRuleUpdate,
    ) -> anyhow::Result<Option<DomainRoutingRule>>;
    async fn delete_rule(&self, id: Uuid) -> anyhow::Result<bool>;

    // ── Device assignment (ordered) ─────────────────────────────────────

    /// A device's assigned profile ids in priority order.
    async fn get_device_profiles(&self, device_id: Uuid) -> anyhow::Result<Vec<Uuid>>;
    /// Atomically replace a device's assignment. The slice order becomes the
    /// stored `position` (index 0 = highest priority).
    async fn set_device_profiles(
        &self,
        device_id: Uuid,
        profile_ids: &[Uuid],
    ) -> anyhow::Result<()>;
    /// Every device that has at least one profile assigned, with its ordered
    /// profile ids. Drives the runtime context refresh.
    async fn list_device_assignments(&self) -> anyhow::Result<Vec<DeviceAssignment>>;
}
