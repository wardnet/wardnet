use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;
use wardnet_common::api::{
    DeviceMeResponse, DnsCaptureSettingsResponse, DnsEventItem, SetMyRuleResponse,
};
use wardnet_common::auth::AuthContext;
use wardnet_common::device::{Device, DeviceConnectionMode};
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

    /// Return the current routing target for a single device by its ID, if it
    /// has one.
    ///
    /// Resolves the rule directly from the device ID rather than round-tripping
    /// through the device's `last_ip` — a departed device's `last_ip` is cleared,
    /// so an IP-keyed lookup would either miss the rule or resolve to a
    /// different device. Requires admin privileges via the [`AuthContext`].
    async fn get_rule_for_device(&self, device_id: &str)
    -> Result<Option<RoutingTarget>, AppError>;

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

    /// Return pending DNS events for the device with `id > after_id`, oldest
    /// first, up to `limit` rows. Self-service: the caller must be the device
    /// itself (matched by IP/MAC) or an admin.
    async fn fetch_pending_dns_events(
        &self,
        device_id: &str,
        after_id: i64,
        limit: i64,
    ) -> Result<Vec<DnsEventItem>, AppError>;

    /// Delete all DNS events with `id <= up_to_id` for the device (called on
    /// client ack). Self-service: the caller must be the device itself (matched
    /// by IP/MAC) or an admin.
    async fn ack_dns_events(&self, device_id: &str, up_to_id: i64) -> Result<(), AppError>;

    /// Return all device IDs that currently have DNS capture enabled.
    /// Requires admin privileges via the [`AuthContext`].
    async fn list_capture_enabled_device_ids(&self) -> Result<Vec<String>, AppError>;

    /// Return the DNS capture settings for a device by ID, or `None` if the
    /// device does not exist. Requires admin privileges via the [`AuthContext`].
    async fn get_device_capture_settings(
        &self,
        device_id: &str,
    ) -> Result<Option<(bool, i64, i64)>, AppError>;

    /// Return the device by id, or `None` if it does not exist. For internal
    /// use by other services needing device identity (e.g. inbound `WireGuard`
    /// peer grants) — no auth check.
    async fn get_device(&self, device_id: &str) -> Result<Option<Device>, AppError>;

    /// Reset a device's `connection_mode` back to `Lan` if it's currently
    /// `Remote` — called when a device's only remote-access path is revoked, so a
    /// stale `Remote` status doesn't persist indefinitely with no live path left
    /// to naturally correct it. No-op if the device is already `Lan` or doesn't
    /// exist. For internal use by other services (e.g. inbound `WireGuard` peer
    /// revocation) — no auth check (matches this file's existing internal-method
    /// convention for `get_device` / `get_device_capture_settings`).
    async fn clear_remote_connection_mode(&self, device_id: &str) -> Result<(), AppError>;

    /// Promote a device to **managed** — an admin has decided to control its
    /// configuration.
    ///
    /// Called by every admin configuration act (naming, locking, an
    /// admin-created routing rule or profile, DNS filter settings, DNS capture,
    /// a Private-DNS grant, a Remote peer credential, a DHCP reservation, a
    /// zone exception, an explicit zone reassignment). Idempotent, and a no-op
    /// if the device does not exist — a promotion must never be the thing that
    /// fails an otherwise-successful configuration change.
    ///
    /// Routed through this service rather than each caller writing
    /// [`DeviceRepository`] directly, per the single-service-per-repository
    /// rule. **Self-service acts must not call this**: a device configuring
    /// itself is the device asking, not the admin deciding, and promoting on it
    /// would make every guest device permanently exempt from retention.
    async fn mark_managed(&self, device_id: &str) -> Result<(), AppError>;

    /// Delete a device's routing rule, returning it to "no rule" — the state a
    /// never-configured device is in, where it follows the gateway's global
    /// default policy.
    ///
    /// Deliberately not `set_rule(Direct)`. That writes an explicit persisted
    /// choice that *overrides* the default policy rather than deferring to it,
    /// and it is validated against the device's zone allow-list — so on a
    /// tunnel-only zone it is rejected outright, which would make releasing
    /// such a device impossible. Deleting cannot conflict with a zone.
    ///
    /// Publishes `RoutingRuleChanged` carrying the *global default policy*, so
    /// the routing listener tears down the device's per-device rules and leaves
    /// it on the default path. Idempotent. Requires admin.
    async fn clear_rule(&self, device_id: &str) -> Result<(), AppError>;

    /// Demote a device back to unmanaged.
    ///
    /// **Only** the release handler (`POST /api/devices/{id}/release`) may call
    /// this, and only as its final step, after every managed setting has been
    /// reverted to default. Calling it with configuration still in place breaks
    /// the invariant device retention relies on — `managed = false` implies no
    /// admin artefacts exist — and the device's rows would be silently deleted
    /// 30 days after it was last seen, taking a live Private-DNS grant or
    /// Remote peer credential with them.
    async fn clear_managed(&self, device_id: &str) -> Result<(), AppError>;
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
            zone: None,                // Enriched by the API handler.
            routing_profiles: vec![],  // Enriched by the API handler.
        })
    }

    async fn set_rule_for_ip(
        &self,
        ip: &str,
        target: RoutingTarget,
    ) -> Result<SetMyRuleResponse, AppError> {
        // Category-(c) guard-not-first (.agents/auth.md §Rules #2): the device's MAC
        // is the subject of the `check_device_mutation_auth` check below, so the
        // device is resolved first. This deviation from "guard must be first" is
        // deliberate, not an oversight — the lookup only materializes the subject,
        // and the auth check remains the first thing done with the caller's identity.
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
        // Category-(c) guard-not-first (.agents/auth.md §Rules #2): the device's MAC
        // is the subject of the `check_device_mutation_auth` check below, so the
        // device is resolved first. This deviation from "guard must be first" is
        // deliberate, not an oversight — the lookup only materializes the subject,
        // and the auth check remains the first thing done with the caller's identity.
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

        // An ADMIN-set routing rule promotes the device to managed (issue
        // #1181); a self-service one deliberately does not — that is the device
        // asking, not the admin deciding, and promoting on it would make every
        // guest device permanently exempt from the retention prune. Gated on
        // the same `ctx` that decides `changed_by`, so the two can't disagree.
        //
        // The stored `created_by` is not usable as this signal:
        // `upsert_user_rule` hard-codes `'user'` regardless of caller, so an
        // admin-set rule is indistinguishable in the row. That is also why the
        // migration's `created_by = 'admin'` backfill clause matches nothing
        // today — it is the correct predicate, kept for the day rules record
        // their true author.
        if changed_by == RuleCreator::Admin {
            self.devices
                .set_managed(device_id, true)
                .await
                .map_err(AppError::Internal)?;
        }

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

    async fn get_rule_for_device(
        &self,
        device_id: &str,
    ) -> Result<Option<RoutingTarget>, AppError> {
        auth_context::require_admin()?;

        let rule = self
            .devices
            .find_rule_for_device(device_id)
            .await
            .map_err(AppError::Internal)?;

        Ok(rule.map(|r| r.target))
    }

    async fn update_admin_locked(&self, device_id: &str, locked: bool) -> Result<(), AppError> {
        auth_context::require_admin()?;

        self.devices
            .update_admin_locked(device_id, locked)
            .await
            .map_err(AppError::Internal)?;

        // Locking promotes to managed (issue #1181). Unlocking does not: it
        // returns the flag to its default, so it is a revert rather than a
        // configuration act — and `managed` is latching anyway, so a device
        // locked then unlocked stays managed until explicitly released. Gating
        // on `locked` also mirrors the migration's `admin_locked = 1` backfill.
        if locked {
            self.devices
                .set_managed(device_id, true)
                .await
                .map_err(AppError::Internal)?;
        }

        // Notify the affected device (push): its routing was locked/unlocked.
        if let Ok(id) = uuid::Uuid::parse_str(device_id) {
            self.events.publish(WardnetEvent::DeviceAdminLocked {
                device_id: id,
                locked,
                timestamp: chrono::Utc::now(),
            });
        }
        Ok(())
    }

    async fn get_dns_capture_settings(
        &self,
        device_id: &str,
    ) -> Result<DnsCaptureSettingsResponse, AppError> {
        auth_context::require_admin()?;

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
        auth_context::require_admin()?;

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

        // Enabling capture promotes to managed (issue #1181), mirroring the
        // migration's `dns_capture_enabled = 1` backfill. Disabling is a revert
        // to default and promotes nothing; `managed` is latching, so a device
        // whose capture was enabled then disabled stays managed until released.
        //
        // Keyed on `now_enabled`, not `enabled`: a caller adjusting only the
        // caps on an already-capturing device is still configuring capture on
        // it. Note `set_my_capture_enabled` — the self-service path — is a
        // separate method and deliberately promotes nothing.
        if now_enabled {
            self.devices
                .set_managed(device_id, true)
                .await
                .map_err(AppError::Internal)?;
        }

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
        // Category-(c) guard-not-first (.agents/auth.md §Rules #2): the device's MAC
        // is the subject of the `check_device_mutation_auth` check below, so the
        // device is resolved first. This deviation from "guard must be first" is
        // deliberate, not an oversight — the lookup only materializes the subject,
        // and the auth check remains the first thing done with the caller's identity.
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
        // Self-service (category c, `.agents/auth.md`): the DNS-events stream is
        // reached by the device itself (resolved by source IP) or an admin.
        // Match the current context against the device's MAC before returning
        // its captured events. Capture is independent of the routing
        // admin-lock, so pass `false`.
        let device = self
            .devices
            .find_by_id(device_id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound("device not found".to_owned()))?;
        let ctx = auth_context::try_current().unwrap_or(AuthContext::Anonymous);
        Self::check_device_mutation_auth(&ctx, &device.mac, false)?;

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

    async fn ack_dns_events(&self, device_id: &str, up_to_id: i64) -> Result<(), AppError> {
        // Self-service (category c, `.agents/auth.md`): the ack route is reached
        // by the device itself (resolved by source IP) or an admin. Match the
        // current context against the device's MAC before deleting its captured
        // events. Capture is independent of the routing admin-lock, so pass
        // `false`.
        let device = self
            .devices
            .find_by_id(device_id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound("device not found".to_owned()))?;
        let ctx = auth_context::try_current().unwrap_or(AuthContext::Anonymous);
        Self::check_device_mutation_auth(&ctx, &device.mac, false)?;

        self.dns_events
            .delete_up_to(device_id, up_to_id)
            .await
            .map_err(AppError::Internal)?;
        Ok(())
    }

    async fn list_capture_enabled_device_ids(&self) -> Result<Vec<String>, AppError> {
        auth_context::require_admin()?;

        self.devices
            .find_all_capture_enabled_ids()
            .await
            .map_err(AppError::Internal)
    }

    async fn get_device_capture_settings(
        &self,
        device_id: &str,
    ) -> Result<Option<(bool, i64, i64)>, AppError> {
        auth_context::require_admin()?;

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

    async fn get_device(&self, device_id: &str) -> Result<Option<Device>, AppError> {
        self.devices
            .find_by_id(device_id)
            .await
            .map_err(AppError::Internal)
    }

    async fn clear_remote_connection_mode(&self, device_id: &str) -> Result<(), AppError> {
        let Some(device) = self
            .devices
            .find_by_id(device_id)
            .await
            .map_err(AppError::Internal)?
        else {
            return Ok(());
        };

        // Only correct a stale `Remote`; leave `Lan` alone so we don't clobber a
        // device that is (or has since become) locally present.
        if device.connection_mode == DeviceConnectionMode::Remote {
            self.devices
                .update_connection_mode(device_id, DeviceConnectionMode::Lan)
                .await
                .map_err(AppError::Internal)?;
        }
        Ok(())
    }

    async fn clear_rule(&self, device_id: &str) -> Result<(), AppError> {
        auth_context::require_admin()?;

        let device_uuid: Uuid = device_id
            .parse()
            .map_err(|_| AppError::NotFound("device not found".to_owned()))?;

        let previous = self
            .devices
            .find_rule_for_device(device_id)
            .await
            .map_err(AppError::Internal)?;

        self.devices
            .delete_rule_for_device(device_id)
            .await
            .map_err(AppError::Internal)?;

        // Nothing to tear down if there was no rule; skipping the publish also
        // keeps the release quiet for the common case.
        if previous.is_none() {
            return Ok(());
        }

        // Publish the target the device now follows — the global default policy
        // — rather than the rule we removed, so the routing listener applies
        // the right end state instead of re-installing what we just deleted.
        let target = self
            .system_config
            .get_default_policy()
            .await
            .map_err(AppError::Internal)?
            .and_then(|raw| serde_json::from_str::<RoutingTarget>(&raw).ok())
            .unwrap_or(RoutingTarget::Direct);

        self.events.publish(WardnetEvent::RoutingRuleChanged {
            device_id: device_uuid,
            target,
            previous_target: previous.map(|r| r.target),
            changed_by: RuleCreator::Admin,
            timestamp: chrono::Utc::now(),
        });
        Ok(())
    }

    async fn mark_managed(&self, device_id: &str) -> Result<(), AppError> {
        auth_context::require_admin()?;

        self.devices
            .set_managed(device_id, true)
            .await
            .map_err(AppError::Internal)
    }

    async fn clear_managed(&self, device_id: &str) -> Result<(), AppError> {
        auth_context::require_admin()?;

        self.devices
            .set_managed(device_id, false)
            .await
            .map_err(AppError::Internal)
    }
}
