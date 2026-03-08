use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine;
use uuid::Uuid;
use wardnet_types::api::{
    CreateTunnelRequest, CreateTunnelResponse, DeleteTunnelResponse, ListTunnelsResponse,
};
use wardnet_types::event::WardnetEvent;
use wardnet_types::tunnel::{Tunnel, TunnelStatus};
use wardnet_types::wireguard_config;

use crate::error::AppError;
use crate::event::EventPublisher;
use crate::keys::KeyStore;
use crate::repository::TunnelRepository;
use crate::repository::tunnel::TunnelRow;
use crate::wireguard::{CreateInterfaceParams, WireGuardOps};

/// Tunnel lifecycle management.
///
/// Orchestrates importing, bringing up, tearing down, and deleting
/// `WireGuard` tunnels. Coordinates between the repository (persistence),
/// key store (private keys on disk), `WireGuard` ops (kernel interface),
/// and event publisher (domain events).
#[async_trait]
pub trait TunnelService: Send + Sync {
    /// Import a tunnel from a `WireGuard` `.conf` file. Tunnel starts `Down`.
    async fn import_tunnel(
        &self,
        req: CreateTunnelRequest,
    ) -> Result<CreateTunnelResponse, AppError>;

    /// List all configured tunnels.
    async fn list_tunnels(&self) -> Result<ListTunnelsResponse, AppError>;

    /// Get a single tunnel by ID.
    async fn get_tunnel(&self, id: Uuid) -> Result<Tunnel, AppError>;

    /// Bring a tunnel interface up.
    async fn bring_up(&self, id: Uuid) -> Result<(), AppError>;

    /// Tear down a tunnel interface.
    async fn tear_down(&self, id: Uuid, reason: &str) -> Result<(), AppError>;

    /// Delete a tunnel entirely (removes config, key, and interface).
    async fn delete_tunnel(&self, id: Uuid) -> Result<DeleteTunnelResponse, AppError>;

    /// Restore tunnel configs from the database on startup (does NOT bring interfaces up).
    async fn restore_tunnels(&self) -> Result<(), AppError>;
}

/// Default implementation of [`TunnelService`].
pub struct TunnelServiceImpl {
    tunnels: Arc<dyn TunnelRepository>,
    wireguard: Arc<dyn WireGuardOps>,
    keys: Arc<dyn KeyStore>,
    events: Arc<dyn EventPublisher>,
}

impl TunnelServiceImpl {
    /// Create a new tunnel service with the given dependencies.
    pub fn new(
        tunnels: Arc<dyn TunnelRepository>,
        wireguard: Arc<dyn WireGuardOps>,
        keys: Arc<dyn KeyStore>,
        events: Arc<dyn EventPublisher>,
    ) -> Self {
        Self {
            tunnels,
            wireguard,
            keys,
            events,
        }
    }

    /// Look up a tunnel by ID, returning `AppError::NotFound` when absent.
    async fn require_tunnel(&self, id: Uuid) -> Result<Tunnel, AppError> {
        self.tunnels
            .find_by_id(&id.to_string())
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound(format!("tunnel {id} not found")))
    }

    /// Decode a base64-encoded `WireGuard` key into a 32-byte array.
    fn decode_key(b64: &str) -> Result<[u8; 32], AppError> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid base64 key: {e}")))?;
        bytes
            .try_into()
            .map_err(|_| AppError::Internal(anyhow::anyhow!("WireGuard key must be 32 bytes")))
    }
}

#[async_trait]
impl TunnelService for TunnelServiceImpl {
    async fn import_tunnel(
        &self,
        req: CreateTunnelRequest,
    ) -> Result<CreateTunnelResponse, AppError> {
        // Parse the `WireGuard` .conf content.
        let config = wireguard_config::parse(&req.config)
            .map_err(|e| AppError::BadRequest(e.to_string()))?;

        let peer = config
            .peers
            .first()
            .ok_or_else(|| AppError::BadRequest("config has no peers".to_owned()))?;

        // Determine interface name.
        let idx = self
            .tunnels
            .next_interface_index()
            .await
            .map_err(AppError::Internal)?;
        let interface_name = format!("wg_ward{idx}");

        // Generate tunnel ID.
        let id = Uuid::new_v4();

        // Save private key to key store.
        self.keys
            .save_key(&id, &config.interface.private_key)
            .await
            .map_err(AppError::Internal)?;

        // Extract endpoint from the first peer.
        let endpoint = peer.endpoint.clone().unwrap_or_default();

        // Serialize sub-structures as JSON for storage.
        let address_json = serde_json::to_string(&config.interface.address)
            .map_err(|e| AppError::Internal(e.into()))?;
        let dns_json = serde_json::to_string(&config.interface.dns)
            .map_err(|e| AppError::Internal(e.into()))?;
        let peer_config_json =
            serde_json::to_string(peer).map_err(|e| AppError::Internal(e.into()))?;

        let row = TunnelRow {
            id: id.to_string(),
            label: req.label.clone(),
            country_code: req.country_code.clone(),
            provider: req.provider.clone(),
            interface_name: interface_name.clone(),
            endpoint: endpoint.clone(),
            status: "down".to_owned(),
            address: address_json,
            dns: dns_json,
            peer_config: peer_config_json,
            listen_port: config.interface.listen_port,
        };

        self.tunnels
            .insert(&row)
            .await
            .map_err(AppError::Internal)?;

        let now = chrono::Utc::now();
        let tunnel = Tunnel {
            id,
            label: req.label,
            country_code: req.country_code,
            provider: req.provider,
            interface_name,
            endpoint,
            status: TunnelStatus::Down,
            last_handshake: None,
            bytes_tx: 0,
            bytes_rx: 0,
            created_at: now,
        };

        Ok(CreateTunnelResponse {
            tunnel,
            message: "tunnel imported successfully".to_owned(),
        })
    }

    async fn list_tunnels(&self) -> Result<ListTunnelsResponse, AppError> {
        let tunnels = self.tunnels.find_all().await.map_err(AppError::Internal)?;
        Ok(ListTunnelsResponse { tunnels })
    }

    async fn get_tunnel(&self, id: Uuid) -> Result<Tunnel, AppError> {
        self.require_tunnel(id).await
    }

    async fn bring_up(&self, id: Uuid) -> Result<(), AppError> {
        let tunnel = self.require_tunnel(id).await?;

        // No-op if already up.
        if tunnel.status == TunnelStatus::Up {
            return Ok(());
        }

        // Load stored `WireGuard` configuration.
        let tunnel_config = self
            .tunnels
            .find_config_by_id(&id.to_string())
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound(format!("tunnel config {id} not found")))?;

        // Load and decode private key from key store.
        let private_key_b64 = self.keys.load_key(&id).await.map_err(AppError::Internal)?;
        let private_key = Self::decode_key(&private_key_b64)?;

        // Decode peer public key.
        let peer_public_key = Self::decode_key(&tunnel_config.peer.public_key)?;

        // Decode optional preshared key.
        let peer_preshared_key = tunnel_config
            .peer
            .preshared_key
            .as_deref()
            .map(Self::decode_key)
            .transpose()?;

        // Parse peer endpoint.
        let peer_endpoint = tunnel_config
            .peer
            .endpoint
            .as_deref()
            .map(str::parse)
            .transpose()
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid peer endpoint: {e}")))?;

        // Parse allowed IPs.
        let peer_allowed_ips = tunnel_config
            .peer
            .allowed_ips
            .iter()
            .map(|ip| ip.parse())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid allowed IP: {e}")))?;

        let params = CreateInterfaceParams {
            interface_name: tunnel.interface_name.clone(),
            private_key,
            listen_port: tunnel_config.listen_port,
            peer_public_key,
            peer_endpoint,
            peer_allowed_ips,
            peer_preshared_key,
            persistent_keepalive: tunnel_config.peer.persistent_keepalive,
        };

        // Create the `WireGuard` interface and bring it up.
        self.wireguard
            .create_interface(params)
            .await
            .map_err(AppError::Internal)?;
        self.wireguard
            .bring_up(&tunnel.interface_name)
            .await
            .map_err(AppError::Internal)?;

        // Update status in the database.
        self.tunnels
            .update_status(&id.to_string(), "up")
            .await
            .map_err(AppError::Internal)?;

        // Publish domain event.
        self.events.publish(WardnetEvent::TunnelUp {
            tunnel_id: id,
            interface_name: tunnel.interface_name,
            endpoint: tunnel.endpoint,
            timestamp: chrono::Utc::now(),
        });

        Ok(())
    }

    async fn tear_down(&self, id: Uuid, reason: &str) -> Result<(), AppError> {
        let tunnel = self.require_tunnel(id).await?;

        // No-op if already down.
        if tunnel.status == TunnelStatus::Down {
            return Ok(());
        }

        // Tear down and remove the `WireGuard` interface.
        self.wireguard
            .tear_down(&tunnel.interface_name)
            .await
            .map_err(AppError::Internal)?;
        self.wireguard
            .remove_interface(&tunnel.interface_name)
            .await
            .map_err(AppError::Internal)?;

        // Update status in the database.
        self.tunnels
            .update_status(&id.to_string(), "down")
            .await
            .map_err(AppError::Internal)?;

        // Publish domain event.
        self.events.publish(WardnetEvent::TunnelDown {
            tunnel_id: id,
            interface_name: tunnel.interface_name,
            reason: reason.to_owned(),
            timestamp: chrono::Utc::now(),
        });

        Ok(())
    }

    async fn delete_tunnel(&self, id: Uuid) -> Result<DeleteTunnelResponse, AppError> {
        let tunnel = self.require_tunnel(id).await?;

        // If the tunnel is up, tear it down first.
        if tunnel.status == TunnelStatus::Up {
            self.tear_down(id, "tunnel deleted").await?;
        }

        // Delete private key from key store.
        self.keys
            .delete_key(&id)
            .await
            .map_err(AppError::Internal)?;

        // Delete from database.
        self.tunnels
            .delete(&id.to_string())
            .await
            .map_err(AppError::Internal)?;

        Ok(DeleteTunnelResponse {
            message: format!("tunnel {} deleted", tunnel.label),
        })
    }

    async fn restore_tunnels(&self) -> Result<(), AppError> {
        let tunnels = self.tunnels.find_all().await.map_err(AppError::Internal)?;

        tracing::info!(
            count = tunnels.len(),
            "restored tunnel configurations from database"
        );
        Ok(())
    }
}
