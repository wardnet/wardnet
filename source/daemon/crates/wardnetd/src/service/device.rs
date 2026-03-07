use std::sync::Arc;

use async_trait::async_trait;
use wardnet_types::api::{DeviceMeResponse, SetMyRuleResponse};
use wardnet_types::routing::RoutingTarget;

use crate::error::AppError;
use crate::repository::DeviceRepository;

/// Device lookup and self-service routing management.
///
/// Handles the unauthenticated user flow: given a client IP, find the
/// matching device, return its current routing rule, and allow the user
/// to change it — unless an admin has locked the device.
#[async_trait]
pub trait DeviceService: Send + Sync {
    /// Look up the device for the given IP and return its routing state.
    async fn get_device_for_ip(&self, ip: &str) -> Result<DeviceMeResponse, AppError>;

    /// Set a new routing rule for the device at the given IP.
    /// Fails with `Forbidden` if the device is admin-locked.
    async fn set_rule_for_ip(
        &self,
        ip: &str,
        target: RoutingTarget,
    ) -> Result<SetMyRuleResponse, AppError>;
}

/// Default implementation of [`DeviceService`] backed by [`DeviceRepository`].
pub struct DeviceServiceImpl {
    devices: Arc<dyn DeviceRepository>,
}

impl DeviceServiceImpl {
    pub fn new(devices: Arc<dyn DeviceRepository>) -> Self {
        Self { devices }
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

        if device.admin_locked {
            return Err(AppError::Forbidden(
                "routing is locked by admin for this device".to_owned(),
            ));
        }

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
}
