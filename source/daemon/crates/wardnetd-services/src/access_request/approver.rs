use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;
use wardnet_common::access_request::{AccessRequestKind, ApprovalParams, DeviceAccessRequest};

use crate::error::AppError;
use crate::private_dns::PrivateDnsService;

/// What approving a request of one kind actually *does*.
///
/// The inbox is generic, but approval is not: recording "approved" on a
/// Private-DNS request has to mint the grant, while recording it on an
/// `allow` / `block` request currently changes nothing. Branching on the kind
/// inside the service would put every future request type's side effects in one
/// growing `match`, so approval dispatches through this registry instead — the
/// same shape as `AnomalyDetector`, one implementation per catalogue entry.
///
/// **A kind with no registered approver is record-only.** That is not an
/// oversight, it is the current, deliberate state of `allow` / `block`: the
/// admin still applies the DNS filter rule by hand, because the rule lands on a
/// *profile* and choosing that profile has household-wide blast radius. Giving
/// them an approver is a follow-up issue, and it is exactly one registration.
#[async_trait]
pub trait AccessRequestApprover: Send + Sync {
    /// The catalogue entry this approver owns.
    fn kind(&self) -> AccessRequestKind;

    /// Carry out the approval.
    ///
    /// Runs **before** the decision is recorded, so returning an error leaves
    /// the request `pending` rather than marking it approved against an effect
    /// that never happened. Must be safe to run when the end state already
    /// holds — an admin can reach the same outcome from another surface.
    async fn approve(
        &self,
        request: &DeviceAccessRequest,
        params: Option<&ApprovalParams>,
    ) -> Result<(), AppError>;
}

/// Approving a Private-DNS request mints the device's grant (issue #919).
pub struct PrivateDnsApprover {
    private_dns: Arc<dyn PrivateDnsService>,
}

impl PrivateDnsApprover {
    #[must_use]
    pub fn new(private_dns: Arc<dyn PrivateDnsService>) -> Self {
        Self { private_dns }
    }
}

#[async_trait]
impl AccessRequestApprover for PrivateDnsApprover {
    fn kind(&self) -> AccessRequestKind {
        AccessRequestKind::PrivateDns
    }

    async fn approve(
        &self,
        request: &DeviceAccessRequest,
        _params: Option<&ApprovalParams>,
    ) -> Result<(), AppError> {
        let device_id = Uuid::parse_str(&request.device_id).map_err(|e| {
            AppError::Internal(anyhow::anyhow!(
                "access request {} has an unparseable device id: {e}",
                request.id
            ))
        })?;

        match self.private_dns.grant_device(device_id).await {
            Ok(_) => {}
            // The device already holds a grant — the end state the admin asked
            // for, so approving is a success rather than a conflict. This is
            // reachable whenever a grant was made from the Remote Access card
            // while the request sat pending, and answering 409 there would make
            // the admin dismiss a request whose outcome already happened.
            Err(AppError::Conflict(msg)) if msg.contains("already has a Private DNS grant") => {}
            Err(e) => return Err(e),
        }

        // Tell the member their ask was answered.
        //
        // Granting from the Remote Access card leaves the push to the admin's
        // "Send to device" button, because there the admin chose the moment. An
        // approval *is* that moment: someone asked and is waiting, so the
        // notification is the answer rather than an unprompted interruption. It
        // also deep-links to `/settings#private-dns`, where the setup steps now
        // are.
        //
        // Best-effort on purpose. The grant is already persisted and working,
        // and `notify_device` reports a member with no subscription as
        // `Ok(false)` rather than an error — so a delivery problem must not
        // fail an approval and leave the request `pending` against a grant that
        // exists. The member's card still flips on next foreground.
        if let Err(e) = self.private_dns.notify_device(device_id).await {
            tracing::warn!(
                error = %e,
                %device_id,
                "granted Private DNS but could not notify the device"
            );
        }

        Ok(())
    }
}

/// Kind → approver lookup. A missing entry means record-only.
pub struct ApproverRegistry {
    by_kind: HashMap<AccessRequestKind, Arc<dyn AccessRequestApprover>>,
}

impl ApproverRegistry {
    /// Build a registry, keying each approver by the kind it declares.
    #[must_use]
    pub fn new(approvers: Vec<Arc<dyn AccessRequestApprover>>) -> Self {
        Self {
            by_kind: approvers.into_iter().map(|a| (a.kind(), a)).collect(),
        }
    }

    /// The approver for `kind`, or `None` when approval is record-only.
    #[must_use]
    pub fn get(&self, kind: AccessRequestKind) -> Option<&Arc<dyn AccessRequestApprover>> {
        self.by_kind.get(&kind)
    }
}

impl Default for ApproverRegistry {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}
