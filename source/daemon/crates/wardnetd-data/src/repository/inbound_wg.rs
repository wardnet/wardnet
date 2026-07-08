use async_trait::async_trait;

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
}

/// Persistence for inbound-`WireGuard` peers (issue #809).
///
/// A standalone CRUD store: peers are not wired into the device/routing/zone
/// model, so there is no cross-table coupling here. The service layer owns IP
/// allocation and keypair generation; this trait only reads and writes rows.
#[async_trait]
pub trait InboundWgPeerRepository: Send + Sync {
    /// Insert a new peer row.
    async fn insert(&self, row: &InboundWgPeerRow) -> anyhow::Result<()>;

    /// Fetch a single peer by id.
    async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<InboundWgPeerRow>>;

    /// Return every peer, oldest first.
    async fn find_all(&self) -> anyhow::Result<Vec<InboundWgPeerRow>>;

    /// Return only the currently-enabled peers, oldest first. Used by the
    /// enable / startup-restore paths to re-add live peers to the interface.
    async fn find_enabled(&self) -> anyhow::Result<Vec<InboundWgPeerRow>>;

    /// Delete a peer by id. No-op when the id is absent.
    async fn delete(&self, id: &str) -> anyhow::Result<()>;
}
