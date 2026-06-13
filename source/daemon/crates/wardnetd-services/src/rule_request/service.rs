use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;
use wardnet_common::auth::AuthContext;
use wardnet_common::rule_request::{DeviceRuleRequest, RuleRequestKind, RuleRequestStatus};
use wardnetd_data::repository::RuleRequestRepository;

use crate::auth_context;
use crate::device::DeviceService;
use crate::error::AppError;

/// Service for the lightweight "ask the admin" rule-request inbox.
///
/// Device-scoped methods resolve the caller's device by IP through
/// [`DeviceService`] (so this service never holds the device repository).
/// Admin-scoped methods gate on [`auth_context::require_admin`].
#[async_trait]
pub trait RuleRequestService: Send + Sync {
    /// Create a pending request from the device resolved by `ip`.
    async fn create_for_ip(
        &self,
        ip: &str,
        kind: RuleRequestKind,
        domain: &str,
        reason: Option<String>,
    ) -> Result<DeviceRuleRequest, AppError>;

    /// List the requests made by the device resolved by `ip`, newest first.
    async fn list_for_ip(&self, ip: &str) -> Result<Vec<DeviceRuleRequest>, AppError>;

    /// Admin: list all requests, optionally filtered by status.
    async fn list(
        &self,
        status: Option<RuleRequestStatus>,
    ) -> Result<Vec<DeviceRuleRequest>, AppError>;

    /// Admin: record an approve/reject decision for a request.
    async fn decide(
        &self,
        id: &str,
        status: RuleRequestStatus,
    ) -> Result<DeviceRuleRequest, AppError>;
}

pub struct RuleRequestServiceImpl {
    requests: Arc<dyn RuleRequestRepository>,
    device_service: Arc<dyn DeviceService>,
}

impl RuleRequestServiceImpl {
    #[must_use]
    pub fn new(
        requests: Arc<dyn RuleRequestRepository>,
        device_service: Arc<dyn DeviceService>,
    ) -> Self {
        Self {
            requests,
            device_service,
        }
    }

    /// Resolve the caller's device id (UUID string) from its source IP, or
    /// `NotFound` if the IP doesn't map to a known device.
    async fn device_id_for_ip(&self, ip: &str) -> Result<String, AppError> {
        let resp = self.device_service.get_device_for_ip(ip).await?;
        let device = resp
            .device
            .ok_or_else(|| AppError::NotFound("device not found for this IP".to_owned()))?;
        Ok(device.id.to_string())
    }
}

#[async_trait]
impl RuleRequestService for RuleRequestServiceImpl {
    async fn create_for_ip(
        &self,
        ip: &str,
        kind: RuleRequestKind,
        domain: &str,
        reason: Option<String>,
    ) -> Result<DeviceRuleRequest, AppError> {
        let device_id = self.device_id_for_ip(ip).await?;

        let domain = domain.trim().to_lowercase();
        if domain.is_empty() {
            return Err(AppError::BadRequest("domain is required".to_owned()));
        }
        let reason = reason
            .map(|r| r.trim().to_owned())
            .filter(|r| !r.is_empty());

        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.requests
            .insert(&id, &device_id, kind, &domain, reason.as_deref(), &now)
            .await
            .map_err(AppError::Internal)
    }

    async fn list_for_ip(&self, ip: &str) -> Result<Vec<DeviceRuleRequest>, AppError> {
        let device_id = self.device_id_for_ip(ip).await?;
        self.requests
            .list_by_device(&device_id)
            .await
            .map_err(AppError::Internal)
    }

    async fn list(
        &self,
        status: Option<RuleRequestStatus>,
    ) -> Result<Vec<DeviceRuleRequest>, AppError> {
        auth_context::require_admin()?;
        self.requests
            .list_all(status)
            .await
            .map_err(AppError::Internal)
    }

    async fn decide(
        &self,
        id: &str,
        status: RuleRequestStatus,
    ) -> Result<DeviceRuleRequest, AppError> {
        auth_context::require_admin()?;
        if !status.is_decision() {
            return Err(AppError::BadRequest(
                "status must be approved or rejected".to_owned(),
            ));
        }
        let admin_id = match auth_context::try_current() {
            Some(AuthContext::Admin { admin_id }) => admin_id.to_string(),
            _ => return Err(AppError::Forbidden("admin privileges required".to_owned())),
        };
        let now = Utc::now().to_rfc3339();
        self.requests
            .update_status(id, status, &admin_id, &now)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound("rule request not found".to_owned()))
    }
}
