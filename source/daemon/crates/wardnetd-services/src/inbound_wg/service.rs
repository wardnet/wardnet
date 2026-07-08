use std::collections::HashSet;
use std::net::Ipv4Addr;
use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine;
use ipnetwork::IpNetwork;
use uuid::Uuid;
use wardnet_common::api::{
    AddInboundWgPeerResponse, InboundWgConfigResponse, InboundWgPeerSummary,
};

use crate::auth_context;
use crate::error::AppError;
use crate::inbound_wg::interface::{
    InboundWgInterface, InboundWgPeerConfig, InboundWgServerConfig,
};
use crate::inbound_wg::key_store::{ServerKeyStore, ServerKeyStoreAdapter};
use crate::inbound_wg::keygen::generate_keypair;
use crate::routing::FirewallManager;
use wardnetd_data::repository::SystemConfigRepository;
use wardnetd_data::repository::inbound_wg::{InboundWgPeerRepository, InboundWgPeerRow};
use wardnetd_data::secret_store::SecretStore;

/// Fixed name of the inbound `WireGuard` server interface.
///
/// The `wg_wardin0` name deliberately shares the `wg_ward` prefix that the
/// firewall's zone-egress gate matches (`TUNNEL_IFACE_PREFIX` in
/// `wardnetd::firewall_netlink`), per issue #809 — so it is covered by the
/// existing outbound-tunnel zone rules today with no new firewall code.
///
/// This shared prefix is currently **INERT**: zone-egress-drop rules are keyed
/// by `saddr == device_ip`, and inbound peers are not `Device` rows yet
/// (issue #810 territory), so no `ZoneRules` can ever be computed for a peer IP.
/// When #810 wires zone enforcement to inbound peers, revisit whether a
/// zone-denied peer's egress-drop rule should actually match `wg_wardin0` — it
/// is the peer's *inbound* attachment point, not an outbound-tunnel egress path,
/// so it almost certainly should NOT, and will need explicit handling then.
/// See the matching note at `TUNNEL_IFACE_PREFIX`.
pub const INBOUND_WG_INTERFACE: &str = "wg_wardin0";

/// Inbound tunnel subnet. The server owns `.1`; peers are allocated `/32`s
/// sequentially from `.2` upward.
const SUBNET_PREFIX: [u8; 3] = [10, 100, 64];
/// Prefix length of the inbound tunnel subnet (`10.100.64.0/24`).
const SUBNET_MASK: u8 = 24;
/// Last octet reserved for the server itself.
const SERVER_HOST: u8 = 1;

/// Inbound (multi-peer) `WireGuard` server management (issue #809).
///
/// Orchestrates the server interface, its singleton keypair, the peer data
/// model, IP allocation from the inbound subnet, and the firewall
/// masquerade/accept rules. Explicitly NOT wired into the device / routing /
/// zone model — peers get a fixed static route only.
#[async_trait]
pub trait InboundWgService: Send + Sync {
    /// Enable or disable the inbound server and set its listen port.
    ///
    /// On enable: generates + persists the server keypair if none exists,
    /// stands up the interface, installs the masquerade + accept firewall
    /// rules, persists config, and re-adds every enabled peer. On disable:
    /// removes the firewall rules, tears the interface down, and marks disabled
    /// — peer rows are preserved.
    async fn set_config(
        &self,
        enabled: bool,
        listen_port: u16,
    ) -> Result<InboundWgConfigResponse, AppError>;

    /// Admit a new peer: generate a keypair, allocate an IP, persist the row
    /// (public key only), add it to the interface, and return the private key
    /// **once**. Rejected when the server is disabled.
    async fn add_peer(&self, name: String) -> Result<AddInboundWgPeerResponse, AppError>;

    /// Remove a peer by id from both the interface and the database.
    async fn remove_peer(&self, id: Uuid) -> Result<(), AppError>;

    /// List every peer (no private keys).
    async fn list_peers(&self) -> Result<Vec<InboundWgPeerSummary>, AppError>;

    /// Startup reconciliation: if the server is enabled, stand the interface up
    /// and re-add every enabled peer. Runs before the system is ready, so it is
    /// intentionally exempt from the `require_admin` guard (see `.agents/auth.md`).
    async fn reconcile(&self) -> Result<(), AppError>;
}

/// Default implementation of [`InboundWgService`].
pub struct InboundWgServiceImpl {
    peers: Arc<dyn InboundWgPeerRepository>,
    system_config: Arc<dyn SystemConfigRepository>,
    keys: Arc<dyn ServerKeyStore>,
    interface: Arc<dyn InboundWgInterface>,
    firewall: Arc<dyn FirewallManager>,
}

impl InboundWgServiceImpl {
    /// Construct with a shared [`SecretStore`], wrapping it in the narrow
    /// server key-store facade internally (mirrors `TunnelServiceImpl::new`).
    pub fn new(
        peers: Arc<dyn InboundWgPeerRepository>,
        system_config: Arc<dyn SystemConfigRepository>,
        secret_store: Arc<dyn SecretStore>,
        interface: Arc<dyn InboundWgInterface>,
        firewall: Arc<dyn FirewallManager>,
    ) -> Self {
        let keys: Arc<dyn ServerKeyStore> = Arc::new(ServerKeyStoreAdapter::new(secret_store));
        Self {
            peers,
            system_config,
            keys,
            interface,
            firewall,
        }
    }

    /// Test constructor that accepts a pre-built [`ServerKeyStore`].
    #[cfg(test)]
    pub(crate) fn with_key_store(
        peers: Arc<dyn InboundWgPeerRepository>,
        system_config: Arc<dyn SystemConfigRepository>,
        keys: Arc<dyn ServerKeyStore>,
        interface: Arc<dyn InboundWgInterface>,
        firewall: Arc<dyn FirewallManager>,
    ) -> Self {
        Self {
            peers,
            system_config,
            keys,
            interface,
            firewall,
        }
    }

    /// Base64-encode a raw 32-byte key.
    fn encode_key(bytes: &[u8; 32]) -> String {
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    /// Decode a base64 `WireGuard` key into a raw 32-byte array.
    fn decode_key(b64: &str) -> Result<[u8; 32], AppError> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid base64 key: {e}")))?;
        bytes
            .try_into()
            .map_err(|_| AppError::Internal(anyhow::anyhow!("WireGuard key must be 32 bytes")))
    }

    /// The server interface's own address (`10.100.64.1/24`).
    fn server_address() -> IpNetwork {
        let ip = Ipv4Addr::new(
            SUBNET_PREFIX[0],
            SUBNET_PREFIX[1],
            SUBNET_PREFIX[2],
            SERVER_HOST,
        );
        IpNetwork::new(ip.into(), SUBNET_MASK).expect("valid /24 server address")
    }

    /// Load the persisted server private key, or generate + persist a fresh
    /// keypair if none exists. Returns the raw private key and the base64
    /// public key. The public key is persisted to `system_config`.
    async fn ensure_server_keypair(&self) -> Result<([u8; 32], String), AppError> {
        if let Some(priv_b64) = self.keys.load_key().await.map_err(AppError::Internal)? {
            let private = Self::decode_key(&priv_b64)?;
            let public = x25519_dalek::x25519(private, x25519_dalek::X25519_BASEPOINT_BYTES);
            return Ok((private, Self::encode_key(&public)));
        }
        let (private, public) = generate_keypair();
        let priv_b64 = Self::encode_key(&private);
        let pub_b64 = Self::encode_key(&public);
        self.keys
            .save_key(&priv_b64)
            .await
            .map_err(AppError::Internal)?;
        self.system_config
            .set_inbound_wg_server_pubkey(&pub_b64)
            .await
            .map_err(AppError::Internal)?;
        Ok((private, pub_b64))
    }

    /// Turn a stored peer row into an [`InboundWgPeerConfig`] for the interface.
    fn peer_row_to_config(row: &InboundWgPeerRow) -> Result<InboundWgPeerConfig, AppError> {
        let public_key = Self::decode_key(&row.public_key)?;
        let allowed_ip: IpNetwork = row
            .allowed_ip
            .parse()
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid stored allowed_ip: {e}")))?;
        Ok(InboundWgPeerConfig {
            public_key,
            allowed_ips: vec![allowed_ip],
            preshared_key: None,
            persistent_keepalive: None,
        })
    }

    /// Allocate the next free peer address (`10.100.64.N/32`) by scanning the
    /// existing rows for the lowest unused host octet in `2..=254`.
    fn allocate_ip(existing: &[InboundWgPeerRow]) -> Result<String, AppError> {
        let used: HashSet<u8> = existing
            .iter()
            .filter_map(|r| r.allowed_ip.split('/').next())
            .filter_map(|ip| ip.parse::<Ipv4Addr>().ok())
            .map(|ip| ip.octets()[3])
            .collect();
        for host in (SERVER_HOST + 1)..=254 {
            if !used.contains(&host) {
                return Ok(format!(
                    "{}.{}.{}.{host}/32",
                    SUBNET_PREFIX[0], SUBNET_PREFIX[1], SUBNET_PREFIX[2]
                ));
            }
        }
        Err(AppError::Conflict(
            "inbound WireGuard subnet is full — no free address".to_owned(),
        ))
    }

    /// Bring the interface up from stored config and re-add every enabled peer.
    /// Peers that fail to add are logged and skipped, never fatal.
    async fn bring_up_server(
        &self,
        private_key: [u8; 32],
        listen_port: u16,
    ) -> Result<(), AppError> {
        self.interface
            .ensure_server(InboundWgServerConfig {
                interface_name: INBOUND_WG_INTERFACE.to_owned(),
                private_key,
                listen_port,
                address: vec![Self::server_address()],
            })
            .await
            .map_err(AppError::Internal)?;

        let enabled_peers = self
            .peers
            .find_enabled()
            .await
            .map_err(AppError::Internal)?;
        for row in &enabled_peers {
            let config = match Self::peer_row_to_config(row) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!(peer_id = %row.id, error = %e, "inbound-wg: skipping peer with invalid stored data");
                    continue;
                }
            };
            if let Err(e) = self.interface.add_peer(INBOUND_WG_INTERFACE, config).await {
                tracing::warn!(peer_id = %row.id, error = %e, "inbound-wg: failed to re-add peer, continuing");
            }
        }
        Ok(())
    }
}

#[async_trait]
impl InboundWgService for InboundWgServiceImpl {
    async fn set_config(
        &self,
        enabled: bool,
        listen_port: u16,
    ) -> Result<InboundWgConfigResponse, AppError> {
        auth_context::require_admin()?;

        if enabled {
            let (private_key, server_pubkey) = self.ensure_server_keypair().await?;

            self.bring_up_server(private_key, listen_port).await?;

            self.firewall
                .add_masquerade(INBOUND_WG_INTERFACE)
                .await
                .map_err(AppError::Internal)?;
            self.firewall
                .add_inbound_wg_accept(listen_port)
                .await
                .map_err(AppError::Internal)?;

            self.system_config
                .set_inbound_wg_enabled(true)
                .await
                .map_err(AppError::Internal)?;
            self.system_config
                .set_inbound_wg_listen_port(listen_port)
                .await
                .map_err(AppError::Internal)?;

            Ok(InboundWgConfigResponse {
                enabled: true,
                listen_port,
                server_public_key: Some(server_pubkey),
            })
        } else {
            self.firewall
                .remove_masquerade(INBOUND_WG_INTERFACE)
                .await
                .map_err(AppError::Internal)?;
            self.firewall
                .remove_inbound_wg_accept()
                .await
                .map_err(AppError::Internal)?;
            self.interface
                .tear_down_server(INBOUND_WG_INTERFACE)
                .await
                .map_err(AppError::Internal)?;

            self.system_config
                .set_inbound_wg_enabled(false)
                .await
                .map_err(AppError::Internal)?;

            let server_pubkey = self
                .system_config
                .inbound_wg_server_pubkey()
                .await
                .map_err(AppError::Internal)?;

            Ok(InboundWgConfigResponse {
                enabled: false,
                listen_port,
                server_public_key: server_pubkey,
            })
        }
    }

    async fn add_peer(&self, name: String) -> Result<AddInboundWgPeerResponse, AppError> {
        auth_context::require_admin()?;

        // Precondition first: no keygen / DB work if the server is off.
        if !self
            .system_config
            .inbound_wg_enabled()
            .await
            .map_err(AppError::Internal)?
        {
            return Err(AppError::Conflict(
                "inbound WireGuard server is disabled — enable it before adding peers".to_owned(),
            ));
        }

        let (private, public) = generate_keypair();
        let private_b64 = Self::encode_key(&private);
        let public_b64 = Self::encode_key(&public);

        let existing = self.peers.find_all().await.map_err(AppError::Internal)?;
        let allowed_ip = Self::allocate_ip(&existing)?;

        let id = Uuid::new_v4();
        let now = chrono::Utc::now();
        let row = InboundWgPeerRow {
            id: id.to_string(),
            public_key: public_b64.clone(),
            allowed_ip: allowed_ip.clone(),
            name: name.clone(),
            enabled: true,
            created_at: now.to_rfc3339(),
        };
        self.peers.insert(&row).await.map_err(AppError::Internal)?;

        let config = Self::peer_row_to_config(&row)?;
        if let Err(error) = self.interface.add_peer(INBOUND_WG_INTERFACE, config).await {
            // Compensating action: the interface refused the peer, so roll the
            // just-inserted row back rather than leaking an orphaned allocation
            // (the IP + public key would otherwise persist with no live peer, and
            // the private key — returned once, never stored — is unrecoverable).
            // A failed rollback is logged but the original error still propagates.
            if let Err(cleanup) = self.peers.delete(&id.to_string()).await {
                tracing::error!(
                    peer_id = %id,
                    error = %cleanup,
                    "inbound-wg: failed to roll back peer row after interface add_peer failure",
                );
            }
            return Err(AppError::Internal(error));
        }

        Ok(AddInboundWgPeerResponse {
            id,
            name,
            public_key: public_b64,
            private_key: private_b64,
            allowed_ip,
        })
    }

    async fn remove_peer(&self, id: Uuid) -> Result<(), AppError> {
        auth_context::require_admin()?;

        let row = self
            .peers
            .find_by_id(&id.to_string())
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound(format!("inbound-wg peer {id} not found")))?;

        let public_key = Self::decode_key(&row.public_key)?;
        // Best-effort removal from the live interface; the DB row is the source
        // of truth, so a failed kernel removal is logged, not fatal.
        if let Err(e) = self
            .interface
            .remove_peer(INBOUND_WG_INTERFACE, public_key)
            .await
        {
            tracing::warn!(peer_id = %id, error = %e, "inbound-wg: failed to remove peer from interface, deleting row anyway");
        }

        self.peers
            .delete(&id.to_string())
            .await
            .map_err(AppError::Internal)?;
        Ok(())
    }

    async fn list_peers(&self) -> Result<Vec<InboundWgPeerSummary>, AppError> {
        auth_context::require_admin()?;

        let rows = self.peers.find_all().await.map_err(AppError::Internal)?;
        rows.into_iter().map(row_to_summary).collect()
    }

    async fn reconcile(&self) -> Result<(), AppError> {
        // Startup/restore method — runs before the system is ready, so it is
        // exempt from `require_admin` per `.agents/auth.md` rule 2.
        if !self
            .system_config
            .inbound_wg_enabled()
            .await
            .map_err(AppError::Internal)?
        {
            return Ok(());
        }

        let listen_port = self
            .system_config
            .inbound_wg_listen_port()
            .await
            .map_err(AppError::Internal)?;
        let (private_key, _pubkey) = self.ensure_server_keypair().await?;
        self.bring_up_server(private_key, listen_port).await?;

        self.firewall
            .add_masquerade(INBOUND_WG_INTERFACE)
            .await
            .map_err(AppError::Internal)?;
        self.firewall
            .add_inbound_wg_accept(listen_port)
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(
            listen_port,
            "inbound wireguard server reconciled on startup"
        );
        Ok(())
    }
}

/// Convert a stored peer row into an API summary (no private key).
fn row_to_summary(row: InboundWgPeerRow) -> Result<InboundWgPeerSummary, AppError> {
    let id = row
        .id
        .parse::<Uuid>()
        .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid stored peer id: {e}")))?;
    let created_at = chrono::DateTime::parse_from_rfc3339(&row.created_at)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid stored created_at: {e}")))?
        .with_timezone(&chrono::Utc);
    Ok(InboundWgPeerSummary {
        id,
        name: row.name,
        public_key: row.public_key,
        allowed_ip: row.allowed_ip,
        enabled: row.enabled,
        created_at,
    })
}
