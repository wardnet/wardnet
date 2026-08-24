use async_trait::async_trait;
use wardnet_common::access_request::{AccessRequestKind, AccessRequestStatus, DeviceAccessRequest};

/// Marker error for a duplicate open request.
///
/// A breach of the open-request partial unique index is a business-rule
/// conflict, not a generic DB failure, so the service layer downcasts on it and
/// maps it to a `409 Conflict` rather than a raw `500`. A primary-key collision
/// is deliberately NOT wrapped in this and surfaces as an ordinary error: the
/// service mints request ids itself, so a collision indicates a genuine
/// internal fault.
#[derive(Debug)]
pub struct DuplicateOpenAccessRequestError;

impl std::fmt::Display for DuplicateOpenAccessRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("device already has an open request of this kind")
    }
}

impl std::error::Error for DuplicateOpenAccessRequestError {}

#[async_trait]
pub trait AccessRequestRepository: Send + Sync {
    /// Insert a new pending request and return the stored row.
    ///
    /// `domain` is `None` for kinds that name none. Returns
    /// [`DuplicateOpenAccessRequestError`] when the partial unique index
    /// rejects a second open request of a kind that permits only one.
    async fn insert(
        &self,
        id: &str,
        device_id: &str,
        kind: AccessRequestKind,
        domain: Option<&str>,
        reason: Option<&str>,
        created_at: &str,
    ) -> anyhow::Result<DeviceAccessRequest>;

    /// All requests made by a single device, newest first.
    async fn list_by_device(&self, device_id: &str) -> anyhow::Result<Vec<DeviceAccessRequest>>;

    /// All requests, optionally filtered by status, newest first.
    async fn list_all(
        &self,
        status: Option<AccessRequestStatus>,
    ) -> anyhow::Result<Vec<DeviceAccessRequest>>;

    /// Fetch a single request by id.
    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<DeviceAccessRequest>>;

    /// Record an admin decision on a *pending* request.
    ///
    /// Guarded on `status = 'pending'` for the same reason
    /// [`AccessRequestRepository::resolve_pending`] is: both can decide the
    /// same row, and the loser must not overwrite the winner's `decided_by`.
    /// Returns `None` when the id is unknown **or** the request was already
    /// decided — the caller re-reads to find out which.
    async fn update_status(
        &self,
        id: &str,
        status: AccessRequestStatus,
        decided_by: &str,
        decided_at: &str,
    ) -> anyhow::Result<Option<DeviceAccessRequest>>;

    /// Resolve a device's *pending* request of `kind` to `status`.
    ///
    /// Guarded on `status = 'pending'`, which is what makes it idempotent: the
    /// approval path writes the decision itself, and `AccessRequestListener`
    /// writes it again when the grant event arrives. Whichever lands second is
    /// a no-op rather than an overwrite, so the original `decided_by` survives.
    /// Returns the resolved row, or `None` when there was nothing pending.
    async fn resolve_pending(
        &self,
        device_id: &str,
        kind: AccessRequestKind,
        status: AccessRequestStatus,
        decided_by: Option<&str>,
        decided_at: &str,
    ) -> anyhow::Result<Option<DeviceAccessRequest>>;
}
