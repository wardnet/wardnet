use async_trait::async_trait;

/// Marker error returned by [`PrivateDnsGrantRepository::insert`] when the
/// insert fails specifically because the `device_id` uniqueness index
/// (`idx_private_dns_grants_device_id`) is violated — i.e. the device already
/// holds a Private-DNS grant (one grant per device, issue #912).
///
/// This is a business-rule conflict, not a generic DB failure, so the service
/// layer downcasts on it and maps it to a `409 Conflict` rather than a raw
/// `500`. A uniqueness violation on `token` is deliberately NOT wrapped in
/// this and surfaces as an ordinary error: the daemon mints tokens itself
/// from a CSPRNG and a collision indicates a genuine internal fault.
#[derive(Debug)]
pub struct DeviceAlreadyGrantedPrivateDnsError;

impl std::fmt::Display for DeviceAlreadyGrantedPrivateDnsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("device already has a Private DNS grant")
    }
}

impl std::error::Error for DeviceAlreadyGrantedPrivateDnsError {}

/// One per-device Private-DNS grant as persisted in `private_dns_grants`.
///
/// The `token` is the secret hostname label the granted device dials over
/// `DoT`; it is minted once at grant time and never regenerated — revoke and
/// re-grant is the rotation story.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateDnsGrantRow {
    /// Grant UUID (primary key).
    pub id: String,
    /// The `Device` this grant belongs to. `UNIQUE` — one grant per device.
    pub device_id: String,
    /// High-entropy base-32 secret hostname label. `UNIQUE` — the `DoT`
    /// listener resolves SNI token → device through it.
    pub token: String,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
}

/// Persistence for Private-DNS grants (issue #912).
///
/// Each grant links a secret `DoT` hostname token (`UNIQUE`) to an
/// already-managed [`Device`](wardnet_common::device::Device) (`device_id`,
/// `UNIQUE`). The service layer owns token minting and the device link; this
/// trait only reads and writes rows.
#[async_trait]
pub trait PrivateDnsGrantRepository: Send + Sync {
    /// Insert a new grant row.
    ///
    /// A `device_id` uniqueness violation is a normal, expected outcome (two
    /// concurrent grant attempts for the same device can race past the
    /// service-layer pre-check): implementations MUST make it detectable by
    /// returning an error that downcasts to
    /// [`DeviceAlreadyGrantedPrivateDnsError`], so the caller can surface a
    /// clean `409 Conflict` instead of a `500`. Any other failure (including
    /// a uniqueness violation on `token`) is returned as an ordinary error.
    async fn insert(&self, row: &PrivateDnsGrantRow) -> anyhow::Result<()>;

    /// Fetch a single grant by id.
    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<PrivateDnsGrantRow>>;

    /// Fetch the single grant linked to the given device, if one exists.
    ///
    /// At most one row can match — `device_id` is `UNIQUE` (one grant per
    /// device).
    async fn find_by_device_id(
        &self,
        device_id: &str,
    ) -> anyhow::Result<Option<PrivateDnsGrantRow>>;

    /// Fetch the single grant carrying the given token, if one exists. This
    /// is the SNI-authentication lookup the `DoT` listener performs on every
    /// accepted connection.
    async fn find_by_token(&self, token: &str) -> anyhow::Result<Option<PrivateDnsGrantRow>>;

    /// Return every grant, oldest first.
    async fn find_all(&self) -> anyhow::Result<Vec<PrivateDnsGrantRow>>;

    /// Delete a grant by id. No-op when the id is absent — callers that need
    /// to distinguish "not found" resolve that via `find_by_id` first.
    async fn delete(&self, id: &str) -> anyhow::Result<()>;
}
