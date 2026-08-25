use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;
use wardnet_common::access_request::{
    AccessRequestKind, AccessRequestStatus, ApprovalParams, DeviceAccessRequest,
};
use wardnet_common::auth::{AuthContext, UserRole};
use wardnet_common::event::WardnetEvent;
use wardnetd_data::repository::{AccessRequestRepository, DuplicateOpenAccessRequestError};

use crate::access_request::approver::ApproverRegistry;
use crate::auth_context;
use crate::device::DeviceService;
use crate::error::AppError;
use crate::event::EventPublisher;
use crate::private_dns::PrivateDnsService;

/// Service for the household "ask the admin" access-request inbox.
///
/// Device-scoped methods resolve the caller's device by IP through
/// [`DeviceService`] (so this service never holds the device repository).
/// Admin-scoped methods gate on [`auth_context::require_admin`].
#[async_trait]
pub trait AccessRequestService: Send + Sync {
    /// Create a pending request from the device resolved by `ip`.
    async fn create_for_ip(
        &self,
        ip: &str,
        kind: AccessRequestKind,
        domain: Option<String>,
        reason: Option<String>,
    ) -> Result<DeviceAccessRequest, AppError>;

    /// List the requests made by the device resolved by `ip`, newest first.
    async fn list_for_ip(&self, ip: &str) -> Result<Vec<DeviceAccessRequest>, AppError>;

    /// Admin: list all requests, optionally filtered by status.
    async fn list(
        &self,
        status: Option<AccessRequestStatus>,
    ) -> Result<Vec<DeviceAccessRequest>, AppError>;

    /// Admin: record an approve/reject decision for a request.
    ///
    /// On approval the registered approver for the request's kind runs first;
    /// a kind with no approver is record-only.
    async fn decide(
        &self,
        id: &str,
        status: AccessRequestStatus,
        params: Option<ApprovalParams>,
    ) -> Result<DeviceAccessRequest, AppError>;

    /// Resolve a device's pending request of `kind`, for reconciliation driven
    /// by the event bus rather than by an admin acting on the inbox.
    ///
    /// Admin-gated like every other write here, per `.agents/auth.md`'s hard
    /// requirement. The caller is `AccessRequestListener`, which has no request
    /// context of its own and therefore runs this under
    /// [`AuthContext::system`] — the same wrapper the other bus listeners use.
    /// Documenting the exception was not enough on its own: this method is
    /// reachable through `AppState::access_request_service()`, so leaving it
    /// ungated meant a future handler could wire an unauthenticated write to
    /// the decision audit trail with nothing failing to warn them.
    async fn resolve_pending(
        &self,
        device_id: Uuid,
        kind: AccessRequestKind,
        status: AccessRequestStatus,
        decided_by: Option<String>,
    ) -> Result<Option<DeviceAccessRequest>, AppError>;
}

pub struct AccessRequestServiceImpl {
    requests: Arc<dyn AccessRequestRepository>,
    device_service: Arc<dyn DeviceService>,
    private_dns: Arc<dyn PrivateDnsService>,
    approvers: ApproverRegistry,
    events: Arc<dyn EventPublisher>,
}

impl AccessRequestServiceImpl {
    #[must_use]
    pub fn new(
        requests: Arc<dyn AccessRequestRepository>,
        device_service: Arc<dyn DeviceService>,
        private_dns: Arc<dyn PrivateDnsService>,
        approvers: ApproverRegistry,
        events: Arc<dyn EventPublisher>,
    ) -> Self {
        Self {
            requests,
            device_service,
            private_dns,
            approvers,
            events,
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

    /// Normalise and check the payload against the kind's contract, mirroring
    /// the database `CHECK`.
    fn normalise_domain(
        kind: AccessRequestKind,
        domain: Option<String>,
    ) -> Result<Option<String>, AppError> {
        let domain = domain
            .map(|d| d.trim().to_lowercase())
            .filter(|d| !d.is_empty());

        if kind.requires_domain() && domain.is_none() {
            return Err(AppError::BadRequest("domain is required".to_owned()));
        }
        if !kind.requires_domain() && domain.is_some() {
            return Err(AppError::BadRequest(format!(
                "a {} request does not take a domain",
                kind.as_str()
            )));
        }
        Ok(domain)
    }

    /// Refuse a request the admin could not act on.
    ///
    /// Private DNS must already be enabled network-wide, because `grant_device`
    /// requires it — a request raised while the feature is off would sit in the
    /// inbox as a trap, un-approvable until the admin turns on a Premium
    /// feature. The user PWA therefore only offers the button in the
    /// enabled-but-ungranted state, and this is the server-side half of that.
    ///
    /// `is_enabled` is admin-gated but the caller here is an unauthenticated,
    /// device-keyed request, so it runs under the system context — the same
    /// wrapper `GET /api/private-dns/me` already uses.
    async fn check_requestable(
        &self,
        kind: AccessRequestKind,
        device_id: &str,
    ) -> Result<(), AppError> {
        if kind != AccessRequestKind::PrivateDns {
            return Ok(());
        }
        let enabled =
            auth_context::with_context(AuthContext::system(), self.private_dns.is_enabled())
                .await?;
        if !enabled {
            return Err(AppError::Conflict(
                "Private DNS is not enabled on this network".to_owned(),
            ));
        }

        // Refuse a device that already holds a grant.
        //
        // Not merely redundant — this is the one phantom-pending row
        // `AccessRequestListener` cannot clean up. The listener resolves
        // requests that were pending *when the grant happened*; a request filed
        // *after* it has already missed the event and no future one will name
        // this device, so the row would nag the admin until decided by hand.
        // Reachable in practice: the PWA's cached `me.granted` is stale for as
        // long as the member hasn't foregrounded the app since the admin
        // granted from the Remote Access card.
        let uuid = Uuid::parse_str(device_id).map_err(|e| {
            AppError::Internal(anyhow::anyhow!("device id {device_id} is not a UUID: {e}"))
        })?;
        let granted =
            auth_context::with_context(AuthContext::system(), self.private_dns.device_grant(uuid))
                .await?;
        if granted.is_some() {
            return Err(AppError::Conflict(
                "this device already has Private DNS".to_owned(),
            ));
        }
        Ok(())
    }
}

#[async_trait]
impl AccessRequestService for AccessRequestServiceImpl {
    async fn create_for_ip(
        &self,
        ip: &str,
        kind: AccessRequestKind,
        domain: Option<String>,
        reason: Option<String>,
    ) -> Result<DeviceAccessRequest, AppError> {
        let device_id = self.device_id_for_ip(ip).await?;
        let domain = Self::normalise_domain(kind, domain)?;
        self.check_requestable(kind, &device_id).await?;

        let reason = reason
            .map(|r| r.trim().to_owned())
            .filter(|r| !r.is_empty());

        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let request = self
            .requests
            .insert(
                &id,
                &device_id,
                kind,
                domain.as_deref(),
                reason.as_deref(),
                &now,
            )
            .await
            .map_err(|e| {
                if e.downcast_ref::<DuplicateOpenAccessRequestError>()
                    .is_some()
                {
                    // Asking twice is not an error the member should see as a
                    // fault — they already have one waiting.
                    AppError::Conflict("this device already has a pending request".to_owned())
                } else {
                    AppError::Internal(e)
                }
            })?;

        // Tell listeners — the push subsystem notifies admins of the new
        // request. Published after the insert so a subscriber can always read
        // the request back.
        self.events.publish(WardnetEvent::AccessRequestCreated {
            request_id: request.id.clone(),
            device_id: request.device_id.clone(),
            kind: request.kind,
            domain: request.domain.clone(),
            timestamp: Utc::now(),
        });

        Ok(request)
    }

    async fn list_for_ip(&self, ip: &str) -> Result<Vec<DeviceAccessRequest>, AppError> {
        let device_id = self.device_id_for_ip(ip).await?;
        self.requests
            .list_by_device(&device_id)
            .await
            .map_err(AppError::Internal)
    }

    async fn list(
        &self,
        status: Option<AccessRequestStatus>,
    ) -> Result<Vec<DeviceAccessRequest>, AppError> {
        auth_context::require_admin()?;
        self.requests
            .list_all(status)
            .await
            .map_err(AppError::Internal)
    }

    async fn decide(
        &self,
        id: &str,
        status: AccessRequestStatus,
        params: Option<ApprovalParams>,
    ) -> Result<DeviceAccessRequest, AppError> {
        auth_context::require_admin()?;
        if !status.is_decision() {
            return Err(AppError::BadRequest(
                "status must be approved or rejected".to_owned(),
            ));
        }
        // Enumerated rather than `_ =>`: this value is the audit record of *who*
        // decided the request, so a new principal must be named here explicitly.
        //
        // The nil check excludes `AuthContext::system()`, which is a `User`
        // carrying the admin role, so it clears `require_admin` and would
        // otherwise stamp `00000000-…-0000` into `decided_by` — an id ADR-0031
        // guarantees has no `users` row, leaving the inbox showing a decision
        // made by nobody. A decision needs a human; the daemon reconciling
        // state goes through `resolve_pending` instead. Mirrors the same guard
        // in `PrivateDnsServiceImpl::grant_device`.
        let admin_id = match auth_context::try_current() {
            Some(AuthContext::User(user))
                if user.role() == UserRole::Admin && user.user_id() != Uuid::nil() =>
            {
                user.user_id().to_string()
            }
            Some(AuthContext::User(_) | AuthContext::Device { .. } | AuthContext::Anonymous)
            | None => {
                return Err(AppError::Forbidden("admin privileges required".to_owned()));
            }
        };

        let request = self
            .requests
            .find_by_id(id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound("access request not found".to_owned()))?;

        // Deciding an already-decided request would silently rewrite the audit
        // trail — and, on approval, run the side effect a second time.
        if request.status.is_decision() {
            return Err(AppError::Conflict(format!(
                "this request was already {}",
                request.status.as_str()
            )));
        }

        // The side effect runs *before* the decision is recorded: if the
        // approver fails, the request stays pending and the admin can retry,
        // rather than reading "approved" against something that never happened.
        if status == AccessRequestStatus::Approved
            && let Some(approver) = self.approvers.get(request.kind)
        {
            approver.approve(&request, params.as_ref()).await?;
        }

        let now = Utc::now().to_rfc3339();
        let updated = self
            .requests
            .update_status(id, status, &admin_id, &now)
            .await
            .map_err(AppError::Internal)?;

        match updated {
            Some(request) => Ok(request),
            // The write is guarded on `pending`, so `None` here means either
            // the id is unknown — impossible, we read it above — or the
            // approver's own grant raced its listener home and resolved the
            // request first. That is the outcome the admin asked for, so
            // return what the listener wrote rather than inventing a 404 or
            // stamping this admin over the recorded decider.
            None => self
                .requests
                .find_by_id(id)
                .await
                .map_err(AppError::Internal)?
                .ok_or_else(|| AppError::NotFound("access request not found".to_owned())),
        }
    }

    async fn resolve_pending(
        &self,
        device_id: Uuid,
        kind: AccessRequestKind,
        status: AccessRequestStatus,
        decided_by: Option<String>,
    ) -> Result<Option<DeviceAccessRequest>, AppError> {
        auth_context::require_admin()?;
        let now = Utc::now().to_rfc3339();
        self.requests
            .resolve_pending(
                &device_id.to_string(),
                kind,
                status,
                decided_by.as_deref(),
                &now,
            )
            .await
            .map_err(AppError::Internal)
    }
}
