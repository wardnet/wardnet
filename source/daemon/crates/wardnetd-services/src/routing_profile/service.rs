//! Routing-profile service: CRUD over profiles, rules and device assignments,
//! plus the post-resolution hook the DNS server calls to enforce a matched
//! domain's route.
//!
//! Mirrors the DNS filter profile service in shape (profile catalog + rules +
//! ordered device assignment), but a matched rule yields a routing decision
//! that is enforced by the [`crate::routing::RoutingService`] rather than a
//! filter action. Enforcement is decoupled from the DNS hot path: a match is
//! pushed onto a channel drained by [`super::runner::DomainRouteRunner`].

use std::net::IpAddr;
use std::sync::Arc;

use arc_swap::ArcSwap;
use async_trait::async_trait;
use tokio::sync::mpsc;
use uuid::Uuid;

use wardnet_common::routing_profile::{
    DomainRoutingRule, DomainRoutingTarget, RoutingProfile, validate_pattern,
};
use wardnetd_data::repository::{
    RoutingProfileRepository, RoutingProfileRow, RoutingProfileUpdate, RoutingRuleRow,
    RoutingRuleUpdate,
};

use crate::TunnelService;
use crate::auth_context;
use crate::error::AppError;

use super::view::{CompiledRule, DeviceRoutingContext, RoutingView};

/// A resolved-domain match, queued for the [`super::runner::DomainRouteRunner`]
/// to enforce off the DNS hot path.
#[derive(Debug, Clone)]
pub struct DomainRouteRequest {
    pub device_ip: IpAddr,
    pub resolved_ips: Vec<IpAddr>,
    pub target: DomainRoutingTarget,
    pub ttl_secs: u32,
}

#[async_trait]
pub trait RoutingProfileService: Send + Sync {
    // ── Profiles ────────────────────────────────────────────────────────
    async fn list_profiles(&self) -> Result<Vec<RoutingProfile>, AppError>;
    async fn get_profile(&self, id: Uuid) -> Result<RoutingProfile, AppError>;
    async fn create_profile(&self, name: &str) -> Result<RoutingProfile, AppError>;
    async fn rename_profile(&self, id: Uuid, name: &str) -> Result<RoutingProfile, AppError>;
    async fn delete_profile(&self, id: Uuid) -> Result<(), AppError>;

    // ── Rules ───────────────────────────────────────────────────────────
    async fn list_rules(&self, profile_id: Uuid) -> Result<Vec<DomainRoutingRule>, AppError>;
    async fn create_rule(
        &self,
        profile_id: Uuid,
        pattern: &str,
        target: DomainRoutingTarget,
        enabled: bool,
    ) -> Result<DomainRoutingRule, AppError>;
    async fn update_rule(
        &self,
        id: Uuid,
        pattern: Option<String>,
        target: Option<DomainRoutingTarget>,
        enabled: Option<bool>,
    ) -> Result<DomainRoutingRule, AppError>;
    async fn delete_rule(&self, id: Uuid) -> Result<(), AppError>;

    // ── Device assignment (ordered) ─────────────────────────────────────
    async fn get_device_profiles(&self, device_id: Uuid) -> Result<Vec<Uuid>, AppError>;
    async fn set_device_profiles(
        &self,
        device_id: Uuid,
        profile_ids: &[Uuid],
    ) -> Result<(), AppError>;
    /// The device ids a profile is assigned to. Reverse of
    /// [`Self::get_device_profiles`], for the profile page's "used by" view.
    async fn list_profile_devices(&self, profile_id: Uuid) -> Result<Vec<Uuid>, AppError>;

    /// Rebuild the hot-path routing view from the database. Called once at
    /// startup by the runner and internally after every mutation. No auth guard
    /// — internal.
    async fn refresh_view(&self) -> Result<(), AppError>;

    /// Post-resolution hook for the DNS server. If `device_id` has a routing
    /// profile whose rule matches `name`, the resolved IPv4 destinations are
    /// queued for enforcement. Cheap and non-blocking (an in-memory lookup plus
    /// a channel `try_send`); safe to call on every answered query.
    fn note_resolution(
        &self,
        device_id: Uuid,
        device_ip: IpAddr,
        name: &str,
        answer_ips: &[IpAddr],
        ttl_secs: u32,
    );
}

pub struct RoutingProfileServiceImpl {
    repo: Arc<dyn RoutingProfileRepository>,
    tunnels: Arc<dyn TunnelService>,
    /// Enforcement sink drained by the domain-route runner.
    sink: mpsc::Sender<DomainRouteRequest>,
    /// Lock-free compiled view consulted by [`Self::note_resolution`].
    view: Arc<ArcSwap<RoutingView>>,
}

impl RoutingProfileServiceImpl {
    #[must_use]
    pub fn new(
        repo: Arc<dyn RoutingProfileRepository>,
        tunnels: Arc<dyn TunnelService>,
        sink: mpsc::Sender<DomainRouteRequest>,
    ) -> Self {
        Self {
            repo,
            tunnels,
            sink,
            view: Arc::new(ArcSwap::from_pointee(RoutingView::new())),
        }
    }

    /// Validate that a rule's target is usable: a tunnel target must reference an
    /// existing tunnel.
    async fn validate_target(&self, target: &DomainRoutingTarget) -> Result<(), AppError> {
        if let DomainRoutingTarget::Tunnel { tunnel_id } = target {
            // `get_tunnel` returns `NotFound` for an unknown id; surface it as a
            // bad request against the rule rather than a bare 404.
            self.tunnels.get_tunnel(*tunnel_id).await.map_err(|_| {
                AppError::BadRequest(format!(
                    "routing rule references unknown tunnel {tunnel_id}"
                ))
            })?;
        }
        Ok(())
    }

    fn map_validate_pattern(pattern: &str) -> Result<(), AppError> {
        validate_pattern(pattern).map_err(AppError::BadRequest)
    }
}

#[async_trait]
impl RoutingProfileService for RoutingProfileServiceImpl {
    async fn list_profiles(&self) -> Result<Vec<RoutingProfile>, AppError> {
        auth_context::require_admin()?;
        self.repo.list_profiles().await.map_err(AppError::Internal)
    }

    async fn get_profile(&self, id: Uuid) -> Result<RoutingProfile, AppError> {
        auth_context::require_admin()?;
        self.repo
            .get_profile(id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound(format!("routing profile {id} not found")))
    }

    async fn create_profile(&self, name: &str) -> Result<RoutingProfile, AppError> {
        auth_context::require_admin()?;
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::BadRequest(
                "profile name must not be empty".to_owned(),
            ));
        }
        let row = RoutingProfileRow {
            id: Uuid::new_v4().to_string(),
            name: name.to_owned(),
        };
        let profile = self
            .repo
            .create_profile(&row)
            .await
            .map_err(map_unique("a profile with that name already exists"))?;
        self.refresh_view().await?;
        Ok(profile)
    }

    async fn rename_profile(&self, id: Uuid, name: &str) -> Result<RoutingProfile, AppError> {
        auth_context::require_admin()?;
        let name = name.trim();
        if name.is_empty() {
            return Err(AppError::BadRequest(
                "profile name must not be empty".to_owned(),
            ));
        }
        let updated = self
            .repo
            .update_profile(
                id,
                &RoutingProfileUpdate {
                    name: Some(name.to_owned()),
                },
            )
            .await
            .map_err(map_unique("a profile with that name already exists"))?
            .ok_or_else(|| AppError::NotFound(format!("routing profile {id} not found")))?;
        self.refresh_view().await?;
        Ok(updated)
    }

    async fn delete_profile(&self, id: Uuid) -> Result<(), AppError> {
        auth_context::require_admin()?;
        if !self
            .repo
            .delete_profile(id)
            .await
            .map_err(AppError::Internal)?
        {
            return Err(AppError::NotFound(format!(
                "routing profile {id} not found"
            )));
        }
        self.refresh_view().await?;
        Ok(())
    }

    async fn list_rules(&self, profile_id: Uuid) -> Result<Vec<DomainRoutingRule>, AppError> {
        auth_context::require_admin()?;
        // Confirm the profile exists so the caller gets a 404 rather than an
        // empty list for a bad id.
        self.get_profile(profile_id).await?;
        self.repo
            .list_rules(profile_id)
            .await
            .map_err(AppError::Internal)
    }

    async fn create_rule(
        &self,
        profile_id: Uuid,
        pattern: &str,
        target: DomainRoutingTarget,
        enabled: bool,
    ) -> Result<DomainRoutingRule, AppError> {
        auth_context::require_admin()?;
        self.get_profile(profile_id).await?;
        let pattern = pattern.trim();
        Self::map_validate_pattern(pattern)?;
        self.validate_target(&target).await?;
        let row = RoutingRuleRow {
            id: Uuid::new_v4().to_string(),
            profile_id,
            pattern: pattern.to_owned(),
            target,
            enabled,
        };
        let rule = self.repo.create_rule(&row).await.map_err(map_unique(
            "a rule for that pattern already exists in this profile",
        ))?;
        self.refresh_view().await?;
        Ok(rule)
    }

    async fn update_rule(
        &self,
        id: Uuid,
        pattern: Option<String>,
        target: Option<DomainRoutingTarget>,
        enabled: Option<bool>,
    ) -> Result<DomainRoutingRule, AppError> {
        auth_context::require_admin()?;
        if let Some(pattern) = pattern.as_deref() {
            Self::map_validate_pattern(pattern.trim())?;
        }
        if let Some(target) = target.as_ref() {
            self.validate_target(target).await?;
        }
        let updated = self
            .repo
            .update_rule(
                id,
                &RoutingRuleUpdate {
                    pattern: pattern.map(|p| p.trim().to_owned()),
                    target,
                    enabled,
                },
            )
            .await
            .map_err(map_unique(
                "a rule for that pattern already exists in this profile",
            ))?
            .ok_or_else(|| AppError::NotFound(format!("routing rule {id} not found")))?;
        self.refresh_view().await?;
        Ok(updated)
    }

    async fn delete_rule(&self, id: Uuid) -> Result<(), AppError> {
        auth_context::require_admin()?;
        if !self
            .repo
            .delete_rule(id)
            .await
            .map_err(AppError::Internal)?
        {
            return Err(AppError::NotFound(format!("routing rule {id} not found")));
        }
        self.refresh_view().await?;
        Ok(())
    }

    async fn get_device_profiles(&self, device_id: Uuid) -> Result<Vec<Uuid>, AppError> {
        auth_context::require_admin()?;
        self.repo
            .get_device_profiles(device_id)
            .await
            .map_err(AppError::Internal)
    }

    async fn list_profile_devices(&self, profile_id: Uuid) -> Result<Vec<Uuid>, AppError> {
        auth_context::require_admin()?;
        // Confirm the profile exists so the caller gets a 404 rather than an
        // empty list for a bad id.
        self.get_profile(profile_id).await?;
        self.repo
            .list_profile_devices(profile_id)
            .await
            .map_err(AppError::Internal)
    }

    async fn set_device_profiles(
        &self,
        device_id: Uuid,
        profile_ids: &[Uuid],
    ) -> Result<(), AppError> {
        auth_context::require_admin()?;
        // Reject duplicates and unknown profiles up front so a bad assignment
        // never lands half-written.
        let mut seen = std::collections::HashSet::new();
        for id in profile_ids {
            if !seen.insert(*id) {
                return Err(AppError::BadRequest(format!(
                    "profile {id} assigned more than once"
                )));
            }
            if self
                .repo
                .get_profile(*id)
                .await
                .map_err(AppError::Internal)?
                .is_none()
            {
                return Err(AppError::BadRequest(format!(
                    "routing profile {id} does not exist"
                )));
            }
        }
        self.repo
            .set_device_profiles(device_id, profile_ids)
            .await
            .map_err(AppError::Internal)?;
        self.refresh_view().await?;
        Ok(())
    }

    async fn refresh_view(&self) -> Result<(), AppError> {
        // No auth guard — internal, called from mutations (already guarded) and
        // from the runner's startup bootstrap.
        let assignments = self
            .repo
            .list_device_assignments()
            .await
            .map_err(AppError::Internal)?;
        let all_rules = self
            .repo
            .list_all_rules()
            .await
            .map_err(AppError::Internal)?;

        // Group enabled rules by profile id.
        let mut by_profile: std::collections::HashMap<Uuid, Vec<CompiledRule>> =
            std::collections::HashMap::new();
        for rule in all_rules {
            if !rule.enabled {
                continue;
            }
            by_profile
                .entry(rule.profile_id)
                .or_default()
                .push(CompiledRule {
                    pattern: rule.pattern,
                    target: rule.target,
                });
        }

        let mut view = RoutingView::new();
        for assignment in assignments {
            let profiles = assignment
                .profile_ids
                .iter()
                .map(|pid| by_profile.get(pid).cloned().unwrap_or_default())
                .collect();
            view.insert(
                assignment.device_id,
                Arc::new(DeviceRoutingContext { profiles }),
            );
        }

        self.view.store(Arc::new(view));
        Ok(())
    }

    fn note_resolution(
        &self,
        device_id: Uuid,
        client_ip: IpAddr,
        name: &str,
        answer_ips: &[IpAddr],
        ttl_secs: u32,
    ) {
        let view = self.view.load();
        let Some(context) = view.get(&device_id) else {
            return;
        };
        let Some(target) = context.resolve(name) else {
            return;
        };
        // v1 enforces IPv4 only — the `ip rule` primitive is v4.
        let resolved_ips: Vec<IpAddr> =
            answer_ips.iter().copied().filter(IpAddr::is_ipv4).collect();
        if resolved_ips.is_empty() {
            return;
        }
        // Non-blocking: if the enforcement queue is full we drop this match; the
        // next resolution of the same name re-queues it.
        let _ = self.sink.try_send(DomainRouteRequest {
            device_ip: client_ip,
            resolved_ips,
            target: target.clone(),
            ttl_secs,
        });
    }
}

/// `CompiledRule` needs `Clone` so the per-profile groups can be shared into
/// each assigned device's context.
impl Clone for CompiledRule {
    fn clone(&self) -> Self {
        Self {
            pattern: self.pattern.clone(),
            target: self.target.clone(),
        }
    }
}

/// Map a `SQLite` `UNIQUE` violation surfaced through the repo's `anyhow` error
/// into a 409 with `msg`; anything else stays a 500.
fn map_unique(msg: &'static str) -> impl Fn(anyhow::Error) -> AppError {
    move |e| {
        if e.to_string().contains("UNIQUE constraint failed") {
            AppError::Conflict(msg.to_owned())
        } else {
            AppError::Internal(e)
        }
    }
}
