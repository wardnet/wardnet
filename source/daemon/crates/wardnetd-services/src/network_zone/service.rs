use std::str::FromStr;
use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::api::{CreateNetworkZoneRequest, NetworkZoneView, UpdateNetworkZoneRequest};
use wardnet_common::event::WardnetEvent;
use wardnet_common::network_zone::{NetworkZone, ZoneProvenance};
use wardnetd_data::repository::{DeviceRepository, NetworkZoneRepository};

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
}

/// Default implementation of [`NetworkZoneService`].
pub struct NetworkZoneServiceImpl {
    zones: Arc<dyn NetworkZoneRepository>,
    devices: Arc<dyn DeviceRepository>,
    events: Arc<dyn EventPublisher>,
}

impl NetworkZoneServiceImpl {
    /// Create a new service backed by the given repositories and event publisher.
    #[must_use]
    pub fn new(
        zones: Arc<dyn NetworkZoneRepository>,
        devices: Arc<dyn DeviceRepository>,
        events: Arc<dyn EventPublisher>,
    ) -> Self {
        Self {
            zones,
            devices,
            events,
        }
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
        if let Some(subnet) = subnet
            && ipnetwork::IpNetwork::from_str(&subnet.cidr).is_err()
        {
            return Err(AppError::BadRequest(format!(
                "subnet cidr '{}' is not a valid CIDR",
                subnet.cidr
            )));
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
        let mut views = Vec::with_capacity(zones.len());
        for zone in zones {
            views.push(self.view_of(zone).await?);
        }
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

        zone.updated_at = chrono::Utc::now();
        self.zones.update(&zone).await.map_err(AppError::Internal)?;

        if req.is_default == Some(true) {
            self.zones
                .set_default(&id.to_string())
                .await
                .map_err(AppError::Internal)?;
        }
        if req.is_default_for_new == Some(true) {
            self.zones
                .set_default_for_new(&id.to_string())
                .await
                .map_err(AppError::Internal)?;
        }

        self.emit_zone_changed(id);
        // Re-fetch so any default-flag flips are reflected in the response.
        self.require_zone(id).await
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
        self.require_zone(zone_id).await?;

        let old_zone_id = device.zone_id;
        if old_zone_id == zone_id {
            return Ok(());
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
}
