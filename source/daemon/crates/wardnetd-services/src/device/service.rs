use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;
use wardnet_common::api::{
    DeviceMeResponse, DnsCaptureSettingsResponse, DnsEventItem, SetMyRuleResponse,
};
use wardnet_common::auth::AuthContext;
use wardnet_common::device::Device;
use wardnet_common::event::WardnetEvent;
use wardnet_common::network_zone::AllowedTargetKind;
use wardnet_common::routing::{RoutingTarget, RuleCreator};

use crate::auth_context;
use crate::error::AppError;
use crate::event::EventPublisher;
use wardnetd_data::repository::{
    DeviceRepository, DnsEventsRepository, NetworkZoneRepository, SystemConfigRepository,
};

/// Device lookup and self-service routing management.
///
/// Handles both admin and self-service flows. Authorization is derived
/// from the [`AuthContext`] task-local set by the API middleware:
///
/// - **Admin**: full access to all operations, bypasses admin-lock.
/// - **Device**: can only modify their own device, respects admin-lock.
/// - **Anonymous**: read-only access (e.g. `get_device_for_ip`).
#[async_trait]
pub trait DeviceService: Send + Sync {
    /// Look up the device for the given IP and return its routing state.
    async fn get_device_for_ip(&self, ip: &str) -> Result<DeviceMeResponse, AppError>;

    /// Set a new routing rule for a device identified by its IP.
    ///
    /// Authorization rules (enforced via [`AuthContext`]):
    /// - Admin: always allowed (bypasses admin-lock).
    /// - Device caller whose MAC matches: allowed unless admin-locked.
    /// - Otherwise: forbidden.
    async fn set_rule_for_ip(
        &self,
        ip: &str,
        target: RoutingTarget,
    ) -> Result<SetMyRuleResponse, AppError>;

    /// Set a routing rule for a device identified by its ID.
    ///
    /// Same authorization rules as [`set_rule_for_ip`](Self::set_rule_for_ip).
    async fn set_rule(&self, device_id: &str, target: RoutingTarget) -> Result<(), AppError>;

    /// Return every device's current routing target, keyed by device ID.
    ///
    /// Batched companion to the per-device rule lookup used to enrich the
    /// admin device list (`GET /api/devices`) without an N+1. Devices with no
    /// rule are absent from the map (they follow the gateway default policy).
    /// Requires admin privileges via the [`AuthContext`].
    async fn current_rules(&self) -> Result<HashMap<Uuid, RoutingTarget>, AppError>;

    /// Update the `admin_locked` flag for a device.
    ///
    /// Requires admin privileges via the [`AuthContext`].
    async fn update_admin_locked(&self, device_id: &str, locked: bool) -> Result<(), AppError>;

    /// Return current DNS capture settings and storage stats for a device.
    ///
    /// Requires admin privileges via the [`AuthContext`].
    async fn get_dns_capture_settings(
        &self,
        device_id: &str,
    ) -> Result<DnsCaptureSettingsResponse, AppError>;

    /// Update DNS capture settings for a device.
    ///
    /// Only `Some` fields are written; `None` leaves the existing value
    /// unchanged. Returns `AppError::NotFound` when the device does not exist.
    /// Requires admin privileges via the [`AuthContext`].
    async fn update_dns_capture_settings(
        &self,
        device_id: &str,
        enabled: Option<bool>,
        cap_count: Option<i64>,
        cap_days: Option<i64>,
    ) -> Result<(), AppError>;

    /// Self-service capture toggle: enable/disable DNS capture for the device
    /// resolved by source `ip`. Flips only the `enabled` flag — retention caps
    /// (`cap_count`/`cap_days`) stay admin-only and are left untouched. The
    /// caller must be the device itself (matched by IP/MAC) or an admin.
    /// Returns the device's current capture settings and storage stats.
    async fn set_my_capture_enabled(
        &self,
        ip: &str,
        enabled: bool,
    ) -> Result<DnsCaptureSettingsResponse, AppError>;

    /// Return pending (unsynced) DNS events for the device with `id > after_id`,
    /// oldest first, up to `limit` rows. No auth check — caller resolves device
    /// by IP.
    async fn fetch_pending_dns_events(
        &self,
        device_id: &str,
        after_id: i64,
        limit: i64,
    ) -> Result<Vec<DnsEventItem>, AppError>;

    /// Mark all DNS events with `id <= up_to_id` as synced for the device.
    async fn mark_dns_events_synced(&self, device_id: &str, up_to_id: i64) -> Result<(), AppError>;

    /// Delete all DNS events with `id <= up_to_id` for the device (called on
    /// client ack).
    async fn ack_dns_events(&self, device_id: &str, up_to_id: i64) -> Result<(), AppError>;

    /// Return all device IDs that currently have DNS capture enabled.
    /// For internal use by background tasks — no auth check.
    async fn list_capture_enabled_device_ids(&self) -> Result<Vec<String>, AppError>;

    /// Return the DNS capture settings for a device by ID, or `None` if the
    /// device does not exist. For internal use by background tasks — no auth check.
    async fn get_device_capture_settings(
        &self,
        device_id: &str,
    ) -> Result<Option<(bool, i64, i64)>, AppError>;
}

/// Default implementation of [`DeviceService`] backed by [`DeviceRepository`].
pub struct DeviceServiceImpl {
    devices: Arc<dyn DeviceRepository>,
    dns_events: Arc<dyn DnsEventsRepository>,
    zones: Arc<dyn NetworkZoneRepository>,
    system_config: Arc<dyn SystemConfigRepository>,
    events: Arc<dyn EventPublisher>,
}

impl DeviceServiceImpl {
    /// Create a new service backed by the given repositories and event publisher.
    pub fn new(
        devices: Arc<dyn DeviceRepository>,
        dns_events: Arc<dyn DnsEventsRepository>,
        zones: Arc<dyn NetworkZoneRepository>,
        system_config: Arc<dyn SystemConfigRepository>,
        events: Arc<dyn EventPublisher>,
    ) -> Self {
        Self {
            devices,
            dns_events,
            zones,
            system_config,
            events,
        }
    }

    /// Resolve a routing target into the coarse [`AllowedTargetKind`] a zone
    /// gates on. `Default` is resolved through the persisted global
    /// `default_policy` (read from `system_config`, avoiding a service→service
    /// cycle) using the shared [`RoutingTarget::from_default_policy`] — the same
    /// classifier the routing engine uses, so the two never disagree.
    async fn resolve_target_kind(
        &self,
        target: &RoutingTarget,
    ) -> Result<AllowedTargetKind, AppError> {
        if let Some(kind) = AllowedTargetKind::of_target(target) {
            return Ok(kind);
        }
        // `Default` — resolve via the global policy.
        let policy = self
            .system_config
            .get_default_policy()
            .await
            .map_err(AppError::Internal)?
            .unwrap_or_else(|| "direct".to_owned());
        let concrete = RoutingTarget::from_default_policy(&policy);
        Ok(AllowedTargetKind::of_target(&concrete).unwrap_or(AllowedTargetKind::Direct))
    }

    /// Reject a routing target that the device's Network Zone does not permit.
    ///
    /// `Default` is resolve-then-check: it is resolved to a concrete kind via
    /// the global default policy, then that kind is checked against the zone's
    /// `allowed_targets`. Returns `Conflict` when disallowed. Applies to both
    /// admin and self-service rule writes.
    async fn validate_target_against_zone(
        &self,
        device: &Device,
        target: &RoutingTarget,
    ) -> Result<(), AppError> {
        let kind = self.resolve_target_kind(target).await?;

        let zone = self
            .zones
            .find_by_id(&device.zone_id.to_string())
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| {
                AppError::Internal(anyhow::anyhow!(
                    "device {} references unknown zone {}",
                    device.id,
                    device.zone_id
                ))
            })?;

        if !zone.permits_kind(kind) {
            return Err(AppError::Conflict(format!(
                "routing target '{}' is not permitted by this device's zone '{}'",
                kind.as_str(),
                zone.name
            )));
        }
        Ok(())
    }

    /// Check whether the current auth context authorises a mutation on the
    /// given device. Returns `Ok(())` if allowed, `Err(Forbidden)` otherwise.
    fn check_device_mutation_auth(
        ctx: &AuthContext,
        device_mac: &str,
        admin_locked: bool,
    ) -> Result<(), AppError> {
        match ctx {
            AuthContext::Admin { .. } => Ok(()),
            AuthContext::Device { mac } if mac == device_mac => {
                if admin_locked {
                    Err(AppError::Forbidden(
                        "routing is locked by admin for this device".to_owned(),
                    ))
                } else {
                    Ok(())
                }
            }
            _ => Err(AppError::Forbidden(
                "not authorised to modify this device".to_owned(),
            )),
        }
    }
}

#[async_trait]
impl DeviceService for DeviceServiceImpl {
    async fn get_device_for_ip(&self, ip: &str) -> Result<DeviceMeResponse, AppError> {
        let device = self
            .devices
            .find_by_ip(ip)
            .await
            .map_err(AppError::Internal)?;

        let (current_rule, admin_locked) = match &device {
            Some(d) => {
                let rule = self
                    .devices
                    .find_rule_for_device(&d.id.to_string())
                    .await
                    .map_err(AppError::Internal)?;
                (rule.map(|r| r.target), d.admin_locked)
            }
            None => (None, false),
        };

        Ok(DeviceMeResponse {
            device,
            current_rule,
            admin_locked,
            available_tunnels: vec![], // Enriched by the API handler.
        })
    }

    async fn set_rule_for_ip(
        &self,
        ip: &str,
        target: RoutingTarget,
    ) -> Result<SetMyRuleResponse, AppError> {
        let device = self
            .devices
            .find_by_ip(ip)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound("device not found for this IP".to_owned()))?;

        let ctx = auth_context::try_current().unwrap_or(AuthContext::Anonymous);
        Self::check_device_mutation_auth(&ctx, &device.mac, device.admin_locked)?;

        self.validate_target_against_zone(&device, &target).await?;

        let previous = self
            .devices
            .find_rule_for_device(&device.id.to_string())
            .await
            .map_err(AppError::Internal)?;

        let target_json =
            serde_json::to_string(&target).map_err(|e| AppError::Internal(e.into()))?;
        let now = chrono::Utc::now().to_rfc3339();

        self.devices
            .upsert_user_rule(&device.id.to_string(), &target_json, &now)
            .await
            .map_err(AppError::Internal)?;

        let changed_by = match &ctx {
            AuthContext::Admin { .. } => RuleCreator::Admin,
            _ => RuleCreator::User,
        };
        self.events.publish(WardnetEvent::RoutingRuleChanged {
            device_id: device.id,
            target: target.clone(),
            previous_target: previous.map(|r| r.target),
            changed_by,
            timestamp: chrono::Utc::now(),
        });

        Ok(SetMyRuleResponse {
            message: "routing rule updated".to_owned(),
            target,
        })
    }

    async fn set_rule(&self, device_id: &str, target: RoutingTarget) -> Result<(), AppError> {
        let device = self
            .devices
            .find_by_id(device_id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound("device not found".to_owned()))?;

        let ctx = auth_context::try_current().unwrap_or(AuthContext::Anonymous);
        Self::check_device_mutation_auth(&ctx, &device.mac, device.admin_locked)?;

        self.validate_target_against_zone(&device, &target).await?;

        let previous = self
            .devices
            .find_rule_for_device(&device.id.to_string())
            .await
            .map_err(AppError::Internal)?;

        let target_json =
            serde_json::to_string(&target).map_err(|e| AppError::Internal(e.into()))?;
        let now = chrono::Utc::now().to_rfc3339();

        self.devices
            .upsert_user_rule(device_id, &target_json, &now)
            .await
            .map_err(AppError::Internal)?;

        let changed_by = match &ctx {
            AuthContext::Admin { .. } => RuleCreator::Admin,
            _ => RuleCreator::User,
        };
        self.events.publish(WardnetEvent::RoutingRuleChanged {
            device_id: device.id,
            target,
            previous_target: previous.map(|r| r.target),
            changed_by,
            timestamp: chrono::Utc::now(),
        });

        Ok(())
    }

    async fn current_rules(&self) -> Result<HashMap<Uuid, RoutingTarget>, AppError> {
        auth_context::require_admin()?;

        let rules = self
            .devices
            .find_all_rules()
            .await
            .map_err(AppError::Internal)?;

        Ok(rules.into_iter().map(|r| (r.device_id, r.target)).collect())
    }

    async fn update_admin_locked(&self, device_id: &str, locked: bool) -> Result<(), AppError> {
        let ctx = auth_context::try_current().unwrap_or(AuthContext::Anonymous);
        if !ctx.is_admin() {
            return Err(AppError::Forbidden("admin privileges required".to_owned()));
        }

        self.devices
            .update_admin_locked(device_id, locked)
            .await
            .map_err(AppError::Internal)
    }

    async fn get_dns_capture_settings(
        &self,
        device_id: &str,
    ) -> Result<DnsCaptureSettingsResponse, AppError> {
        let ctx = auth_context::try_current().unwrap_or(AuthContext::Anonymous);
        if !ctx.is_admin() {
            return Err(AppError::Forbidden("admin privileges required".to_owned()));
        }

        let device = self
            .devices
            .find_by_id(device_id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound("device not found".to_owned()))?;

        let stats = self
            .dns_events
            .stats_for_device(device_id)
            .await
            .map_err(AppError::Internal)?;

        Ok(DnsCaptureSettingsResponse {
            enabled: device.dns_capture_enabled,
            cap_count: device.dns_capture_cap_count,
            cap_days: device.dns_capture_cap_days,
            row_count: stats.row_count,
            size_bytes: stats.size_bytes,
        })
    }

    async fn update_dns_capture_settings(
        &self,
        device_id: &str,
        enabled: Option<bool>,
        cap_count: Option<i64>,
        cap_days: Option<i64>,
    ) -> Result<(), AppError> {
        let ctx = auth_context::try_current().unwrap_or(AuthContext::Anonymous);
        if !ctx.is_admin() {
            return Err(AppError::Forbidden("admin privileges required".to_owned()));
        }

        let found = self
            .devices
            .update_dns_capture_settings(device_id, enabled, cap_count, cap_days)
            .await
            .map_err(AppError::Internal)?;

        if !found {
            return Err(AppError::NotFound("device not found".to_owned()));
        }

        let device_uuid: Uuid = device_id
            .parse()
            .map_err(|_| AppError::NotFound("device not found".to_owned()))?;

        // Resolve actual enabled state to publish the correct event value.
        let now_enabled = if let Some(e) = enabled {
            e
        } else {
            self.devices
                .find_by_id(device_id)
                .await
                .map_err(AppError::Internal)?
                .is_some_and(|d| d.dns_capture_enabled)
        };

        let () = self
            .events
            .publish(WardnetEvent::DeviceCaptureSettingsChanged {
                device_id: device_uuid,
                enabled: now_enabled,
                timestamp: Utc::now(),
            });

        Ok(())
    }

    async fn set_my_capture_enabled(
        &self,
        ip: &str,
        enabled: bool,
    ) -> Result<DnsCaptureSettingsResponse, AppError> {
        let device = self
            .devices
            .find_by_ip(ip)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound("device not found for this IP".to_owned()))?;

        // Self-service: the caller must be this device (by IP/MAC) or an admin.
        // Capture is independent of the routing admin-lock, so pass `false`.
        let ctx = auth_context::try_current().unwrap_or(AuthContext::Anonymous);
        Self::check_device_mutation_auth(&ctx, &device.mac, false)?;

        let device_id = device.id.to_string();

        // Flip only the `enabled` flag; leave retention caps (admin-owned) alone.
        let found = self
            .devices
            .update_dns_capture_settings(&device_id, Some(enabled), None, None)
            .await
            .map_err(AppError::Internal)?;
        if !found {
            return Err(AppError::NotFound("device not found".to_owned()));
        }

        self.events
            .publish(WardnetEvent::DeviceCaptureSettingsChanged {
                device_id: device.id,
                enabled,
                timestamp: Utc::now(),
            });

        let stats = self
            .dns_events
            .stats_for_device(&device_id)
            .await
            .map_err(AppError::Internal)?;

        Ok(DnsCaptureSettingsResponse {
            enabled,
            cap_count: device.dns_capture_cap_count,
            cap_days: device.dns_capture_cap_days,
            row_count: stats.row_count,
            size_bytes: stats.size_bytes,
        })
    }

    async fn fetch_pending_dns_events(
        &self,
        device_id: &str,
        after_id: i64,
        limit: i64,
    ) -> Result<Vec<DnsEventItem>, AppError> {
        let rows = self
            .dns_events
            .fetch_pending(device_id, after_id, limit)
            .await
            .map_err(AppError::Internal)?;
        Ok(rows
            .into_iter()
            .map(|r| DnsEventItem {
                id: r.id,
                domain: r.domain,
                status: r.status,
                captured_at: r.captured_at,
            })
            .collect())
    }

    async fn mark_dns_events_synced(&self, device_id: &str, up_to_id: i64) -> Result<(), AppError> {
        self.dns_events
            .mark_synced_up_to(device_id, up_to_id)
            .await
            .map_err(AppError::Internal)?;
        Ok(())
    }

    async fn ack_dns_events(&self, device_id: &str, up_to_id: i64) -> Result<(), AppError> {
        self.dns_events
            .delete_up_to(device_id, up_to_id)
            .await
            .map_err(AppError::Internal)?;
        Ok(())
    }

    async fn list_capture_enabled_device_ids(&self) -> Result<Vec<String>, AppError> {
        self.devices
            .find_all_capture_enabled_ids()
            .await
            .map_err(AppError::Internal)
    }

    async fn get_device_capture_settings(
        &self,
        device_id: &str,
    ) -> Result<Option<(bool, i64, i64)>, AppError> {
        match self
            .devices
            .find_by_id(device_id)
            .await
            .map_err(AppError::Internal)?
        {
            None => Ok(None),
            Some(d) => Ok(Some((
                d.dns_capture_enabled,
                d.dns_capture_cap_count,
                d.dns_capture_cap_days,
            ))),
        }
    }
}
