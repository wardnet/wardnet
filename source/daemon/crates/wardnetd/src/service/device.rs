use std::sync::Arc;

use async_trait::async_trait;
use wardnet_types::api::{DeviceMeResponse, SetMyRuleResponse};
use wardnet_types::auth::AuthContext;
use wardnet_types::routing::RoutingTarget;

use crate::auth_context;
use crate::error::AppError;
use crate::repository::DeviceRepository;

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

    /// Update the `admin_locked` flag for a device.
    ///
    /// Requires admin privileges via the [`AuthContext`].
    async fn update_admin_locked(&self, device_id: &str, locked: bool) -> Result<(), AppError>;
}

/// Default implementation of [`DeviceService`] backed by [`DeviceRepository`].
pub struct DeviceServiceImpl {
    devices: Arc<dyn DeviceRepository>,
}

impl DeviceServiceImpl {
    /// Create a new service backed by the given device repository.
    pub fn new(devices: Arc<dyn DeviceRepository>) -> Self {
        Self { devices }
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

        let target_json =
            serde_json::to_string(&target).map_err(|e| AppError::Internal(e.into()))?;
        let now = chrono::Utc::now().to_rfc3339();

        self.devices
            .upsert_user_rule(&device.id.to_string(), &target_json, &now)
            .await
            .map_err(AppError::Internal)?;

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

        let target_json =
            serde_json::to_string(&target).map_err(|e| AppError::Internal(e.into()))?;
        let now = chrono::Utc::now().to_rfc3339();

        self.devices
            .upsert_user_rule(device_id, &target_json, &now)
            .await
            .map_err(AppError::Internal)?;

        Ok(())
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
}
