//! Network-Zone packet enforcer (issue #736 — Phase 1 / CI-2).
//!
//! Translates each device's [`NetworkZone`] into nftables rules that make the
//! zone *mean something* on a flat shared subnet, even when Wardnet is not the
//! DHCP server:
//!
//! - **Egress gate** — a device may only egress via a routing-target *kind* its
//!   zone permits. A forbidden path is dropped in the forward chain (`wg_ward*`
//!   for a tunnel-forbidding zone, the LAN/WAN interface for a direct-forbidding
//!   one).
//! - **Admin-UI gate** — a zone with `admin_ui_reachable = false` has its
//!   devices refused (TCP reset) on the Pi's :443/:7411 admin surfaces while
//!   DNS (:53) and DHCP still pass.
//!
//! Rules are keyed by device IP via nftables comment UDATA
//! (`wardnet:zone:<kind>:<ip>`), so they survive daemon restarts, and are
//! live-reloaded on zone/device events with no restart. After each live change
//! the enforcer flushes conntrack for the affected IP so already-open flows are
//! re-evaluated immediately.
//!
//! The enforcer also closes the one edge the #735 write-time zone gate cannot
//! catch: a change to the **global default routing policy** re-resolves every
//! `Default`-ruled device at once, which can bind a device to a target its zone
//! forbids. On [`WardnetEvent::DefaultPolicyChanged`] (and on startup) the
//! enforcer unbinds any such device back to direct via the routing service.
//!
//! **Honest limitation:** same-subnet peer↔peer traffic is *not* affected here.
//! On a flat L2 segment the daemon never sees it; peer isolation is delegated to
//! the access point (or the `IsolateMembers` rung, #737). See
//! `adr-network-zone-enforcement.md` and the epic-#244 ADR.
//!
//! [`WardnetEvent::DefaultPolicyChanged`]: wardnet_common::event::WardnetEvent::DefaultPolicyChanged

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::device::Device;
use wardnet_common::network_zone::{AllowedTargetKind, NetworkZone};
use wardnet_common::routing::RoutingTarget;

use wardnetd_data::repository::{DeviceRepository, NetworkZoneRepository, SystemConfigRepository};

use crate::auth_context;
use crate::error::AppError;
use crate::routing::RoutingService;
use crate::routing::firewall::{FirewallManager, ZoneRules};
use crate::routing::policy_router::PolicyRouter;

/// Applies and live-reloads per-device Network-Zone nftables enforcement.
///
/// All methods are admin-guarded and are driven by `ZoneEnforcementListener`
/// (in `wardnetd`) from the domain event bus, plus a startup [`Self::reconcile`].
#[async_trait]
pub trait ZoneEnforcementService: Send + Sync {
    /// Reconcile kernel zone rules with the database on startup.
    ///
    /// Installs every current device's zone rules, drops orphaned rules for IPs
    /// no longer backed by a device, and clamps any forbidden `Default`-policy
    /// binding to direct. Must run *after* `RoutingService::reconcile` (which
    /// (re)creates + flushes the shared `wardnet` table, including the input
    /// chain, and applies stored routing rules).
    async fn reconcile(&self) -> Result<(), AppError>;

    /// Recompute and re-apply zone rules for a single device (its zone, IP, or
    /// discovery changed).
    async fn apply_device(&self, device_id: Uuid) -> Result<(), AppError>;

    /// Recompute and re-apply zone rules for every device currently in a zone
    /// (the zone's `allowed_targets` / `admin_ui_reachable` changed).
    async fn apply_zone(&self, zone_id: Uuid) -> Result<(), AppError>;

    /// Re-key a device's zone rules after its IP changed: drop the old-IP rules
    /// and install rules for the new IP.
    async fn handle_ip_change(
        &self,
        device_id: Uuid,
        old_ip: &str,
        new_ip: &str,
    ) -> Result<(), AppError>;

    /// Remove a departed device's zone rules.
    async fn remove_device(&self, device_id: Uuid, last_ip: &str) -> Result<(), AppError>;

    /// React to a global default-policy change by unbinding any `Default`-ruled
    /// device whose zone forbids the newly-resolved target kind (see the module
    /// docs). Packet rules are unaffected — they depend only on the zone.
    async fn handle_default_policy_changed(&self, policy: &str) -> Result<(), AppError>;
}

/// Default [`ZoneEnforcementService`] implementation.
///
/// Reads zones + devices from their repositories, installs nftables rules via
/// the shared [`FirewallManager`], flushes conntrack via the shared
/// [`PolicyRouter`], and calls back into the [`RoutingService`] to unbind
/// forbidden default-policy bindings.
pub struct ZoneEnforcementServiceImpl {
    zones: Arc<dyn NetworkZoneRepository>,
    devices: Arc<dyn DeviceRepository>,
    system_config: Arc<dyn SystemConfigRepository>,
    firewall: Arc<dyn FirewallManager>,
    policy_router: Arc<dyn PolicyRouter>,
    routing: Arc<dyn RoutingService>,
    /// WAN-facing egress interface, used for the direct-egress drop.
    lan_interface: String,
}

impl ZoneEnforcementServiceImpl {
    /// Create a new enforcer from its repositories, shared backends, and the
    /// routing service it clamps forbidden default bindings through.
    #[must_use]
    pub fn new(
        zones: Arc<dyn NetworkZoneRepository>,
        devices: Arc<dyn DeviceRepository>,
        system_config: Arc<dyn SystemConfigRepository>,
        firewall: Arc<dyn FirewallManager>,
        policy_router: Arc<dyn PolicyRouter>,
        routing: Arc<dyn RoutingService>,
        lan_interface: String,
    ) -> Self {
        Self {
            zones,
            devices,
            system_config,
            firewall,
            policy_router,
            routing,
            lan_interface,
        }
    }

    /// The packet policy a zone implies: its `allowed_targets` become the
    /// egress allows, and its `admin_ui_reachable` flag the admin-UI allow.
    fn zone_rules(zone: &NetworkZone) -> ZoneRules {
        ZoneRules {
            allow_direct: zone.permits_kind(AllowedTargetKind::Direct),
            allow_tunnel: zone.permits_kind(AllowedTargetKind::Tunnel),
            admin_ui_reachable: zone.admin_ui_reachable,
        }
    }

    /// Install a device IP's zone rules. On a live change (`flush = true`) the
    /// device's conntrack is flushed so already-open flows re-evaluate at once;
    /// on bulk reconcile (`flush = false`) it is skipped — the table was just
    /// flushed and there are no meaningful flows to tear down.
    async fn apply_one(
        &self,
        device_ip: &str,
        zone: &NetworkZone,
        flush: bool,
    ) -> Result<(), AppError> {
        let rules = Self::zone_rules(zone);
        self.firewall
            .apply_zone_rules(device_ip, rules, &self.lan_interface)
            .await
            .map_err(AppError::Internal)?;
        if flush && let Err(e) = self.policy_router.flush_conntrack(device_ip).await {
            tracing::warn!(
                error = %e,
                device_ip,
                "zone enforcer: failed to flush conntrack after apply (live flows may lag)"
            );
        }
        Ok(())
    }

    /// Load a device and its zone. Returns `None` (with a log) if the device or
    /// its referenced zone is gone — a benign race during discovery/teardown.
    async fn load_device_and_zone(
        &self,
        device_id: Uuid,
    ) -> Result<Option<(Device, NetworkZone)>, AppError> {
        let Some(device) = self
            .devices
            .find_by_id(&device_id.to_string())
            .await
            .map_err(AppError::Internal)?
        else {
            tracing::debug!(device_id = %device_id, "zone enforcer: device not found, skipping");
            return Ok(None);
        };
        let Some(zone) = self
            .zones
            .find_by_id(&device.zone_id.to_string())
            .await
            .map_err(AppError::Internal)?
        else {
            tracing::warn!(
                device_id = %device_id,
                zone_id = %device.zone_id,
                "zone enforcer: device references unknown zone, skipping"
            );
            return Ok(None);
        };
        Ok(Some((device, zone)))
    }

    /// Unbind every `Default`-ruled device whose zone forbids the target `policy`
    /// resolves to, pinning it back to direct through the routing service. The
    /// one edge the #735 write-time gate cannot catch (a policy flip re-resolves
    /// all `Default` rules at once). A device whose zone forbids *direct* too
    /// (a tunnel-only zone under a `direct` policy) has no valid fallback, so its
    /// binding is left for the forward-chain drop to handle.
    async fn clamp_default_bindings(&self, policy: &str) -> Result<(), AppError> {
        let resolved = RoutingTarget::from_default_policy(policy);
        let Some(kind) = AllowedTargetKind::of_target(&resolved) else {
            // `from_default_policy` never yields `Default`; unreachable.
            return Ok(());
        };
        let devices = self.devices.find_all().await.map_err(AppError::Internal)?;
        let mut clamped = 0u32;
        for device in &devices {
            let rule = match self
                .devices
                .find_rule_for_device(&device.id.to_string())
                .await
            {
                Ok(Some(rule)) => rule,
                Ok(None) => continue,
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        device_id = %device.id,
                        "zone enforcer: failed to load rule while clamping default bindings"
                    );
                    continue;
                }
            };
            // Only stored `Default` rules re-resolve on a policy flip; explicit
            // Direct/Tunnel rules were already validated by the #735 write gate.
            if !matches!(rule.target, RoutingTarget::Default) {
                continue;
            }
            let Some(zone) = self
                .zones
                .find_by_id(&device.zone_id.to_string())
                .await
                .ok()
                .flatten()
            else {
                continue;
            };
            if zone.permits_kind(kind) {
                continue;
            }
            if !zone.permits_kind(AllowedTargetKind::Direct) {
                tracing::warn!(
                    device_id = %device.id,
                    zone = %zone.name,
                    policy,
                    "zone enforcer: default policy forbidden by zone which also forbids direct; \
                     leaving binding for the packet-layer drop"
                );
                continue;
            }
            tracing::info!(
                device_id = %device.id,
                zone = %zone.name,
                policy,
                "zone enforcer: clamping Default binding to direct (zone forbids resolved target)"
            );
            if let Err(e) = self
                .routing
                .apply_rule_for_device(device.id, &RoutingTarget::Direct)
                .await
            {
                tracing::warn!(
                    error = %e,
                    device_id = %device.id,
                    "zone enforcer: failed to clamp default binding to direct"
                );
            } else {
                clamped += 1;
            }
        }
        if clamped > 0 {
            tracing::info!(
                clamped,
                "zone enforcer: clamped default-policy bindings to direct"
            );
        }
        Ok(())
    }

    /// Read the persisted global default policy, defaulting to `"direct"`.
    async fn current_default_policy(&self) -> Result<String, AppError> {
        Ok(self
            .system_config
            .get_default_policy()
            .await
            .map_err(AppError::Internal)?
            .unwrap_or_else(|| "direct".to_owned()))
    }
}

#[async_trait]
impl ZoneEnforcementService for ZoneEnforcementServiceImpl {
    async fn reconcile(&self) -> Result<(), AppError> {
        auth_context::require_admin()?;
        tracing::info!("reconciling network-zone enforcement with kernel");

        let devices = self.devices.find_all().await.map_err(AppError::Internal)?;
        let all_zones = self.zones.find_all().await.map_err(AppError::Internal)?;
        let zone_by_id: HashMap<Uuid, &NetworkZone> = all_zones.iter().map(|z| (z.id, z)).collect();

        let mut applied = 0u32;
        let mut live_ips: HashSet<String> = HashSet::with_capacity(devices.len());
        for device in &devices {
            live_ips.insert(device.last_ip.clone());
            let Some(zone) = zone_by_id.get(&device.zone_id) else {
                tracing::warn!(
                    device_id = %device.id,
                    zone_id = %device.zone_id,
                    "zone enforcer: device references unknown zone during reconcile, skipping"
                );
                continue;
            };
            if let Err(e) = self.apply_one(&device.last_ip, zone, false).await {
                tracing::warn!(
                    error = %e,
                    device_id = %device.id,
                    "zone enforcer: failed to apply zone rules during reconcile"
                );
            } else {
                applied += 1;
            }
        }

        // Drop orphaned zone rules for IPs no longer backed by a device.
        match self.firewall.list_zone_rule_ips().await {
            Ok(ips) => {
                let mut orphans = 0u32;
                for ip in ips {
                    if !live_ips.contains(&ip) {
                        tracing::info!(device_ip = %ip, "zone enforcer: removing orphaned zone rules");
                        if let Err(e) = self.firewall.remove_zone_rules(&ip).await {
                            tracing::warn!(
                                error = %e,
                                device_ip = %ip,
                                "zone enforcer: failed to remove orphaned zone rules"
                            );
                        } else {
                            orphans += 1;
                        }
                    }
                }
                if orphans > 0 {
                    tracing::info!(orphans, "zone enforcer: removed orphaned zone rules");
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "zone enforcer: failed to list zone rules for orphan cleanup");
            }
        }

        // Close the default-policy caveat on startup: a policy set while the
        // daemon was down (or a device booting into a forbidden Default binding)
        // is clamped now, after RoutingService::reconcile applied stored rules.
        let policy = self.current_default_policy().await?;
        self.clamp_default_bindings(&policy).await?;

        tracing::info!(
            applied,
            total_devices = devices.len(),
            "network-zone enforcement reconcile complete"
        );
        Ok(())
    }

    async fn apply_device(&self, device_id: Uuid) -> Result<(), AppError> {
        auth_context::require_admin()?;
        tracing::debug!(device_id = %device_id, "zone enforcer: apply_device");
        if let Some((device, zone)) = self.load_device_and_zone(device_id).await? {
            self.apply_one(&device.last_ip, &zone, true).await?;
        }
        Ok(())
    }

    async fn apply_zone(&self, zone_id: Uuid) -> Result<(), AppError> {
        auth_context::require_admin()?;
        tracing::debug!(zone_id = %zone_id, "zone enforcer: apply_zone");
        let Some(zone) = self
            .zones
            .find_by_id(&zone_id.to_string())
            .await
            .map_err(AppError::Internal)?
        else {
            // Zone deleted — its (now unreferenced) members will be re-applied
            // when they are reassigned; nothing to install here.
            tracing::debug!(zone_id = %zone_id, "zone enforcer: zone not found, skipping");
            return Ok(());
        };
        let devices = self.devices.find_all().await.map_err(AppError::Internal)?;
        let mut applied = 0u32;
        for device in &devices {
            if device.zone_id != zone_id {
                continue;
            }
            if let Err(e) = self.apply_one(&device.last_ip, &zone, true).await {
                tracing::warn!(
                    error = %e,
                    device_id = %device.id,
                    "zone enforcer: failed to apply zone rules for member"
                );
            } else {
                applied += 1;
            }
        }
        tracing::debug!(zone_id = %zone_id, applied, "zone enforcer: apply_zone complete");
        Ok(())
    }

    async fn handle_ip_change(
        &self,
        device_id: Uuid,
        old_ip: &str,
        new_ip: &str,
    ) -> Result<(), AppError> {
        auth_context::require_admin()?;
        tracing::debug!(device_id = %device_id, old_ip, new_ip, "zone enforcer: handle_ip_change");
        // Drop the stale-IP rules first so they never outlive the device's move.
        self.firewall
            .remove_zone_rules(old_ip)
            .await
            .map_err(AppError::Internal)?;
        if let Some((_device, zone)) = self.load_device_and_zone(device_id).await? {
            // Key on the event's new IP rather than the row's `last_ip`, which
            // may not have been observed yet.
            self.apply_one(new_ip, &zone, true).await?;
        }
        Ok(())
    }

    async fn remove_device(&self, device_id: Uuid, last_ip: &str) -> Result<(), AppError> {
        auth_context::require_admin()?;
        tracing::debug!(device_id = %device_id, last_ip, "zone enforcer: remove_device");
        self.firewall
            .remove_zone_rules(last_ip)
            .await
            .map_err(AppError::Internal)?;
        Ok(())
    }

    async fn handle_default_policy_changed(&self, policy: &str) -> Result<(), AppError> {
        auth_context::require_admin()?;
        tracing::debug!(policy, "zone enforcer: handle_default_policy_changed");
        // Packet rules depend only on the zone, so a policy flip changes no zone
        // rule — only which target a device's `Default` rule resolves to. Clamp
        // any now-forbidden binding back to direct.
        self.clamp_default_bindings(policy).await
    }
}
