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
use crate::device::service::DeviceService;
use crate::entitlement::Entitlement;
use crate::error::AppError;
use crate::inbound_wg::interface::{
    InboundWgInterface, InboundWgPeerConfig, InboundWgServerConfig,
};
use crate::inbound_wg::key_store::{ServerKeyStore, ServerKeyStoreAdapter};
use crate::inbound_wg::keygen::generate_keypair;
use crate::routing::FirewallManager;
use wardnetd_data::repository::SystemConfigRepository;
use wardnetd_data::repository::inbound_wg::{
    DeviceAlreadyGrantedError, InboundWgPeerRepository, InboundWgPeerRow,
};
use wardnetd_data::secret_store::SecretStore;

/// One enabled inbound peer paired with its linked device, for the inbound
/// `WireGuard` monitor's handshake-polling loop (issue #810).
///
/// Deliberately device-id-keyed and mac-free: the monitor never needs — and
/// must never resolve — a device's MAC. `public_key` matches the raw
/// 32-byte key used by
/// [`InboundWgPeerStats`](crate::inbound_wg::interface::InboundWgPeerStats) so
/// the two can be joined; `allowed_ip` is the peer's bare tunnel IP (no `/32`
/// suffix) to feed straight into the discovery observation path.
#[derive(Debug, Clone)]
pub struct InboundWgMonitorPeer {
    /// The `Device` this peer grants remote access to.
    pub device_id: Uuid,
    /// The peer's raw 32-byte `WireGuard` public key.
    pub public_key: [u8; 32],
    /// The peer's bare IP on the inbound subnet (e.g. `10.100.64.2`).
    pub allowed_ip: String,
}

/// Fixed name of the inbound `WireGuard` server interface.
///
/// The `wg_wardin0` name deliberately shares the `wg_ward` prefix that the
/// firewall's zone-egress gate matches (`TUNNEL_IFACE_PREFIX` in
/// `wardnetd::firewall_netlink`), per issue #809.
///
/// Since #810 wired inbound peers to `Device` rows, a zone-denied remote
/// device *can* now have `ZoneRules` computed for its peer IP. The zone-egress
/// drop rule therefore explicitly **excludes** `wg_wardin0`: that interface is
/// the peer's *inbound* attachment point, not an outbound-provider-tunnel
/// egress path, so a zone-denied remote device's own return traffic must not
/// be dropped by the tunnel-egress gate. See the exclusion at
/// `TUNNEL_IFACE_PREFIX` / `inbound_wg_iface_exact_value` in
/// `wardnetd::firewall_netlink`.
pub const INBOUND_WG_INTERFACE: &str = "wg_wardin0";

/// Inbound tunnel subnet. The server owns `.1`; peers are allocated `/32`s
/// sequentially from `.2` upward.
const SUBNET_PREFIX: [u8; 3] = [10, 100, 64];
/// Prefix length of the inbound tunnel subnet (`10.100.64.0/24`).
const SUBNET_MASK: u8 = 24;
/// Last octet reserved for the server itself.
const SERVER_HOST: u8 = 1;

/// Inbound (multi-peer) `WireGuard` server management (issues #809, #810).
///
/// Orchestrates the server interface, its singleton keypair, the peer data
/// model, IP allocation from the inbound subnet, and the firewall
/// masquerade/accept rules. Each peer is a remote-access grant on an
/// already-managed [`Device`](wardnet_common::device::Device) (one credential
/// per device); a live inbound handshake flips that device's
/// [`connection_mode`](wardnet_common::device::DeviceConnectionMode) to
/// `Remote` via the discovery service, driven by the inbound-WireGuard monitor.
#[async_trait]
pub trait InboundWgService: Send + Sync {
    /// Read the current server config (enabled, listen port, public key)
    /// without mutating anything — for UI surfaces that need to show live
    /// state on load rather than only after a `set_config` call.
    async fn get_config(&self) -> Result<InboundWgConfigResponse, AppError>;

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

    /// Grant remote access to an already-managed device: generate a keypair,
    /// allocate an IP, persist the row (public key + `device_id`), add it to
    /// the interface, and return the **full client config** (with the private
    /// key) exactly once. The peer's user-facing name is taken from the device
    /// itself. `endpoint` is the reachable `host:port` the client dials, which
    /// the caller derives (DDNS today, cloud relay per #824) — `None` yields a
    /// response with no `client_config`. Rejected when the server is disabled,
    /// the device does not exist or is unmanaged, or it already has a
    /// credential (one per device).
    async fn add_peer(
        &self,
        device_id: Uuid,
        endpoint: Option<String>,
    ) -> Result<AddInboundWgPeerResponse, AppError>;

    /// Remove a peer by id from both the interface and the database.
    async fn remove_peer(&self, id: Uuid) -> Result<(), AppError>;

    /// Pause or resume a peer without deleting its credential: re-admit it
    /// onto the live interface (enable) or best-effort remove it (disable),
    /// then persist the flag. Distinct from `remove_peer` — the keypair and
    /// allocated IP survive, so re-enabling never needs a fresh QR scan.
    /// No-op (returns the current summary) if the peer is already in the
    /// requested state. 404 if the peer does not exist.
    async fn set_peer_enabled(
        &self,
        id: Uuid,
        enabled: bool,
    ) -> Result<InboundWgPeerSummary, AppError>;

    /// List every peer (no private keys).
    async fn list_peers(&self) -> Result<Vec<InboundWgPeerSummary>, AppError>;

    /// Every enabled peer with its linked device and tunnel IP, for the
    /// inbound `WireGuard` monitor's handshake-polling loop. Internal use — no
    /// auth check. Peers with no `device_id` (impossible from #810 onward) or
    /// an unparseable key/IP are skipped, not fatal.
    async fn list_peers_for_monitor(&self) -> Result<Vec<InboundWgMonitorPeer>, AppError>;

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
    /// Resolves the target device for a remote-access grant. Goes through
    /// [`DeviceService`] (never `DeviceRepository` directly) so the
    /// single-service-per-repository rule holds.
    devices: Arc<dyn DeviceService>,
    /// Shared entitlement handle. Personal VPN is a Premium capability, so
    /// enabling the server and granting peers require an active entitlement,
    /// and a box that has lost entitlement disables the server on reconcile.
    entitlement: Arc<Entitlement>,
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
        devices: Arc<dyn DeviceService>,
        entitlement: Arc<Entitlement>,
    ) -> Self {
        let keys: Arc<dyn ServerKeyStore> = Arc::new(ServerKeyStoreAdapter::new(secret_store));
        Self {
            peers,
            system_config,
            keys,
            interface,
            firewall,
            devices,
            entitlement,
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
        devices: Arc<dyn DeviceService>,
        entitlement: Arc<Entitlement>,
    ) -> Self {
        Self {
            peers,
            system_config,
            keys,
            interface,
            firewall,
            devices,
            entitlement,
        }
    }

    /// Premium gate for the Personal VPN feature. Enabling the inbound server
    /// and granting peers require an active entitlement, mirroring the 403 the
    /// serving layer returns for the premium PWA surfaces.
    fn require_entitled(&self) -> Result<(), AppError> {
        if self.entitlement.is_entitled() {
            Ok(())
        } else {
            Err(AppError::Forbidden(
                "inbound WireGuard (Personal VPN) requires an active Premium subscription"
                    .to_owned(),
            ))
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

    /// Assemble the full `WireGuard` client `.conf` for a freshly-granted peer.
    /// It embeds the peer's private key, so it is only ever produced here (for
    /// the one-time `add_peer` response) and never persisted. The client's
    /// `DNS` is the inbound server's own address (`10.100.64.1`) so filtering
    /// still applies, and the tunnel is full (`AllowedIPs = 0.0.0.0/0, ::/0`)
    /// so a remote device routes everything through the home gateway.
    /// `allowed_ip` already carries its `/32`.
    fn build_client_config(
        private_key_b64: &str,
        allowed_ip: &str,
        server_pubkey_b64: &str,
        endpoint: &str,
    ) -> String {
        let dns = format!(
            "{}.{}.{}.{SERVER_HOST}",
            SUBNET_PREFIX[0], SUBNET_PREFIX[1], SUBNET_PREFIX[2]
        );
        format!(
            "[Interface]\n\
             PrivateKey = {private_key_b64}\n\
             Address = {allowed_ip}\n\
             DNS = {dns}\n\
             \n\
             [Peer]\n\
             PublicKey = {server_pubkey_b64}\n\
             Endpoint = {endpoint}\n\
             AllowedIPs = 0.0.0.0/0, ::/0\n\
             PersistentKeepalive = 25\n"
        )
    }

    /// Load the persisted server private key, or generate + persist a fresh
    /// keypair if none exists. Returns the raw private key and the base64
    /// public key, and (idempotently) caches the public key in `system_config`.
    async fn ensure_server_keypair(&self) -> Result<([u8; 32], String), AppError> {
        let (private, pub_b64) =
            if let Some(priv_b64) = self.keys.load_key().await.map_err(AppError::Internal)? {
                let private = Self::decode_key(&priv_b64)?;
                let public = x25519_dalek::x25519(private, x25519_dalek::X25519_BASEPOINT_BYTES);
                (private, Self::encode_key(&public))
            } else {
                let (private, public) = generate_keypair();
                let priv_b64 = Self::encode_key(&private);
                self.keys
                    .save_key(&priv_b64)
                    .await
                    .map_err(AppError::Internal)?;
                (private, Self::encode_key(&public))
            };
        // Persist the public key on EVERY call, not just first keygen: the
        // `system_config` copy is a cache of what the key store holds, and the
        // two can desync (e.g. the DB is reset while the key-store file
        // survives). Without this, an enabled server whose key predates the
        // cache reads back with a `null` public key, and every generated client
        // config gets an empty `PublicKey =` line (WireGuard "syntax error").
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
            "inbound WireGuard subnet is full - no free address".to_owned(),
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

    /// Best-effort removal of a peer from the live `wg_wardin0` interface. The
    /// DB row is the source of truth, so a kernel-side removal failure is
    /// logged and swallowed. A malformed stored public key IS fatal (it
    /// signals corruption the caller should surface). Shared by revoke
    /// ([`Self::remove_peer`]) and pause ([`Self::set_peer_enabled`]) so the
    /// eviction path stays identical.
    async fn remove_peer_from_interface(&self, row: &InboundWgPeerRow) -> Result<(), AppError> {
        let public_key = Self::decode_key(&row.public_key)?;
        if let Err(e) = self
            .interface
            .remove_peer(INBOUND_WG_INTERFACE, public_key)
            .await
        {
            tracing::warn!(
                peer_id = %row.id,
                error = %e,
                "inbound-wg: failed to remove peer {} from interface, continuing: {e}",
                row.id,
            );
        }
        Ok(())
    }

    /// Reset the peer's device back off `Remote` `connection_mode`. A peer
    /// with no live path (revoked or paused) must not leave its device stuck
    /// `Remote` with nothing to correct it. Best-effort — the primary state
    /// change is already persisted, so a failure here is logged, not fatal.
    /// Shared by revoke ([`Self::remove_peer`]) and pause
    /// ([`Self::set_peer_enabled`]).
    async fn reset_peer_connection_mode(&self, row: &InboundWgPeerRow) {
        if let Some(device_id) = &row.device_id
            && let Err(e) = self.devices.clear_remote_connection_mode(device_id).await
        {
            tracing::warn!(
                peer_id = %row.id,
                device_id = %device_id,
                error = %e,
                "inbound-wg: failed to reset device connection_mode for peer {}: {e}",
                row.id,
            );
        }
    }
}

#[async_trait]
impl InboundWgService for InboundWgServiceImpl {
    async fn get_config(&self) -> Result<InboundWgConfigResponse, AppError> {
        auth_context::require_admin()?;

        let enabled = self
            .system_config
            .inbound_wg_enabled()
            .await
            .map_err(AppError::Internal)?;
        let listen_port = self
            .system_config
            .inbound_wg_listen_port()
            .await
            .map_err(AppError::Internal)?;
        let server_public_key = self
            .system_config
            .inbound_wg_server_pubkey()
            .await
            .map_err(AppError::Internal)?;

        Ok(InboundWgConfigResponse {
            enabled,
            listen_port,
            server_public_key,
        })
    }

    async fn set_config(
        &self,
        enabled: bool,
        listen_port: u16,
    ) -> Result<InboundWgConfigResponse, AppError> {
        auth_context::require_admin()?;

        if enabled {
            // Disabling is always allowed (an unentitled box must be able to
            // turn the server off); only enabling is Premium-gated.
            self.require_entitled()?;

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

    async fn add_peer(
        &self,
        device_id: Uuid,
        endpoint: Option<String>,
    ) -> Result<AddInboundWgPeerResponse, AppError> {
        auth_context::require_admin()?;
        self.require_entitled()?;

        // Precondition first: no keygen / DB work if the server is off.
        if !self
            .system_config
            .inbound_wg_enabled()
            .await
            .map_err(AppError::Internal)?
        {
            return Err(AppError::Conflict(
                "inbound WireGuard server is disabled - enable it before adding peers".to_owned(),
            ));
        }

        // A remote-access grant targets an already-managed device. Resolve it
        // via `DeviceService` (never the repository directly).
        let device_id_str = device_id.to_string();
        let device = self
            .devices
            .get_device(&device_id_str)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("device {device_id} not found")))?;

        // One credential per device. The DB `UNIQUE` constraint also enforces
        // this, but a clean conflict is friendlier than a raw violation.
        if self
            .peers
            .find_by_device_id(&device_id_str)
            .await
            .map_err(AppError::Internal)?
            .is_some()
        {
            return Err(AppError::Conflict(format!(
                "device {device_id} already has an inbound WireGuard credential"
            )));
        }

        // Only a *managed* device — one an admin has named — can be granted
        // remote access; a bare discovered device must be adopted (named)
        // first. The admin UI filters unmanaged devices out of the picker, so
        // this is defense-in-depth. The peer's user-facing label is that
        // admin-set name (never a free-text param).
        let Some(name) = device.name.clone() else {
            return Err(AppError::Conflict(format!(
                "device {device_id} is unmanaged - name (adopt) it before granting remote access"
            )));
        };

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
            device_id: Some(device_id_str),
        };
        // The service-layer `find_by_device_id` check above is not
        // transactional, so two concurrent grants for the same device can both
        // pass it; the loser's insert trips the `device_id` UNIQUE index. The
        // repository flags that specific collision as a
        // [`DeviceAlreadyGrantedError`] so we can return a clean 409 instead of
        // a raw 500 (other unique violations still surface as internal errors).
        if let Err(e) = self.peers.insert(&row).await {
            if e.downcast_ref::<DeviceAlreadyGrantedError>().is_some() {
                return Err(AppError::Conflict(
                    "device already has a remote-access credential".to_owned(),
                ));
            }
            return Err(AppError::Internal(e));
        }

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

        // Assemble the full client config server-side (the private key never
        // leaves this method's memory otherwise). The server public key is the
        // one persisted on enable; if it is somehow absent, or no endpoint is
        // known, there is no usable config to return.
        let server_pubkey = self
            .system_config
            .inbound_wg_server_pubkey()
            .await
            .map_err(AppError::Internal)?;
        let client_config = match (server_pubkey, endpoint) {
            (Some(pubkey), Some(endpoint)) => Some(Self::build_client_config(
                &private_b64,
                &allowed_ip,
                &pubkey,
                &endpoint,
            )),
            _ => None,
        };

        Ok(AddInboundWgPeerResponse {
            id,
            name,
            public_key: public_b64,
            allowed_ip,
            client_config,
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

        self.remove_peer_from_interface(&row).await?;

        self.peers
            .delete(&id.to_string())
            .await
            .map_err(AppError::Internal)?;

        // The revoked credential was this device's only remote-access path, so
        // clear its (monitor-set) `Remote` connection_mode now that nothing is
        // left to correct it.
        self.reset_peer_connection_mode(&row).await;
        Ok(())
    }

    async fn set_peer_enabled(
        &self,
        id: Uuid,
        enabled: bool,
    ) -> Result<InboundWgPeerSummary, AppError> {
        auth_context::require_admin()?;

        let row = self
            .peers
            .find_by_id(&id.to_string())
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound(format!("inbound-wg peer {id} not found")))?;

        if row.enabled == enabled {
            return row_to_summary(row);
        }

        // Only touch the live interface when the server itself is up —
        // otherwise nothing is admitted to add/remove, and `bring_up_server`'s
        // `find_enabled()` re-admit sweep picks up the persisted flag on the
        // next enable.
        let server_up = self
            .system_config
            .inbound_wg_enabled()
            .await
            .map_err(AppError::Internal)?;

        if server_up {
            if enabled {
                let config = Self::peer_row_to_config(&row)?;
                self.interface
                    .add_peer(INBOUND_WG_INTERFACE, config)
                    .await
                    .map_err(AppError::Internal)?;
            } else {
                self.remove_peer_from_interface(&row).await?;
            }
        }

        self.peers
            .set_enabled(&id.to_string(), enabled)
            .await
            .map_err(AppError::Internal)?;

        // A disabled peer has no live path, so clear its device's (monitor-set)
        // `Remote` connection_mode — same teardown revoke applies.
        if !enabled {
            self.reset_peer_connection_mode(&row).await;
        }

        let mut updated = row;
        updated.enabled = enabled;
        row_to_summary(updated)
    }

    async fn list_peers(&self) -> Result<Vec<InboundWgPeerSummary>, AppError> {
        auth_context::require_admin()?;

        let rows = self.peers.find_all().await.map_err(AppError::Internal)?;
        rows.into_iter().map(row_to_summary).collect()
    }

    async fn list_peers_for_monitor(&self) -> Result<Vec<InboundWgMonitorPeer>, AppError> {
        // Internal use by the inbound-WireGuard monitor — no auth check.
        let rows = self
            .peers
            .find_enabled()
            .await
            .map_err(AppError::Internal)?;
        let mut peers = Vec::with_capacity(rows.len());
        for row in rows {
            let Some(device_id_str) = row.device_id.as_deref() else {
                // Pre-#810 rows had no device link; nothing for the monitor to
                // observe against, so skip rather than fail the whole poll.
                tracing::warn!(peer_id = %row.id, "inbound-wg monitor: peer {} has no device_id, skipping", row.id);
                continue;
            };
            let device_id = match device_id_str.parse::<Uuid>() {
                Ok(id) => id,
                Err(e) => {
                    tracing::warn!(peer_id = %row.id, error = %e, "inbound-wg monitor: unparseable device_id for peer {}, skipping: {e}", row.id);
                    continue;
                }
            };
            let public_key = match Self::decode_key(&row.public_key) {
                Ok(k) => k,
                Err(e) => {
                    tracing::warn!(peer_id = %row.id, error = %e, "inbound-wg monitor: unparseable public key for peer {}, skipping: {e}", row.id);
                    continue;
                }
            };
            // Strip the `/32` suffix — discovery wants the bare IP.
            let allowed_ip = row
                .allowed_ip
                .split('/')
                .next()
                .unwrap_or(&row.allowed_ip)
                .to_owned();
            peers.push(InboundWgMonitorPeer {
                device_id,
                public_key,
                allowed_ip,
            });
        }
        Ok(peers)
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

        // Personal VPN is Premium. If the server was enabled while entitled but
        // the box has since lost entitlement (subscription lapsed, or moved off
        // the wardnet provider), do not stand it back up: disable it and persist
        // that, so the daemon never serves a Premium feature the box no longer
        // has. Interface/firewall teardown is best-effort (nothing is up yet on
        // a fresh boot; this only matters if a prior process left state behind).
        if !self.entitlement.is_entitled() {
            tracing::warn!(
                "inbound wireguard server was enabled but the box is no longer entitled to \
                 Premium; disabling it on reconcile"
            );
            self.system_config
                .set_inbound_wg_enabled(false)
                .await
                .map_err(AppError::Internal)?;
            let _ = self.interface.tear_down_server(INBOUND_WG_INTERFACE).await;
            let _ = self.firewall.remove_masquerade(INBOUND_WG_INTERFACE).await;
            let _ = self.firewall.remove_inbound_wg_accept().await;
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
    // Unparseable is treated the same as absent (logged, not fatal) — a
    // single bad row must not break the whole listing.
    let device_id = row.device_id.as_deref().and_then(|s| match s.parse() {
        Ok(id) => Some(id),
        Err(e) => {
            tracing::warn!(peer_id = %row.id, error = %e, "inbound-wg: unparseable device_id for peer {}", row.id);
            None
        }
    });
    Ok(InboundWgPeerSummary {
        id,
        name: row.name,
        public_key: row.public_key,
        allowed_ip: row.allowed_ip,
        enabled: row.enabled,
        created_at,
        device_id,
    })
}
