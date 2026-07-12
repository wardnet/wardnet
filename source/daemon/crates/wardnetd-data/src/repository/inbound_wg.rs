use async_trait::async_trait;

/// Marker error returned by [`InboundWgPeerRepository::insert`] when the insert
/// fails specifically because the `device_id` uniqueness index
/// (`idx_inbound_wg_peers_device_id`) is violated — i.e. the device already has
/// a remote-access credential (one credential per device, issue #810).
///
/// This is a business-rule conflict, not a generic DB failure, so the service
/// layer downcasts on it and maps it to a `409 Conflict` rather than a raw
/// `500`. Uniqueness violations on the *other* unique columns (`public_key`,
/// `allowed_ip`) are deliberately NOT wrapped in this and surface as ordinary
/// errors, since those indicate a genuine internal fault (the daemon generates
/// those values itself and they should never collide).
#[derive(Debug)]
pub struct DeviceAlreadyGrantedError;

impl std::fmt::Display for DeviceAlreadyGrantedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("device already has an inbound WireGuard credential")
    }
}

impl std::error::Error for DeviceAlreadyGrantedError {}

/// One admitted inbound-`WireGuard` peer as persisted in `inbound_wg_peers`.
///
/// The peer's private key is deliberately absent — it is generated on the
/// daemon, returned once to the admin, and never stored (see the migration
/// rationale). Only the public key and the allocated `/32` travel with the row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboundWgPeerRow {
    /// Peer UUID (primary key).
    pub id: String,
    /// Base64 `WireGuard` public key. Unique across peers.
    pub public_key: String,
    /// Fixed CIDR inside the inbound tunnel subnet, e.g. `10.100.64.2/32`.
    pub allowed_ip: String,
    /// Human-facing label.
    pub name: String,
    /// Whether the peer is currently admitted onto the server interface.
    pub enabled: bool,
    /// RFC 3339 creation timestamp.
    pub created_at: String,
    /// The `Device` this credential grants remote access to (issue #810).
    ///
    /// A remote-access grant is a property of an already-managed device, so
    /// the application layer always sets this at peer-creation time. Modelled
    /// as `Option` only because the DB column is nullable (`SQLite` cannot add a
    /// `NOT NULL` column without a constant default); in practice every row
    /// written from #810 onward carries it. `UNIQUE` — one credential per
    /// device.
    pub device_id: Option<String>,
}

/// Persistence for inbound-`WireGuard` peers (issues #809, #810).
///
/// Each peer is a remote-access credential linked (`device_id`, `UNIQUE`) to
/// an already-managed [`Device`](wardnet_common::device::Device). The service
/// layer owns IP allocation, keypair generation, and the device link; this
/// trait only reads and writes rows.
#[async_trait]
pub trait InboundWgPeerRepository: Send + Sync {
    /// Insert a new peer row.
    ///
    /// A `device_id` uniqueness violation is a normal, expected outcome (two
    /// concurrent grant attempts for the same device can race past the
    /// service-layer pre-check): implementations MUST make it detectable by
    /// returning an error that downcasts to [`DeviceAlreadyGrantedError`], so
    /// the caller can surface a clean `409 Conflict` instead of a `500`. Any
    /// other failure (including uniqueness violations on `public_key` /
    /// `allowed_ip`) is returned as an ordinary error.
    async fn insert(&self, row: &InboundWgPeerRow) -> anyhow::Result<()>;

    /// Fetch a single peer by id.
    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<InboundWgPeerRow>>;

    /// Fetch the single peer linked to the given device, if one exists.
    ///
    /// At most one row can match — `device_id` is `UNIQUE` (one credential per
    /// device, issue #810).
    async fn find_by_device_id(&self, device_id: &str) -> anyhow::Result<Option<InboundWgPeerRow>>;

    /// Return every peer, oldest first.
    async fn find_all(&self) -> anyhow::Result<Vec<InboundWgPeerRow>>;

    /// Return only the currently-enabled peers, oldest first. Used by the
    /// enable / startup-restore paths to re-add live peers to the interface.
    async fn find_enabled(&self) -> anyhow::Result<Vec<InboundWgPeerRow>>;

    /// Delete a peer by id. No-op when the id is absent.
    async fn delete(&self, id: &str) -> anyhow::Result<()>;

    /// Set a peer's `enabled` flag. No-op when the id is absent — callers
    /// that need to distinguish "not found" resolve that via `find_by_id`
    /// first, the same pattern `remove_peer`/`add_peer` already use.
    async fn set_enabled(&self, id: &str, enabled: bool) -> anyhow::Result<()>;
}
