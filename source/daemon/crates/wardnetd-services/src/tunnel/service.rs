use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine;
use uuid::Uuid;
use wardnet_common::api::{
    CreateTunnelRequest, CreateTunnelResponse, DeleteTunnelResponse, ListTunnelsResponse,
};
use wardnet_common::event::WardnetEvent;
use wardnet_common::tunnel::{Tunnel, TunnelStatus};
use wardnet_common::wireguard_config;

use crate::auth_context;
use crate::error::AppError;
use crate::event::EventPublisher;
use crate::tunnel::interface::{
    CreateTunnelParams, TunnelConfig as TiTunnelConfig, TunnelInterface,
};
use wardnetd_data::keys::KeyStore;
use wardnetd_data::repository::TunnelRepository;
use wardnetd_data::repository::tunnel::TunnelRow;

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

    /// Bring a tunnel interface up without requiring admin authentication.
    ///
    /// Used internally by the routing engine when a device's routing rule
    /// targets a tunnel that is currently down.
    async fn bring_up_internal(&self, id: Uuid) -> Result<(), AppError>;

    /// Tear down a tunnel interface without requiring admin authentication.
    ///
    /// Used internally by the idle tunnel watcher and routing engine for
    /// automated lifecycle management.
    async fn tear_down_internal(&self, id: Uuid, reason: &str) -> Result<(), AppError>;

    /// Restore tunnel configs from the database on startup (does NOT bring interfaces up).
    async fn restore_tunnels(&self) -> Result<(), AppError>;

    /// Collect stats for all Up tunnels, update the database, and publish events.
    ///
    /// Used by the tunnel monitor background task. No auth guard — called from
    /// background task context.
    async fn collect_stats(&self) -> Result<(), AppError>;

    /// Run health checks on all Up tunnels, logging warnings for stale handshakes.
    ///
    /// Used by the tunnel monitor background task. No auth guard — called from
    /// background task context.
    async fn run_health_check(&self) -> Result<(), AppError>;
}

/// Default implementation of [`TunnelService`].
pub struct TunnelServiceImpl {
    tunnels: Arc<dyn TunnelRepository>,
    devices: Arc<dyn wardnetd_data::repository::DeviceRepository>,
    tunnel_interface: Arc<dyn TunnelInterface>,
    keys: Arc<dyn KeyStore>,
    events: Arc<dyn EventPublisher>,
}

impl TunnelServiceImpl {
    /// Create a new tunnel service with the given dependencies.
    pub fn new(
        tunnels: Arc<dyn TunnelRepository>,
        devices: Arc<dyn wardnetd_data::repository::DeviceRepository>,
        tunnel_interface: Arc<dyn TunnelInterface>,
        keys: Arc<dyn KeyStore>,
        events: Arc<dyn EventPublisher>,
    ) -> Self {
        Self {
            tunnels,
            devices,
            tunnel_interface,
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

    /// Core logic for bringing a tunnel up (no auth check).
    async fn bring_up_core(&self, id: Uuid) -> Result<(), AppError> {
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

        // Parse peer endpoint — resolve hostname if needed (e.g. NordVPN gives
        // `pt149.nordvpn.com:51820` which must be resolved to an IP for WireGuard).
        let peer_endpoint = match tunnel_config.peer.endpoint.as_deref() {
            None => None,
            Some(ep) => {
                // Try direct parse first (already an IP:port).
                if let Ok(addr) = ep.parse::<std::net::SocketAddr>() {
                    Some(addr)
                } else {
                    // Resolve hostname via DNS.
                    let addr = tokio::net::lookup_host(ep)
                        .await
                        .map_err(|e| {
                            AppError::Internal(anyhow::anyhow!(
                                "failed to resolve peer endpoint '{ep}': {e}"
                            ))
                        })?
                        .next()
                        .ok_or_else(|| {
                            AppError::Internal(anyhow::anyhow!(
                                "DNS resolution returned no addresses for '{ep}'"
                            ))
                        })?;
                    Some(addr)
                }
            }
        };

        // Parse allowed IPs.
        let peer_allowed_ips = tunnel_config
            .peer
            .allowed_ips
            .iter()
            .map(|ip| ip.parse())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid allowed IP: {e}")))?;

        // Parse interface addresses (e.g. `10.66.0.2/32`).
        let interface_addresses = tunnel_config
            .address
            .iter()
            .map(|a| a.parse())
            .collect::<Result<Vec<ipnetwork::IpNetwork>, _>>()
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid interface address: {e}")))?;

        let params = CreateTunnelParams {
            interface_name: tunnel.interface_name.clone(),
            config: TiTunnelConfig::WireGuard {
                address: interface_addresses,
                private_key,
                listen_port: tunnel_config.listen_port,
                peer_public_key,
                peer_endpoint,
                peer_allowed_ips,
                peer_preshared_key,
                persistent_keepalive: tunnel_config.peer.persistent_keepalive,
            },
        };

        // Create the tunnel interface and bring it up.
        self.tunnel_interface
            .create(params)
            .await
            .map_err(AppError::Internal)?;
        self.tunnel_interface
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

    /// Core logic for tearing down a tunnel (no auth check).
    async fn tear_down_core(&self, id: Uuid, reason: &str) -> Result<(), AppError> {
        let tunnel = self.require_tunnel(id).await?;

        // No-op if already down.
        if tunnel.status == TunnelStatus::Down {
            return Ok(());
        }

        // Tear down and remove the tunnel interface.
        self.tunnel_interface
            .tear_down(&tunnel.interface_name)
            .await
            .map_err(AppError::Internal)?;
        self.tunnel_interface
            .remove(&tunnel.interface_name)
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
}

#[async_trait]
impl TunnelService for TunnelServiceImpl {
    async fn import_tunnel(
        &self,
        req: CreateTunnelRequest,
    ) -> Result<CreateTunnelResponse, AppError> {
        auth_context::require_admin()?;

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
        auth_context::require_authenticated()?;

        let tunnels = self.tunnels.find_all().await.map_err(AppError::Internal)?;
        Ok(ListTunnelsResponse { tunnels })
    }

    async fn get_tunnel(&self, id: Uuid) -> Result<Tunnel, AppError> {
        auth_context::require_authenticated()?;

        self.require_tunnel(id).await
    }

    async fn bring_up(&self, id: Uuid) -> Result<(), AppError> {
        auth_context::require_admin()?;
        self.bring_up_core(id).await
    }

    async fn tear_down(&self, id: Uuid, reason: &str) -> Result<(), AppError> {
        auth_context::require_admin()?;
        self.tear_down_core(id, reason).await
    }

    async fn bring_up_internal(&self, id: Uuid) -> Result<(), AppError> {
        self.bring_up_core(id).await
    }

    async fn tear_down_internal(&self, id: Uuid, reason: &str) -> Result<(), AppError> {
        self.tear_down_core(id, reason).await
    }

    async fn delete_tunnel(&self, id: Uuid) -> Result<DeleteTunnelResponse, AppError> {
        auth_context::require_admin()?;

        let tunnel = self.require_tunnel(id).await?;

        // Switch all routing rules targeting this tunnel to Direct so devices
        // don't lose connectivity.
        let now = chrono::Utc::now().to_rfc3339();
        let switched = self
            .devices
            .switch_tunnel_rules_to_direct(&id.to_string(), &now)
            .await
            .map_err(AppError::Internal)?;

        if !switched.is_empty() {
            tracing::info!(
                tunnel_id = %id,
                device_count = switched.len(),
                "switched devices from deleted tunnel to direct routing"
            );
            // Emit routing rule change events so the routing listener updates
            // kernel state for each affected device.
            for device_id_str in &switched {
                if let Ok(device_id) = device_id_str.parse::<Uuid>() {
                    self.events.publish(WardnetEvent::RoutingRuleChanged {
                        device_id,
                        target: wardnet_common::routing::RoutingTarget::Direct,
                        previous_target: Some(wardnet_common::routing::RoutingTarget::Tunnel {
                            tunnel_id: id,
                        }),
                        changed_by: wardnet_common::routing::RuleCreator::Admin,
                        timestamp: chrono::Utc::now(),
                    });
                }
            }
        }

        // If the tunnel is up, tear it down first.
        if tunnel.status == TunnelStatus::Up {
            self.tear_down_core(id, "tunnel deleted").await?;
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

    async fn collect_stats(&self) -> Result<(), AppError> {
        let all_tunnels = self.tunnels.find_all().await.map_err(AppError::Internal)?;
        let up_tunnels: Vec<_> = all_tunnels
            .into_iter()
            .filter(|t| t.status == wardnet_common::tunnel::TunnelStatus::Up)
            .collect();

        for tunnel in up_tunnels {
            let stats = match self
                .tunnel_interface
                .get_stats(&tunnel.interface_name)
                .await
            {
                Ok(Some(s)) => s,
                Ok(None) => continue,
                Err(e) => {
                    tracing::error!(
                        interface = %tunnel.interface_name,
                        error = %e,
                        "stats loop: failed to get stats for {}: {e}", tunnel.interface_name
                    );
                    continue;
                }
            };

            let last_handshake_str = stats.last_handshake.map(|ts| ts.to_rfc3339());

            if let Err(e) = self
                .tunnels
                .update_stats(
                    &tunnel.id.to_string(),
                    stats.bytes_tx.cast_signed(),
                    stats.bytes_rx.cast_signed(),
                    last_handshake_str.as_deref(),
                )
                .await
            {
                tracing::error!(
                    tunnel_id = %tunnel.id,
                    error = %e,
                    "stats loop: failed to update stats in database for tunnel {}: {e}", tunnel.id
                );
                continue;
            }

            self.events
                .publish(wardnet_common::event::WardnetEvent::TunnelStatsUpdated {
                    tunnel_id: tunnel.id,
                    status: wardnet_common::tunnel::TunnelStatus::Up,
                    bytes_tx: stats.bytes_tx,
                    bytes_rx: stats.bytes_rx,
                    last_handshake: stats.last_handshake,
                    timestamp: chrono::Utc::now(),
                });
        }
        Ok(())
    }

    async fn run_health_check(&self) -> Result<(), AppError> {
        let stale_threshold = chrono::Duration::minutes(3);
        let all_tunnels = self.tunnels.find_all().await.map_err(AppError::Internal)?;
        let up_tunnels: Vec<_> = all_tunnels
            .into_iter()
            .filter(|t| t.status == wardnet_common::tunnel::TunnelStatus::Up)
            .collect();

        for tunnel in up_tunnels {
            match self
                .tunnel_interface
                .get_stats(&tunnel.interface_name)
                .await
            {
                Ok(Some(stats)) => {
                    if let Some(last_handshake) = stats.last_handshake {
                        let age = chrono::Utc::now() - last_handshake;
                        if age > stale_threshold {
                            tracing::warn!(
                                tunnel_id = %tunnel.id,
                                interface = %tunnel.interface_name,
                                last_handshake = %last_handshake,
                                age_secs = age.num_seconds(),
                                "health check: last handshake is stale (>3 minutes) for {}: age={}s",
                                tunnel.interface_name,
                                age.num_seconds()
                            );
                        }
                    }
                }
                Ok(None) => {
                    tracing::error!(
                        tunnel_id = %tunnel.id,
                        interface = %tunnel.interface_name,
                        "health check: interface {} not found (may have been removed externally)",
                        tunnel.interface_name
                    );
                }
                Err(e) => {
                    tracing::error!(
                        tunnel_id = %tunnel.id,
                        interface = %tunnel.interface_name,
                        error = %e,
                        "health check: failed to get stats for {}: {e}", tunnel.interface_name
                    );
                }
            }
        }
        Ok(())
    }
}
