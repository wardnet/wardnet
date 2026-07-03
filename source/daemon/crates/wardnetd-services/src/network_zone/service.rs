use std::str::FromStr;
use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::api::{CreateNetworkZoneRequest, NetworkZoneView, UpdateNetworkZoneRequest};
use wardnet_common::event::WardnetEvent;
use wardnet_common::network_zone::{AllowedTargetKind, NetworkZone, ZoneProvenance};
use wardnet_common::routing::RoutingTarget;
use wardnetd_data::repository::{DeviceRepository, NetworkZoneRepository, SystemConfigRepository};

use crate::auth_context;
use crate::error::AppError;
use crate::event::EventPublisher;

/// Network Zones service (epic #244, issue #735).
///
/// CRUD over the policy-bucket catalog plus device→zone assignment. Every
/// method is admin-gated. Guard rails (system-zone / default / has-members
/// protection, name uniqueness) live here; the FK `ON DELETE RESTRICT` and the
/// partial unique indexes in the schema are the belt-and-suspenders backstop.
#[async_trait]
pub trait NetworkZoneService: Send + Sync {
    /// List all zones with their member counts.
    async fn list_zones(&self) -> Result<Vec<NetworkZoneView>, AppError>;

    /// Fetch a single zone with its member count.
    async fn get_zone(&self, id: Uuid) -> Result<NetworkZoneView, AppError>;

    /// Create a new (manual) zone.
    async fn create_zone(&self, req: CreateNetworkZoneRequest) -> Result<NetworkZone, AppError>;

    /// Partially update a zone. `is_default`/`is_default_for_new = Some(true)`
    /// promote this zone; `Some(false)` is rejected.
    async fn update_zone(
        &self,
        id: Uuid,
        req: UpdateNetworkZoneRequest,
    ) -> Result<NetworkZone, AppError>;

    /// Delete a zone. Rejected for system zones, the anchor zone, or zones with
    /// members.
    async fn delete_zone(&self, id: Uuid) -> Result<(), AppError>;

    /// Move the anchor (`is_default`) flag to the given zone.
    async fn set_default(&self, id: Uuid) -> Result<(), AppError>;

    /// Move the default-for-new flag to the given zone.
    async fn set_default_for_new(&self, id: Uuid) -> Result<(), AppError>;

    /// Reassign a device to a different zone.
    async fn assign_device(&self, device_id: Uuid, zone_id: Uuid) -> Result<(), AppError>;

    /// Read the new-device quarantine toggle (issue #738). When on, a
    /// freshly-discovered device triggers an admin push to approve it.
    async fn get_quarantine_new_devices(&self) -> Result<bool, AppError>;

    /// Enable or disable new-device quarantine (issue #738).
    async fn set_quarantine_new_devices(&self, enabled: bool) -> Result<(), AppError>;
}

/// Default implementation of [`NetworkZoneService`].
pub struct NetworkZoneServiceImpl {
    zones: Arc<dyn NetworkZoneRepository>,
    devices: Arc<dyn DeviceRepository>,
    system_config: Arc<dyn SystemConfigRepository>,
    events: Arc<dyn EventPublisher>,
}

impl NetworkZoneServiceImpl {
    /// Create a new service backed by the given repositories and event publisher.
    #[must_use]
    pub fn new(
        zones: Arc<dyn NetworkZoneRepository>,
        devices: Arc<dyn DeviceRepository>,
        system_config: Arc<dyn SystemConfigRepository>,
        events: Arc<dyn EventPublisher>,
    ) -> Self {
        Self {
            zones,
            devices,
            system_config,
            events,
        }
    }

    /// Resolve a device's *current* routing-rule kind (if it has a rule),
    /// resolving `Default` through the global policy — the same classification
    /// the write-time zone gate in `DeviceService` uses. `Ok(None)` means the
    /// device has no rule of its own (it follows the gateway default, which is
    /// itself already zone-checked at write time only — see the ADR caveat).
    async fn current_rule_kind(
        &self,
        device_id: &str,
    ) -> Result<Option<AllowedTargetKind>, AppError> {
        let Some(rule) = self
            .devices
            .find_rule_for_device(device_id)
            .await
            .map_err(AppError::Internal)?
        else {
            return Ok(None);
        };
        let concrete = match rule.target {
            RoutingTarget::Default => {
                let policy = self
                    .system_config
                    .get_default_policy()
                    .await
                    .map_err(AppError::Internal)?
                    .unwrap_or_else(|| "direct".to_owned());
                RoutingTarget::from_default_policy(&policy)
            }
            other => other,
        };
        Ok(Some(
            AllowedTargetKind::of_target(&concrete).unwrap_or(AllowedTargetKind::Direct),
        ))
    }

    /// Reject if any device in `zone_id` holds a routing rule whose resolved
    /// kind is not in `allowed` — used when a zone's `allowed_targets` is about
    /// to change. Batched: two queries total (all devices + all rules) plus one
    /// policy read, regardless of member count.
    async fn assert_members_conform(
        &self,
        zone_id: Uuid,
        allowed: &[AllowedTargetKind],
    ) -> Result<(), AppError> {
        let members: std::collections::HashSet<String> = self
            .devices
            .find_all()
            .await
            .map_err(AppError::Internal)?
            .into_iter()
            .filter(|d| d.zone_id == zone_id)
            .map(|d| d.id.to_string())
            .collect();
        if members.is_empty() {
            return Ok(());
        }
        let policy = self
            .system_config
            .get_default_policy()
            .await
            .map_err(AppError::Internal)?
            .unwrap_or_else(|| "direct".to_owned());
        let rules = self
            .devices
            .find_all_rules()
            .await
            .map_err(AppError::Internal)?;
        for rule in rules {
            if !members.contains(&rule.device_id.to_string()) {
                continue;
            }
            let concrete = match rule.target {
                RoutingTarget::Default => RoutingTarget::from_default_policy(&policy),
                other => other,
            };
            let kind = AllowedTargetKind::of_target(&concrete).unwrap_or(AllowedTargetKind::Direct);
            if !allowed.contains(&kind) {
                return Err(AppError::Conflict(format!(
                    "cannot narrow allowed_targets: device {} has a '{}' rule the new set forbids; \
                     change or clear its rule first",
                    rule.device_id,
                    kind.as_str()
                )));
            }
        }
        Ok(())
    }

    /// Fetch a zone by id or return `NotFound`.
    async fn require_zone(&self, id: Uuid) -> Result<NetworkZone, AppError> {
        self.zones
            .find_by_id(&id.to_string())
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound(format!("network zone {id} not found")))
    }

    /// Validate `allowed_targets` is non-empty and any subnet CIDR parses.
    fn validate_targets_and_subnet(
        allowed_targets: &[wardnet_common::network_zone::AllowedTargetKind],
        subnet: Option<&wardnet_common::network_zone::ZoneSubnet>,
    ) -> Result<(), AppError> {
        if allowed_targets.is_empty() {
            return Err(AppError::BadRequest(
                "allowed_targets must not be empty".to_owned(),
            ));
        }
        if let Some(subnet) = subnet {
            let net = ipnetwork::Ipv4Network::from_str(&subnet.cidr).map_err(|_| {
                AppError::BadRequest(format!("subnet cidr '{}' is not a valid CIDR", subnet.cidr))
            })?;
            // A LAN subnet must sit entirely inside an RFC 1918 block — a public
            // (or boundary-straddling) range would make the Pi alias a gateway
            // on address space that belongs to real internet hosts.
            if !wardnet_common::net::is_rfc1918_subnet(net.network(), net.prefix()) {
                return Err(AppError::BadRequest(format!(
                    "subnet cidr '{}' must be {}",
                    subnet.cidr,
                    wardnet_common::net::PRIVATE_RANGE_HINT
                )));
            }
            if crate::subnet::pool_bounds(net).is_none() {
                return Err(AppError::BadRequest(format!(
                    "subnet cidr '{}' is too small to host a DHCP pool",
                    subnet.cidr
                )));
            }
        }
        Ok(())
    }

    /// Reject a name that is empty or collides with a different zone.
    async fn ensure_name_available(
        &self,
        name: &str,
        exclude: Option<Uuid>,
    ) -> Result<(), AppError> {
        if name.trim().is_empty() {
            return Err(AppError::BadRequest(
                "zone name must not be empty".to_owned(),
            ));
        }
        let existing = self.zones.find_all().await.map_err(AppError::Internal)?;
        if existing
            .iter()
            .any(|z| z.name == name && Some(z.id) != exclude)
        {
            return Err(AppError::Conflict(format!(
                "a network zone named '{name}' already exists"
            )));
        }
        Ok(())
    }

    async fn view_of(&self, zone: NetworkZone) -> Result<NetworkZoneView, AppError> {
        let member_count = self
            .zones
            .count_members(&zone.id.to_string())
            .await
            .map_err(AppError::Internal)?;
        Ok(NetworkZoneView { zone, member_count })
    }

    fn emit_zone_changed(&self, zone_id: Uuid) {
        self.events.publish(WardnetEvent::NetworkZoneChanged {
            zone_id,
            timestamp: chrono::Utc::now(),
        });
    }
}

#[async_trait]
impl NetworkZoneService for NetworkZoneServiceImpl {
    async fn list_zones(&self) -> Result<Vec<NetworkZoneView>, AppError> {
        auth_context::require_admin()?;
        let zones = self.zones.find_all().await.map_err(AppError::Internal)?;
        // One GROUP BY instead of an N+1 COUNT per zone.
        let counts = self
            .zones
            .member_counts()
            .await
            .map_err(AppError::Internal)?;
        let views = zones
            .into_iter()
            .map(|zone| {
                let member_count = counts.get(&zone.id.to_string()).copied().unwrap_or(0);
                NetworkZoneView { zone, member_count }
            })
            .collect();
        Ok(views)
    }

    async fn get_zone(&self, id: Uuid) -> Result<NetworkZoneView, AppError> {
        auth_context::require_admin()?;
        let zone = self.require_zone(id).await?;
        self.view_of(zone).await
    }

    async fn create_zone(&self, req: CreateNetworkZoneRequest) -> Result<NetworkZone, AppError> {
        auth_context::require_admin()?;
        Self::validate_targets_and_subnet(&req.allowed_targets, req.subnet.as_ref())?;
        self.ensure_name_available(&req.name, None).await?;

        let now = chrono::Utc::now();
        let zone = NetworkZone {
            id: Uuid::new_v4(),
            name: req.name,
            provenance: ZoneProvenance::Manual,
            isolation_stance: req.isolation_stance,
            allowed_targets: req.allowed_targets,
            member_isolation: req.member_isolation,
            subnet: req.subnet,
            admin_ui_reachable: req.admin_ui_reachable,
            is_default: false,
            is_default_for_new: false,
            created_at: now,
            updated_at: now,
        };
        self.zones.insert(&zone).await.map_err(AppError::Internal)?;
        self.emit_zone_changed(zone.id);
        Ok(zone)
    }

    async fn update_zone(
        &self,
        id: Uuid,
        req: UpdateNetworkZoneRequest,
    ) -> Result<NetworkZone, AppError> {
        auth_context::require_admin()?;

        // Default flags only move by promoting a zone; demotion has no meaning.
        if req.is_default == Some(false) || req.is_default_for_new == Some(false) {
            return Err(AppError::BadRequest(
                "default flags cannot be cleared directly; promote another zone instead".to_owned(),
            ));
        }

        let mut zone = self.require_zone(id).await?;

        if let Some(name) = req.name {
            if name != zone.name {
                self.ensure_name_available(&name, Some(id)).await?;
            }
            zone.name = name;
        }
        if let Some(stance) = req.isolation_stance {
            zone.isolation_stance = stance;
        }
        let targets_changed = req.allowed_targets.is_some();
        if let Some(allowed_targets) = req.allowed_targets {
            zone.allowed_targets = allowed_targets;
        }
        if let Some(member_isolation) = req.member_isolation {
            zone.member_isolation = member_isolation;
        }
        if let Some(admin_ui_reachable) = req.admin_ui_reachable {
            zone.admin_ui_reachable = admin_ui_reachable;
        }
        if let Some(subnet) = req.subnet {
            zone.subnet = subnet;
        }

        Self::validate_targets_and_subnet(&zone.allowed_targets, zone.subnet.as_ref())?;

        // If the permitted target kinds change, no existing member may be left
        // holding a rule the new set forbids — the same invariant the write-time
        // gate enforces, applied from the zone-config side.
        if targets_changed {
            self.assert_members_conform(id, &zone.allowed_targets)
                .await?;
        }

        zone.updated_at = chrono::Utc::now();
        self.zones.update(&zone).await.map_err(AppError::Internal)?;

        let promoted_default = req.is_default == Some(true);
        let promoted_default_for_new = req.is_default_for_new == Some(true);
        if promoted_default {
            self.zones
                .set_default(&id.to_string())
                .await
                .map_err(AppError::Internal)?;
        }
        if promoted_default_for_new {
            self.zones
                .set_default_for_new(&id.to_string())
                .await
                .map_err(AppError::Internal)?;
        }

        self.emit_zone_changed(id);
        // Re-fetch only when a default flag was moved out-of-band; otherwise the
        // already-mutated local `zone` is authoritative.
        if promoted_default || promoted_default_for_new {
            self.require_zone(id).await
        } else {
            Ok(zone)
        }
    }

    async fn delete_zone(&self, id: Uuid) -> Result<(), AppError> {
        auth_context::require_admin()?;
        let zone = self.require_zone(id).await?;

        if zone.provenance == ZoneProvenance::System {
            return Err(AppError::Conflict(
                "system zones cannot be deleted".to_owned(),
            ));
        }
        if zone.is_default {
            return Err(AppError::Conflict(
                "the default (anchor) zone cannot be deleted".to_owned(),
            ));
        }
        if zone.is_default_for_new {
            // Deleting the sole default-for-new zone would orphan the pointer
            // (`find_default_for_new` would then error, breaking device
            // discovery). Promote another zone first.
            return Err(AppError::Conflict(
                "the default-for-new zone cannot be deleted; promote another zone first".to_owned(),
            ));
        }
        let members = self
            .zones
            .count_members(&id.to_string())
            .await
            .map_err(AppError::Internal)?;
        if members > 0 {
            return Err(AppError::Conflict(format!(
                "zone still has {members} member device(s); reassign them first"
            )));
        }

        self.zones
            .delete(&id.to_string())
            .await
            .map_err(AppError::Internal)?;
        self.emit_zone_changed(id);
        Ok(())
    }

    async fn set_default(&self, id: Uuid) -> Result<(), AppError> {
        auth_context::require_admin()?;
        self.require_zone(id).await?;
        self.zones
            .set_default(&id.to_string())
            .await
            .map_err(AppError::Internal)?;
        self.emit_zone_changed(id);
        Ok(())
    }

    async fn set_default_for_new(&self, id: Uuid) -> Result<(), AppError> {
        auth_context::require_admin()?;
        self.require_zone(id).await?;
        self.zones
            .set_default_for_new(&id.to_string())
            .await
            .map_err(AppError::Internal)?;
        self.emit_zone_changed(id);
        Ok(())
    }

    async fn assign_device(&self, device_id: Uuid, zone_id: Uuid) -> Result<(), AppError> {
        auth_context::require_admin()?;

        let device = self
            .devices
            .find_by_id(&device_id.to_string())
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound(format!("device {device_id} not found")))?;

        // Validate the target zone exists (also enforced by the FK).
        let target_zone = self.require_zone(zone_id).await?;

        let old_zone_id = device.zone_id;
        if old_zone_id == zone_id {
            return Ok(());
        }

        // Don't strand the device with a routing rule its new zone forbids —
        // the write-time gate would have rejected that target, so reject the
        // move too rather than persist contradictory state.
        if let Some(kind) = self.current_rule_kind(&device_id.to_string()).await?
            && !target_zone.permits_kind(kind)
        {
            return Err(AppError::Conflict(format!(
                "device's current routing target '{}' is not permitted by zone '{}'; change or clear its rule first",
                kind.as_str(),
                target_zone.name
            )));
        }

        let updated = self
            .devices
            .assign_zone(&device_id.to_string(), &zone_id.to_string())
            .await
            .map_err(AppError::Internal)?;
        if !updated {
            return Err(AppError::NotFound(format!("device {device_id} not found")));
        }

        self.events.publish(WardnetEvent::DeviceZoneChanged {
            device_id,
            old_zone_id,
            new_zone_id: zone_id,
            timestamp: chrono::Utc::now(),
        });
        Ok(())
    }

    async fn get_quarantine_new_devices(&self) -> Result<bool, AppError> {
        auth_context::require_admin()?;
        self.system_config
            .is_quarantine_new_devices()
            .await
            .map_err(AppError::Internal)
    }

    async fn set_quarantine_new_devices(&self, enabled: bool) -> Result<(), AppError> {
        auth_context::require_admin()?;
        self.system_config
            .set_quarantine_new_devices(enabled)
            .await
            .map_err(AppError::Internal)
    }
}
