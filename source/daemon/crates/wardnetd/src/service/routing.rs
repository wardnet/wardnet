use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;
use uuid::Uuid;
use wardnet_types::device::Device;
use wardnet_types::routing::{RoutingRule, RoutingTarget};
use wardnet_types::tunnel::TunnelStatus;

use crate::auth_context;
use crate::error::AppError;
use crate::firewall::FirewallManager;
use crate::policy_router::PolicyRouter;
use crate::repository::{DeviceRepository, TunnelRepository};
use crate::service::TunnelService;

/// Manages Linux kernel policy routing rules for per-device VPN routing.
///
/// Translates high-level [`RoutingTarget`] assignments into kernel operations:
/// - `ip rule` for source-based routing per device
/// - `ip route` for per-tunnel routing tables
/// - nftables masquerade for NAT on tunnel-bound traffic
/// - nftables DNS redirect to prevent DNS leaks
///
/// All kernel state modifications are serialized via [`tokio::sync::Mutex`] to
/// prevent race conditions from concurrent events (e.g. tunnel up + device rule
/// change arriving simultaneously).
#[async_trait]
#[allow(clippy::similar_names)]
pub trait RoutingService: Send + Sync {
    /// Apply a routing rule for a device.
    ///
    /// This may bring up tunnels on-demand, add ip rules, configure masquerade,
    /// and set up DNS redirects as needed. If kernel operations fail, the device
    /// silently falls back to direct routing.
    async fn apply_rule(
        &self,
        device_id: Uuid,
        device_ip: &str,
        target: &RoutingTarget,
    ) -> Result<(), AppError>;

    /// Remove all kernel routing state for a device (ip rules, DNS redirect).
    async fn remove_device_routes(&self, device_id: Uuid, device_ip: &str) -> Result<(), AppError>;

    /// Handle a device IP change — remove old rules and re-apply with new IP.
    async fn handle_ip_change(
        &self,
        device_id: Uuid,
        old_ip: &str,
        new_ip: &str,
    ) -> Result<(), AppError>;

    /// Handle a tunnel going down — remove all routes for devices using it.
    ///
    /// Affected devices fall back to direct routing until the tunnel comes back
    /// up and [`handle_tunnel_up`](Self::handle_tunnel_up) re-applies their rules.
    async fn handle_tunnel_down(&self, tunnel_id: Uuid) -> Result<(), AppError>;

    /// Handle a tunnel coming up — re-apply routing rules for devices targeting it.
    async fn handle_tunnel_up(&self, tunnel_id: Uuid) -> Result<(), AppError>;

    /// Reconcile kernel state with the database on startup.
    ///
    /// Enables IP forwarding, initialises nftables, and applies all stored rules.
    /// Cleans up any orphaned kernel rules that don't match the database.
    async fn reconcile(&self) -> Result<(), AppError>;

    /// Return the list of device IDs currently routing through the given tunnel.
    async fn devices_using_tunnel(&self, tunnel_id: Uuid) -> Result<Vec<Uuid>, AppError>;
}

/// Tracks kernel state that has been applied for a single device.
struct AppliedRule {
    /// The device's IP address for which kernel rules are configured.
    device_ip: String,
    /// The resolved routing target (never `Default` — always resolved).
    target: RoutingTarget,
    /// The routing table number if targeting a tunnel.
    table: Option<u32>,
    /// The tunnel ID if targeting a tunnel.
    tunnel_id: Option<Uuid>,
}

/// Aggregate kernel state tracked by the routing service.
struct RoutingState {
    /// Per-device applied kernel rules. Key is `device_id`.
    applied: HashMap<Uuid, AppliedRule>,
    /// Routing tables that have been configured with default route + masquerade.
    tunnel_tables: HashSet<u32>,
}

/// Default implementation of [`RoutingService`].
///
/// Coordinates between the device/tunnel repositories, tunnel lifecycle service,
/// and low-level kernel abstractions (netlink, nftables) to manage per-device
/// policy routing.
pub struct RoutingServiceImpl {
    devices: Arc<dyn DeviceRepository>,
    tunnel_repo: Arc<dyn TunnelRepository>,
    tunnels: Arc<dyn TunnelService>,
    netlink: Arc<dyn PolicyRouter>,
    nftables: Arc<dyn FirewallManager>,
    /// Global default routing policy from config (e.g. "direct").
    default_policy: String,
    /// LAN interface name (e.g. "eth1") for the base masquerade rule.
    lan_interface: String,
    /// Mutable in-memory state protected by a mutex.
    state: Mutex<RoutingState>,
}

impl RoutingServiceImpl {
    /// Create a new routing service with the given dependencies.
    pub fn new(
        devices: Arc<dyn DeviceRepository>,
        tunnel_repo: Arc<dyn TunnelRepository>,
        tunnels: Arc<dyn TunnelService>,
        netlink: Arc<dyn PolicyRouter>,
        nftables: Arc<dyn FirewallManager>,
        default_policy: String,
        lan_interface: String,
    ) -> Self {
        Self {
            devices,
            tunnel_repo,
            tunnels,
            netlink,
            nftables,
            default_policy,
            lan_interface,
            state: Mutex::new(RoutingState {
                applied: HashMap::new(),
                tunnel_tables: HashSet::new(),
            }),
        }
    }

    /// Resolve `RoutingTarget::Default` into a concrete target based on the
    /// global default policy.
    fn resolve_target(&self, target: &RoutingTarget) -> RoutingTarget {
        match target {
            RoutingTarget::Default => {
                let resolved = if self.default_policy == "direct" {
                    RoutingTarget::Direct
                } else if let Ok(tunnel_id) = self.default_policy.parse::<Uuid>() {
                    RoutingTarget::Tunnel { tunnel_id }
                } else {
                    tracing::warn!(
                        policy = %self.default_policy,
                        "unknown default policy, falling back to direct"
                    );
                    RoutingTarget::Direct
                };
                tracing::debug!(
                    policy = %self.default_policy,
                    ?resolved,
                    "resolved Default routing target"
                );
                resolved
            }
            other => other.clone(),
        }
    }

    /// Remove all kernel state for a device from the applied set.
    ///
    /// Removes ip rules and DNS redirects. Errors are logged but not propagated
    /// — partial cleanup is better than none.
    async fn remove_device_kernel_state(&self, state: &mut RoutingState, device_id: Uuid) {
        if let Some(rule) = state.applied.remove(&device_id) {
            tracing::debug!(
                device_id = %device_id,
                device_ip = %rule.device_ip,
                ?rule.target,
                table = ?rule.table,
                tunnel_id = ?rule.tunnel_id,
                "removing kernel state for device"
            );
            if let Some(table) = rule.table {
                tracing::debug!(
                    device_ip = %rule.device_ip,
                    table,
                    "removing ip rule"
                );
                if let Err(e) = self.netlink.remove_ip_rule(&rule.device_ip, table).await {
                    tracing::warn!(
                        error = %e,
                        device_ip = %rule.device_ip,
                        table,
                        "failed to remove ip rule"
                    );
                }
            }
            tracing::debug!(device_ip = %rule.device_ip, "removing DNS redirect");
            if let Err(e) = self.nftables.remove_dns_redirect(&rule.device_ip).await {
                tracing::warn!(
                    error = %e,
                    device_ip = %rule.device_ip,
                    "failed to remove DNS redirect"
                );
            }
        } else {
            tracing::debug!(
                device_id = %device_id,
                "no kernel state to remove for device"
            );
        }
    }

    /// Maximum number of retries when `add_route_table` fails because the
    /// kernel interface is not yet UP.
    const ROUTE_ADD_MAX_RETRIES: u32 = 5;

    /// Delay between retries when waiting for the interface to come UP.
    const ROUTE_ADD_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(200);

    /// Ensure the routing table for a tunnel interface is configured.
    ///
    /// Adds a default route through the interface and a masquerade rule if the
    /// table hasn't been set up yet. The kernel interface may not be fully UP
    /// when the daemon's internal state transitions, so `add_route_table` is
    /// retried up to [`Self::ROUTE_ADD_MAX_RETRIES`] times with a short delay
    /// if the error indicates the device is not yet ready.
    async fn ensure_tunnel_table(
        &self,
        state: &mut RoutingState,
        interface_name: &str,
        table: u32,
    ) -> Result<(), anyhow::Error> {
        if state.tunnel_tables.contains(&table) {
            tracing::debug!(
                interface = interface_name,
                table,
                "tunnel routing table already configured"
            );
        } else {
            tracing::debug!(
                interface = interface_name,
                table,
                "setting up new tunnel routing table"
            );
            let mut last_err = None;
            for attempt in 0..=Self::ROUTE_ADD_MAX_RETRIES {
                match self.netlink.add_route_table(interface_name, table).await {
                    Ok(()) => {
                        last_err = None;
                        break;
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        let is_not_up = msg.contains("not up") || msg.contains("not ready");
                        if is_not_up && attempt < Self::ROUTE_ADD_MAX_RETRIES {
                            tracing::debug!(
                                interface = interface_name,
                                table,
                                attempt = attempt + 1,
                                max_retries = Self::ROUTE_ADD_MAX_RETRIES,
                                error = %e,
                                "interface not yet UP, retrying after delay"
                            );
                            tokio::time::sleep(Self::ROUTE_ADD_RETRY_DELAY).await;
                            last_err = Some(e);
                        } else {
                            return Err(e);
                        }
                    }
                }
            }
            if let Some(e) = last_err {
                return Err(e);
            }
            tracing::debug!(
                interface = interface_name,
                table,
                "added default route in table"
            );
            self.nftables.add_masquerade(interface_name).await?;
            tracing::debug!(interface = interface_name, "added masquerade rule");
            state.tunnel_tables.insert(table);
        }
        Ok(())
    }

    /// Load all devices that have a routing rule targeting a specific tunnel.
    async fn load_devices_targeting_tunnel(
        &self,
        tunnel_id: Uuid,
    ) -> Result<Vec<(Device, RoutingRule)>, AppError> {
        let all_devices = self.devices.find_all().await.map_err(AppError::Internal)?;
        tracing::debug!(
            tunnel_id = %tunnel_id,
            total_devices = all_devices.len(),
            "scanning devices for tunnel routing rules"
        );
        let mut result = Vec::new();

        for device in all_devices {
            if let Some(rule) = self
                .devices
                .find_rule_for_device(&device.id.to_string())
                .await
                .map_err(AppError::Internal)?
            {
                let resolved = self.resolve_target(&rule.target);
                if let RoutingTarget::Tunnel { tunnel_id: tid, .. } = &resolved
                    && *tid == tunnel_id
                {
                    tracing::debug!(
                        device_id = %device.id,
                        device_ip = %device.last_ip,
                        "device targets this tunnel"
                    );
                    result.push((device, rule));
                }
            }
        }

        tracing::debug!(
            tunnel_id = %tunnel_id,
            matched = result.len(),
            "finished scanning devices for tunnel"
        );
        Ok(result)
    }
}

/// Extract the numeric index from a Wardnet tunnel interface name.
///
/// For example, `"wg_ward0"` returns `Some(0)` and `"wg_ward12"` returns `Some(12)`.
fn parse_interface_index(interface_name: &str) -> Option<u32> {
    interface_name.strip_prefix("wg_ward")?.parse().ok()
}

/// Compute the routing table number for a tunnel interface index.
///
/// Wardnet uses tables starting at 100 to avoid collision with the main/local
/// tables. Index 0 maps to table 100, index 3 maps to table 103, etc.
fn table_for_index(index: u32) -> u32 {
    100 + index
}

#[async_trait]
#[allow(clippy::too_many_lines)]
impl RoutingService for RoutingServiceImpl {
    #[allow(clippy::similar_names)]
    async fn apply_rule(
        &self,
        device_id: Uuid,
        device_ip: &str,
        target: &RoutingTarget,
    ) -> Result<(), AppError> {
        auth_context::require_admin()?;
        tracing::debug!(
            device_id = %device_id,
            device_ip,
            ?target,
            "apply_rule called"
        );
        let resolved = self.resolve_target(target);

        // -- Phase 1: Check existing state (short lock) ----------------------
        {
            let state = self.state.lock().await;
            if let Some(existing) = state.applied.get(&device_id) {
                if existing.target == resolved && existing.device_ip == device_ip {
                    tracing::debug!(
                        device_id = %device_id,
                        device_ip,
                        ?resolved,
                        "rule already applied with same target and IP, skipping"
                    );
                    return Ok(());
                }
                tracing::debug!(
                    device_id = %device_id,
                    old_ip = %existing.device_ip,
                    new_ip = device_ip,
                    old_target = ?existing.target,
                    new_target = ?resolved,
                    "rule differs from applied state, will re-apply"
                );
            } else {
                tracing::debug!(
                    device_id = %device_id,
                    "no existing applied rule for device"
                );
            }
        }

        // -- Phase 2: Tunnel operations (no lock held) -----------------------
        // If targeting a tunnel, gather the info we need outside the lock to
        // avoid holding it across potentially slow tunnel bring-up.
        let tunnel_info = if let RoutingTarget::Tunnel { tunnel_id } = &resolved {
            match self.tunnels.get_tunnel(*tunnel_id).await {
                Ok(tunnel) => {
                    tracing::debug!(
                        tunnel_id = %tunnel_id,
                        interface = %tunnel.interface_name,
                        status = ?tunnel.status,
                        "fetched tunnel for routing"
                    );
                    // Bring tunnel up if it's down.
                    if tunnel.status == TunnelStatus::Down {
                        tracing::debug!(
                            tunnel_id = %tunnel_id,
                            "tunnel is down, attempting on-demand bring-up"
                        );
                        if let Err(e) = self.tunnels.bring_up_internal(*tunnel_id).await {
                            tracing::warn!(
                                error = %e,
                                tunnel_id = %tunnel_id,
                                "failed to bring up tunnel, falling back to direct"
                            );
                            None
                        } else {
                            // Re-fetch to get updated interface name etc.
                            match self.tunnels.get_tunnel(*tunnel_id).await {
                                Ok(t) => Some(t),
                                Err(e) => {
                                    tracing::warn!(
                                        error = %e,
                                        tunnel_id = %tunnel_id,
                                        "failed to re-fetch tunnel after bring-up"
                                    );
                                    None
                                }
                            }
                        }
                    } else {
                        Some(tunnel)
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        tunnel_id = %tunnel_id,
                        "tunnel not found, falling back to direct"
                    );
                    None
                }
            }
        } else {
            None
        };

        // Fetch tunnel DNS if we have a tunnel.
        let dns_ip = if let Some(ref tunnel) = tunnel_info {
            tracing::debug!(
                tunnel_id = %tunnel.id,
                "loading tunnel config for DNS servers"
            );
            match self
                .tunnel_repo
                .find_config_by_id(&tunnel.id.to_string())
                .await
            {
                Ok(Some(config)) => {
                    let dns = config.dns.first().cloned();
                    tracing::debug!(
                        tunnel_id = %tunnel.id,
                        dns_servers = ?config.dns,
                        selected_dns = ?dns,
                        "resolved tunnel DNS configuration"
                    );
                    dns
                }
                Ok(None) => {
                    tracing::debug!(
                        tunnel_id = %tunnel.id,
                        "no tunnel config found, DNS redirect will be skipped"
                    );
                    None
                }
                Err(e) => {
                    tracing::warn!(error = %e, "failed to load tunnel config for DNS");
                    None
                }
            }
        } else {
            None
        };

        // -- Phase 3: Apply kernel state (locked) ----------------------------
        let mut state = self.state.lock().await;

        // Re-check: another concurrent apply may have changed state while we
        // were doing tunnel operations without the lock.
        if let Some(existing) = state.applied.get(&device_id)
            && existing.target == resolved
            && existing.device_ip == device_ip
        {
            tracing::debug!(
                device_id = %device_id,
                "rule was applied by concurrent call while lock was released, skipping"
            );
            return Ok(());
        }

        // Remove old kernel state if present.
        self.remove_device_kernel_state(&mut state, device_id).await;

        // If targeting a tunnel and we have tunnel info, configure routing.
        if let (RoutingTarget::Tunnel { tunnel_id }, Some(tunnel)) = (&resolved, &tunnel_info) {
            let Some(index) = parse_interface_index(&tunnel.interface_name) else {
                tracing::warn!(
                    interface = %tunnel.interface_name,
                    "could not parse interface index, falling back to direct"
                );
                state.applied.insert(
                    device_id,
                    AppliedRule {
                        device_ip: device_ip.to_owned(),
                        target: RoutingTarget::Direct,
                        table: None,
                        tunnel_id: None,
                    },
                );
                return Ok(());
            };
            let table = table_for_index(index);

            // Ensure tunnel routing table is set up.
            if let Err(e) = self
                .ensure_tunnel_table(&mut state, &tunnel.interface_name, table)
                .await
            {
                tracing::warn!(
                    error = %e,
                    interface = %tunnel.interface_name,
                    table,
                    "failed to set up tunnel routing table, falling back to direct"
                );
                state.applied.insert(
                    device_id,
                    AppliedRule {
                        device_ip: device_ip.to_owned(),
                        target: RoutingTarget::Direct,
                        table: None,
                        tunnel_id: None,
                    },
                );
                return Ok(());
            }

            // Add source-based ip rule.
            tracing::debug!(device_ip, table, "adding ip rule");
            if let Err(e) = self.netlink.add_ip_rule(device_ip, table).await {
                tracing::warn!(
                    error = %e,
                    device_ip,
                    table,
                    "failed to add ip rule, falling back to direct"
                );
                state.applied.insert(
                    device_id,
                    AppliedRule {
                        device_ip: device_ip.to_owned(),
                        target: RoutingTarget::Direct,
                        table: None,
                        tunnel_id: None,
                    },
                );
                return Ok(());
            }

            tracing::debug!(device_ip, table, "ip rule added successfully");

            // Add DNS redirect if tunnel has DNS servers.
            if let Some(ref dns) = dns_ip {
                tracing::debug!(device_ip, dns, "adding DNS redirect");
                if let Err(e) = self.nftables.add_dns_redirect(device_ip, dns).await {
                    tracing::warn!(
                        error = %e,
                        device_ip,
                        dns,
                        "failed to add DNS redirect (non-fatal)"
                    );
                } else {
                    tracing::debug!(device_ip, dns, "DNS redirect added successfully");
                }
            } else {
                tracing::debug!(device_ip, "no tunnel DNS configured, skipping DNS redirect");
            }

            tracing::info!(
                device_id = %device_id,
                device_ip,
                tunnel_id = %tunnel_id,
                interface = %tunnel.interface_name,
                table,
                "applied tunnel routing rule"
            );

            state.applied.insert(
                device_id,
                AppliedRule {
                    device_ip: device_ip.to_owned(),
                    target: resolved.clone(),
                    table: Some(table),
                    tunnel_id: Some(*tunnel_id),
                },
            );
        } else {
            // Direct routing — no kernel state needed, the default route handles it.
            tracing::info!(
                device_id = %device_id,
                device_ip,
                "applied direct routing rule"
            );

            state.applied.insert(
                device_id,
                AppliedRule {
                    device_ip: device_ip.to_owned(),
                    target: RoutingTarget::Direct,
                    table: None,
                    tunnel_id: None,
                },
            );
        }

        Ok(())
    }

    async fn remove_device_routes(
        &self,
        device_id: Uuid,
        _device_ip: &str,
    ) -> Result<(), AppError> {
        auth_context::require_admin()?;
        tracing::debug!(device_id = %device_id, "remove_device_routes called");
        let mut state = self.state.lock().await;
        self.remove_device_kernel_state(&mut state, device_id).await;
        tracing::info!(device_id = %device_id, "removed device routing state");
        Ok(())
    }

    async fn handle_ip_change(
        &self,
        device_id: Uuid,
        old_ip: &str,
        new_ip: &str,
    ) -> Result<(), AppError> {
        auth_context::require_admin()?;
        tracing::debug!(
            device_id = %device_id,
            old_ip,
            new_ip,
            "handle_ip_change called"
        );
        // Capture the target from the old rule before removing it.
        let target = {
            let mut state = self.state.lock().await;
            let target = state.applied.get(&device_id).map(|r| r.target.clone());
            self.remove_device_kernel_state(&mut state, device_id).await;
            target
        };

        if let Some(target) = target {
            tracing::info!(
                device_id = %device_id,
                old_ip,
                new_ip,
                ?target,
                "re-applying routing rule after IP change"
            );
            self.apply_rule(device_id, new_ip, &target).await?;
        } else {
            tracing::debug!(
                device_id = %device_id,
                old_ip,
                new_ip,
                "no applied routing rule for device, nothing to re-apply after IP change"
            );
        }

        Ok(())
    }

    async fn handle_tunnel_down(&self, tunnel_id: Uuid) -> Result<(), AppError> {
        auth_context::require_admin()?;
        tracing::debug!(tunnel_id = %tunnel_id, "handle_tunnel_down called");
        let mut state = self.state.lock().await;

        // Find all devices using this tunnel.
        let affected: Vec<Uuid> = state
            .applied
            .iter()
            .filter(|(_, rule)| rule.tunnel_id == Some(tunnel_id))
            .map(|(id, _)| *id)
            .collect();

        // Find the table used by this tunnel so we can clean it up.
        let tunnel_table = state
            .applied
            .values()
            .find(|r| r.tunnel_id == Some(tunnel_id))
            .and_then(|r| r.table);

        if affected.is_empty() {
            tracing::debug!(
                tunnel_id = %tunnel_id,
                "no devices currently routing through this tunnel"
            );
        } else {
            tracing::warn!(
                tunnel_id = %tunnel_id,
                affected_count = affected.len(),
                table = ?tunnel_table,
                "tunnel down — removing routing for affected devices"
            );
        }

        // Remove kernel state for each affected device.
        for device_id in &affected {
            tracing::warn!(
                device_id = %device_id,
                tunnel_id = %tunnel_id,
                "tunnel down — removing routing for device"
            );
            self.remove_device_kernel_state(&mut state, *device_id)
                .await;
        }

        // Clean up the tunnel's routing table and masquerade.
        if let Some(table) = tunnel_table {
            tracing::debug!(
                tunnel_id = %tunnel_id,
                table,
                "cleaning up tunnel routing table"
            );
            if let Err(e) = self.netlink.remove_route_table(table).await {
                tracing::warn!(error = %e, table, "failed to remove tunnel route table");
            }
            // We can't easily remove a specific masquerade rule by table alone,
            // but the nftables flush on reconcile will handle cleanup.
            state.tunnel_tables.remove(&table);
        }

        tracing::debug!(
            tunnel_id = %tunnel_id,
            affected_count = affected.len(),
            "handle_tunnel_down complete"
        );
        Ok(())
    }

    async fn handle_tunnel_up(&self, tunnel_id: Uuid) -> Result<(), AppError> {
        auth_context::require_admin()?;
        tracing::debug!(tunnel_id = %tunnel_id, "handle_tunnel_up called");
        let devices = self.load_devices_targeting_tunnel(tunnel_id).await?;

        if devices.is_empty() {
            tracing::debug!(
                tunnel_id = %tunnel_id,
                "no devices targeting this tunnel, nothing to re-apply"
            );
            return Ok(());
        }

        tracing::info!(
            tunnel_id = %tunnel_id,
            device_count = devices.len(),
            "tunnel up — re-applying routing rules for devices"
        );

        let mut success_count = 0u32;
        for (device, rule) in &devices {
            tracing::debug!(
                device_id = %device.id,
                device_ip = %device.last_ip,
                tunnel_id = %tunnel_id,
                ?rule.target,
                "re-applying routing rule for device"
            );
            if let Err(e) = self
                .apply_rule(device.id, &device.last_ip, &rule.target)
                .await
            {
                tracing::warn!(
                    error = %e,
                    device_id = %device.id,
                    "failed to re-apply routing rule after tunnel up"
                );
            } else {
                success_count += 1;
            }
        }

        let total = devices.len();
        tracing::debug!(
            tunnel_id = %tunnel_id,
            total,
            success_count,
            failed = total.saturating_sub(success_count as usize),
            "handle_tunnel_up complete"
        );
        Ok(())
    }

    async fn reconcile(&self) -> Result<(), AppError> {
        auth_context::require_admin()?;
        tracing::info!("reconciling routing state with kernel");

        // Check tool availability.
        tracing::debug!("checking netlink tool availability");
        self.netlink
            .check_tools_available()
            .await
            .map_err(AppError::Internal)?;
        tracing::debug!("checking nftables tool availability");
        self.nftables
            .check_tools_available()
            .await
            .map_err(AppError::Internal)?;
        tracing::debug!("system tools verified");

        // Enable IP forwarding.
        tracing::debug!("enabling IP forwarding");
        self.netlink
            .enable_ip_forwarding()
            .await
            .map_err(AppError::Internal)?;
        tracing::debug!("IP forwarding enabled");

        // Initialise nftables table (idempotent).
        tracing::debug!("initialising nftables wardnet table");
        self.nftables
            .init_wardnet_table()
            .await
            .map_err(AppError::Internal)?;

        // Flush nftables rules to start clean.
        tracing::debug!("flushing nftables wardnet table");
        self.nftables
            .flush_wardnet_table()
            .await
            .map_err(AppError::Internal)?;
        tracing::debug!("nftables table flushed");

        // Add base LAN masquerade rule so forwarded traffic from devices using
        // the Pi as their gateway gets NAT'd for the upstream router.
        tracing::debug!(interface = %self.lan_interface, "adding LAN masquerade rule");
        self.nftables
            .add_masquerade(&self.lan_interface)
            .await
            .map_err(AppError::Internal)?;

        // Clear in-memory state since we flushed kernel state.
        {
            let mut state = self.state.lock().await;
            tracing::debug!(
                previously_applied = state.applied.len(),
                previously_tracked_tables = state.tunnel_tables.len(),
                "clearing in-memory routing state"
            );
            state.applied.clear();
            state.tunnel_tables.clear();
        }

        // Load all devices and apply rules for those that have them.
        let all_devices = self.devices.find_all().await.map_err(AppError::Internal)?;
        tracing::debug!(
            device_count = all_devices.len(),
            "loaded devices from database for reconciliation"
        );
        let mut applied_count = 0u32;

        for device in &all_devices {
            if let Some(rule) = self
                .devices
                .find_rule_for_device(&device.id.to_string())
                .await
                .map_err(AppError::Internal)?
            {
                tracing::debug!(
                    device_id = %device.id,
                    device_ip = %device.last_ip,
                    target = ?rule.target,
                    "reconciling routing rule for device"
                );
                if let Err(e) = self
                    .apply_rule(device.id, &device.last_ip, &rule.target)
                    .await
                {
                    tracing::warn!(
                        error = %e,
                        device_id = %device.id,
                        "failed to apply routing rule during reconcile"
                    );
                } else {
                    applied_count += 1;
                }
            }
        }

        // Clean up orphaned ip rules — any kernel rules that we didn't apply.
        tracing::debug!("checking for orphaned kernel ip rules");
        match self.netlink.list_wardnet_rules().await {
            Ok(kernel_rules) => {
                tracing::debug!(
                    kernel_rule_count = kernel_rules.len(),
                    "found kernel ip rules"
                );
                let state = self.state.lock().await;
                let known_ips: HashSet<&str> = state
                    .applied
                    .values()
                    .filter_map(|r| r.table.map(|_| r.device_ip.as_str()))
                    .collect();

                let mut orphan_count = 0u32;
                for (src_ip, table) in &kernel_rules {
                    if !known_ips.contains(src_ip.as_str()) {
                        orphan_count += 1;
                        tracing::warn!(src_ip, table, "removing orphaned ip rule");
                        if let Err(e) = self.netlink.remove_ip_rule(src_ip, *table).await {
                            tracing::warn!(error = %e, src_ip, table, "failed to remove orphaned ip rule");
                        }
                    }
                }
                if orphan_count > 0 {
                    tracing::info!(orphan_count, "cleaned up orphaned ip rules");
                } else {
                    tracing::debug!("no orphaned ip rules found");
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to list kernel ip rules for orphan cleanup");
            }
        }

        tracing::info!(
            applied_count,
            total_devices = all_devices.len(),
            "routing reconciliation complete"
        );

        Ok(())
    }

    async fn devices_using_tunnel(&self, tunnel_id: Uuid) -> Result<Vec<Uuid>, AppError> {
        auth_context::require_admin()?;
        let state = self.state.lock().await;
        let result: Vec<Uuid> = state
            .applied
            .iter()
            .filter(|(_, rule)| rule.tunnel_id == Some(tunnel_id))
            .map(|(id, _)| *id)
            .collect();
        tracing::debug!(
            tunnel_id = %tunnel_id,
            device_count = result.len(),
            "queried devices using tunnel"
        );
        Ok(result)
    }
}
