use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use arc_swap::ArcSwap;
use async_trait::async_trait;
use tokio::sync::Mutex;
use uuid::Uuid;
use wardnet_common::device::Device;
use wardnet_common::dns::UpstreamId;
use wardnet_common::event::WardnetEvent;
use wardnet_common::routing::{RoutingRule, RoutingTarget};
use wardnet_common::tunnel::TunnelStatus;

use crate::TunnelService;
use crate::auth_context;
use crate::error::AppError;
use crate::event::EventPublisher;
use crate::routing::firewall::FirewallManager;
use crate::routing::policy_router::PolicyRouter;
use wardnetd_data::repository::{DeviceRepository, SystemConfigRepository, TunnelRepository};

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

    /// Handle a lost routing table — re-apply routes for all devices using it.
    ///
    /// Triggered by the route monitor when the kernel deletes a route from a
    /// Wardnet-managed routing table (tables >= 100). Marks the table as
    /// unconfigured in memory so the next [`apply_rule`](Self::apply_rule)
    /// re-creates it.
    async fn handle_route_table_lost(&self, table: u32) -> Result<(), AppError>;

    /// Return the list of device IDs currently routing through the given tunnel.
    async fn devices_using_tunnel(&self, tunnel_id: Uuid) -> Result<Vec<Uuid>, AppError>;

    /// Look up device by ID and apply its routing rule.
    ///
    /// Used by the routing listener to handle `RoutingRuleChanged` events without
    /// the listener needing direct repository access.
    /// No auth guard — callers wrap this in `auth_context::with_context(...)`.
    async fn apply_rule_for_device(
        &self,
        device_id: Uuid,
        target: &RoutingTarget,
    ) -> Result<(), AppError>;

    /// Check if a newly-discovered device has a persisted routing rule and apply it.
    ///
    /// Used by the routing listener to handle `DeviceDiscovered` events.
    /// No auth guard — callers wrap this in `auth_context::with_context(...)`.
    async fn apply_rule_for_discovered_device(
        &self,
        device_id: Uuid,
        ip: &str,
    ) -> Result<(), AppError>;

    /// Update the global default routing policy.
    ///
    /// Validates `policy` (must be `"direct"` or a tunnel UUID),
    /// persists it to `system_config`, and updates the in-memory
    /// state used by [`Self::apply_rule`] to resolve
    /// [`RoutingTarget::Default`]. Devices whose stored rule is
    /// `RoutingTarget::Default` will pick up the new policy on
    /// their next apply or reconcile.
    async fn set_default_policy(&self, policy: &str) -> Result<(), AppError>;

    /// Read the current global default routing policy.
    async fn default_policy(&self) -> Result<String, AppError>;

    /// Lock-free, atomically swappable snapshot of `device_ip → UpstreamId`
    /// for the DNS server's per-query upstream selection.
    ///
    /// Returned entries map a tunneled device's IP to `Tunnel(id)` only
    /// when the targeted tunnel has `override_default_dns = true`.
    /// Lookup misses (LAN devices, or tunneled devices with override
    /// disabled) implicitly resolve to `UpstreamId::Default`.
    fn dns_upstream_snapshot(&self) -> Arc<ArcSwap<HashMap<IpAddr, UpstreamId>>>;

    /// Force a rebuild + atomic swap of the snapshot returned by
    /// [`Self::dns_upstream_snapshot`]. Called on
    /// [`wardnet_common::event::WardnetEvent::TunnelDnsOverrideChanged`]
    /// so already-applied device rules pick up the new upstream choice
    /// without waiting for the next routing-rule mutation.
    async fn rebuild_dns_upstream_snapshot(&self) -> Result<(), AppError>;
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
    /// The DNS upstream this device's queries should be forwarded to.
    /// `Tunnel(id)` only when the device targets a tunnel **and** that
    /// tunnel has `override_default_dns = true`. Otherwise `Default`.
    /// Read by the DNS server's per-query upstream selection.
    dns_upstream: UpstreamId,
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
    system_config: Arc<dyn SystemConfigRepository>,
    /// Event bus, used to announce a default-policy change so the Network-Zone
    /// enforcer (#736) can re-validate `Default`-ruled devices against their
    /// zones — the one edge the #735 write-time gate cannot catch.
    events: Arc<dyn EventPublisher>,
    /// Global default routing policy (e.g. `"direct"` or a tunnel UUID).
    /// Held in a `RwLock` so [`Self::set_default_policy`] can update it
    /// at runtime without restarting the daemon.
    default_policy: Arc<RwLock<String>>,
    /// LAN interface name (e.g. "eth1") for the base masquerade rule.
    lan_interface: String,
    /// Mutable in-memory state protected by a mutex.
    state: Mutex<RoutingState>,
    /// `device_ip → UpstreamId` snapshot consulted lock-free by the DNS
    /// server hot path. Rebuilt and atomically swapped on every change
    /// to [`RoutingState::applied`].
    dns_upstream_snapshot: Arc<ArcSwap<HashMap<IpAddr, UpstreamId>>>,
}

impl RoutingServiceImpl {
    /// Create a new routing service with the given dependencies.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        devices: Arc<dyn DeviceRepository>,
        tunnel_repo: Arc<dyn TunnelRepository>,
        tunnels: Arc<dyn TunnelService>,
        netlink: Arc<dyn PolicyRouter>,
        nftables: Arc<dyn FirewallManager>,
        system_config: Arc<dyn SystemConfigRepository>,
        events: Arc<dyn EventPublisher>,
        default_policy: String,
        lan_interface: String,
    ) -> Self {
        Self {
            devices,
            tunnel_repo,
            tunnels,
            netlink,
            nftables,
            system_config,
            events,
            default_policy: Arc::new(RwLock::new(default_policy)),
            lan_interface,
            state: Mutex::new(RoutingState {
                applied: HashMap::new(),
                tunnel_tables: HashSet::new(),
            }),
            dns_upstream_snapshot: Arc::new(ArcSwap::from_pointee(HashMap::new())),
        }
    }

    /// Walk [`RoutingState::applied`] and produce the fresh
    /// `device_ip → UpstreamId` map.
    fn build_dns_upstream_map(state: &RoutingState) -> HashMap<IpAddr, UpstreamId> {
        let mut map = HashMap::with_capacity(state.applied.len());
        for rule in state.applied.values() {
            if matches!(rule.dns_upstream, UpstreamId::Default) {
                continue;
            }
            let Ok(ip) = rule.device_ip.parse::<IpAddr>() else {
                tracing::warn!(
                    device_ip = %rule.device_ip,
                    "skipping invalid IP in DNS upstream snapshot rebuild"
                );
                continue;
            };
            map.insert(ip, rule.dns_upstream);
        }
        map
    }

    /// Rebuild and atomically swap the DNS-upstream snapshot. Caller must
    /// hold (or have just released) the [`RoutingState`] mutex so the
    /// snapshot reflects a consistent view.
    fn refresh_dns_upstream_snapshot(&self, state: &RoutingState) {
        let map = Self::build_dns_upstream_map(state);
        tracing::debug!(
            entry_count = map.len(),
            "rebuilt DNS upstream snapshot from routing state"
        );
        self.dns_upstream_snapshot.store(Arc::new(map));
    }

    /// Snapshot the current default policy.
    ///
    /// The `RwLock` is poisoned only on a previous panic while the lock
    /// was held; in that case we fall back to `"direct"` rather than
    /// propagating a panic into routing decisions.
    fn current_default_policy(&self) -> String {
        self.default_policy.read().map_or_else(
            |e| {
                tracing::error!(error = %e, "default_policy lock poisoned, falling back to direct");
                "direct".to_owned()
            },
            |guard| guard.clone(),
        )
    }

    /// Resolve `RoutingTarget::Default` into a concrete target based on the
    /// global default policy.
    fn resolve_target(&self, target: &RoutingTarget) -> RoutingTarget {
        match target {
            RoutingTarget::Default => {
                let policy = self.current_default_policy();
                // Shared classifier — the single source of truth also used by
                // the Network-Zone gate in `DeviceService`.
                let resolved = RoutingTarget::from_default_policy(&policy);
                tracing::debug!(
                    policy = %policy,
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
                    "removing ip rule for device {device_ip}, table={table}",
                    device_ip = rule.device_ip,
                    table = table
                );
                // Loop until the kernel reports no matching rule, clearing any
                // duplicates that accumulated from restarts or races (issue #78).
                let mut removed = 0u32;
                loop {
                    match self.netlink.remove_ip_rule(&rule.device_ip, table).await {
                        Ok(()) => removed += 1,
                        Err(e) => {
                            if removed == 0 {
                                tracing::warn!(
                                    error = %e,
                                    device_ip = %rule.device_ip,
                                    table,
                                    "failed to remove ip rule for {device_ip}, table={table}: {e}",
                                    device_ip = rule.device_ip,
                                    table = table
                                );
                            }
                            break;
                        }
                    }
                }
                if removed > 1 {
                    tracing::warn!(
                        device_ip = %rule.device_ip,
                        table,
                        removed,
                        "drained duplicate ip rules for {device_ip}: device_ip={device_ip}, table={table}, removed={removed}",
                        device_ip = rule.device_ip,
                        table = table,
                        removed = removed
                    );
                }
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
            // Verify the kernel route still exists — it can vanish if the
            // interface was recreated or removed externally.
            match self.netlink.has_route_table(table).await {
                Ok(true) => {
                    tracing::debug!(
                        interface = interface_name,
                        table,
                        "tunnel routing table verified in kernel"
                    );
                }
                Ok(false) => {
                    tracing::warn!(
                        interface = interface_name,
                        table,
                        "tunnel routing table missing from kernel, will re-add"
                    );
                    state.tunnel_tables.remove(&table);
                }
                Err(e) => {
                    tracing::warn!(
                        interface = interface_name,
                        table,
                        error = %e,
                        "failed to verify tunnel routing table, will re-add"
                    );
                    state.tunnel_tables.remove(&table);
                }
            }
        }
        if !state.tunnel_tables.contains(&table) {
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

    /// Delay between adding the TCP RST reject rule and removing it.
    ///
    /// Must be long enough for the device to retransmit at least once and
    /// receive the RST, but short enough to avoid blocking new connections.
    const TCP_RST_HOLD_DURATION: Duration = Duration::from_millis(1500);

    /// Flush stale connections for a device after a routing change.
    ///
    /// Injects a temporary nftables `reject with tcp reset` rule so the
    /// device's stale TCP sockets receive RSTs instead of silently timing
    /// out over 30-60s. The sequence is:
    ///
    /// 1. Add TCP RST reject rule in the forward chain
    /// 2. Flush conntrack entries for the device
    /// 3. Hold briefly for the device's retransmits to hit the reject rule
    /// 4. Remove the reject rule
    /// 5. Flush route cache
    async fn flush_stale_connections(&self, device_ip: &str) {
        // 1. Add TCP RST reject rule (non-fatal — the device will still
        //    recover via TCP timeout if this fails).
        let rst_added = match self.nftables.add_tcp_reset_reject(device_ip).await {
            Ok(()) => {
                tracing::debug!(device_ip, "added TCP RST reject rule");
                true
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    device_ip,
                    "failed to add TCP RST reject rule (device may experience ~30s delay)"
                );
                false
            }
        };

        // 2. Flush conntrack so existing flows are not pinned to the previous
        //    route. When the device retransmits, packets hit the reject rule.
        if let Err(e) = self.netlink.flush_conntrack(device_ip).await {
            tracing::warn!(
                error = %e,
                device_ip,
                "failed to flush conntrack (existing flows may stay on previous route)"
            );
        }

        // 3. Hold for device retransmits.
        if rst_added {
            tracing::debug!(
                device_ip,
                hold_ms = Self::TCP_RST_HOLD_DURATION.as_millis(),
                "holding TCP RST reject rule"
            );
            tokio::time::sleep(Self::TCP_RST_HOLD_DURATION).await;

            // 4. Remove the reject rule so new connections work normally.
            if let Err(e) = self.nftables.remove_tcp_reset_reject(device_ip).await {
                tracing::warn!(
                    error = %e,
                    device_ip,
                    "failed to remove TCP RST reject rule"
                );
            } else {
                tracing::debug!(device_ip, "removed TCP RST reject rule");
            }
        }

        // 5. Flush route cache.
        if let Err(e) = self.netlink.flush_route_cache().await {
            tracing::warn!(
                error = %e,
                "failed to flush route cache (new packets may follow cached path briefly)"
            );
        }
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

        // Fetch tunnel DNS-override flag if we have a tunnel. When the
        // tunnel has `override_default_dns = true`, we mark this device's
        // DNS upstream as `Tunnel(id)` so wardnet's DNS server forwards
        // its queries via a `SO_BINDTODEVICE`-bound socket on the tunnel.
        // No nftables prerouting DNAT is installed — see issue #342.
        let tunnel_dns_override = if let Some(ref tunnel) = tunnel_info {
            tracing::debug!(
                tunnel_id = %tunnel.id,
                "loading tunnel config for DNS override flag"
            );
            match self
                .tunnel_repo
                .find_config_by_id(&tunnel.id.to_string())
                .await
            {
                Ok(Some(config)) => {
                    let on = config.override_default_dns && !config.dns.is_empty();
                    tracing::debug!(
                        tunnel_id = %tunnel.id,
                        override_default_dns = config.override_default_dns,
                        has_dns = !config.dns.is_empty(),
                        active = on,
                        "resolved tunnel DNS override"
                    );
                    on
                }
                Ok(None) => {
                    tracing::debug!(
                        tunnel_id = %tunnel.id,
                        "no tunnel config found, DNS override defaults to off"
                    );
                    false
                }
                Err(e) => {
                    tracing::warn!(error = %e, "failed to load tunnel config for DNS override");
                    false
                }
            }
        } else {
            false
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
                        dns_upstream: UpstreamId::Default,
                    },
                );
                self.refresh_dns_upstream_snapshot(&state);
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
                        dns_upstream: UpstreamId::Default,
                    },
                );
                self.refresh_dns_upstream_snapshot(&state);
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
                        dns_upstream: UpstreamId::Default,
                    },
                );
                self.refresh_dns_upstream_snapshot(&state);
                return Ok(());
            }

            tracing::debug!(device_ip, table, "ip rule added successfully");

            let dns_upstream = if tunnel_dns_override {
                UpstreamId::Tunnel(*tunnel_id)
            } else {
                UpstreamId::Default
            };

            tracing::info!(
                device_id = %device_id,
                device_ip,
                tunnel_id = %tunnel_id,
                interface = %tunnel.interface_name,
                table,
                ?dns_upstream,
                "applied tunnel routing rule"
            );

            state.applied.insert(
                device_id,
                AppliedRule {
                    device_ip: device_ip.to_owned(),
                    target: resolved.clone(),
                    table: Some(table),
                    tunnel_id: Some(*tunnel_id),
                    dns_upstream,
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
                    dns_upstream: UpstreamId::Default,
                },
            );
        }

        self.refresh_dns_upstream_snapshot(&state);

        // Flush stale connections: inject temporary TCP RST reject rule,
        // flush conntrack, wait for device retransmits, then clean up.
        // Must run *after* the new ip rule is in place so re-opened
        // connections pick up the new table.
        drop(state);
        self.flush_stale_connections(device_ip).await;

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
        self.refresh_dns_upstream_snapshot(&state);
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

        // Remove kernel state for each affected device, capturing IPs so we
        // can flush their conntrack entries after releasing the lock.
        let mut affected_ips: Vec<String> = Vec::with_capacity(affected.len());
        for device_id in &affected {
            tracing::warn!(
                device_id = %device_id,
                tunnel_id = %tunnel_id,
                "tunnel down — removing routing for device"
            );
            if let Some(rule) = state.applied.get(device_id) {
                affected_ips.push(rule.device_ip.clone());
            }
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

        // Release the lock before flushing stale connections for affected
        // devices — without this, existing flows stay pinned to the now-dead
        // tunnel route instead of falling back to the default route.
        self.refresh_dns_upstream_snapshot(&state);
        drop(state);
        for ip in &affected_ips {
            self.flush_stale_connections(ip).await;
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

        // One-shot upgrade migration: scrub leftover `wardnet:dns:*`
        // prerouting DNAT rules from previous daemon versions (the
        // mechanism removed by #342). Idempotent. Runs *before* the
        // table flush so the cleanup also covers daemons that crash
        // between init and flush. See `firewall_nftables.rs`.
        if let Err(e) = self.nftables.cleanup_legacy_dns_redirects().await {
            tracing::warn!(error = %e, "failed to clean up legacy DNS redirect rules");
        }

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

                // Group by (ip, table) to detect both orphans and duplicates.
                let mut ip_rule_counts: HashMap<(String, u32), u32> = HashMap::new();
                for (src_ip, table) in &kernel_rules {
                    *ip_rule_counts.entry((src_ip.clone(), *table)).or_insert(0) += 1;
                }

                let mut orphan_count = 0u32;
                let mut duplicate_count = 0u32;
                for ((src_ip, table), count) in &ip_rule_counts {
                    if !known_ips.contains(src_ip.as_str()) {
                        // Orphan: remove all occurrences of this rule.
                        for _ in 0..*count {
                            tracing::warn!(
                                src_ip = %src_ip,
                                table,
                                "removing orphaned ip rule: src_ip={src_ip}, table={table}",
                                src_ip = src_ip,
                                table = table
                            );
                            if let Err(e) = self.netlink.remove_ip_rule(src_ip, *table).await {
                                tracing::warn!(
                                    error = %e,
                                    src_ip = %src_ip,
                                    table,
                                    "failed to remove orphaned ip rule for {src_ip}, table={table}: {e}",
                                    src_ip = src_ip,
                                    table = table
                                );
                                break;
                            }
                            orphan_count += 1;
                        }
                    } else if *count > 1 {
                        // Active IP with duplicates: keep one, remove the rest.
                        let extras = count - 1;
                        tracing::warn!(
                            src_ip = %src_ip,
                            table,
                            count,
                            extras,
                            "pruning duplicate ip rules for active device: src_ip={src_ip}, table={table}, count={count}, extras={extras}",
                            src_ip = src_ip,
                            table = table,
                            count = count,
                            extras = extras
                        );
                        for _ in 0..extras {
                            if let Err(e) = self.netlink.remove_ip_rule(src_ip, *table).await {
                                tracing::warn!(
                                    error = %e,
                                    src_ip = %src_ip,
                                    table,
                                    "failed to remove duplicate ip rule for {src_ip}, table={table}: {e}",
                                    src_ip = src_ip,
                                    table = table
                                );
                                break;
                            }
                            duplicate_count += 1;
                        }
                    }
                }
                if orphan_count > 0 {
                    tracing::info!(
                        orphan_count,
                        "cleaned up orphaned ip rules: orphan_count={orphan_count}",
                        orphan_count = orphan_count
                    );
                } else {
                    tracing::debug!("no orphaned ip rules found");
                }
                if duplicate_count > 0 {
                    tracing::info!(
                        duplicate_count,
                        "pruned duplicate ip rules: duplicate_count={duplicate_count}",
                        duplicate_count = duplicate_count
                    );
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

    async fn handle_route_table_lost(&self, table: u32) -> Result<(), AppError> {
        auth_context::require_admin()?;
        tracing::warn!(table, "handling lost route table");

        let mut state = self.state.lock().await;

        // Mark the table as unconfigured so ensure_tunnel_table re-adds it.
        state.tunnel_tables.remove(&table);

        // Find all devices whose traffic was routed through this table.
        let affected: Vec<(Uuid, String)> = state
            .applied
            .iter()
            .filter(|(_, rule)| rule.table == Some(table))
            .map(|(id, rule)| (*id, rule.device_ip.clone()))
            .collect();

        if affected.is_empty() {
            tracing::debug!(table, "no devices using lost route table");
            return Ok(());
        }

        // Collect targets then remove from applied so apply_rule doesn't
        // skip them via its idempotency check (same target+IP = no-op).
        let re_apply: Vec<(Uuid, String, RoutingTarget)> = affected
            .iter()
            .filter_map(|(id, _)| {
                state
                    .applied
                    .get(id)
                    .map(|r| (*id, r.device_ip.clone(), r.target.clone()))
            })
            .collect();
        for (id, _) in &affected {
            state.applied.remove(id);
        }

        drop(state);

        tracing::info!(
            table,
            device_count = re_apply.len(),
            "re-applying routing rules for devices on lost table"
        );

        for (device_id, device_ip, target) in &re_apply {
            if let Err(e) = self.apply_rule(*device_id, device_ip, target).await {
                tracing::warn!(
                    error = %e,
                    device_id = %device_id,
                    table,
                    "failed to re-apply routing rule after route table lost"
                );
            }
        }

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

    async fn apply_rule_for_device(
        &self,
        device_id: Uuid,
        target: &RoutingTarget,
    ) -> Result<(), AppError> {
        auth_context::require_admin()?;
        match self.devices.find_by_id(&device_id.to_string()).await {
            Ok(Some(device)) => {
                if let Err(e) = self.apply_rule(device_id, &device.last_ip, target).await {
                    tracing::warn!(
                        error = %e,
                        device_id = %device_id,
                        "failed to apply routing rule for device {device_id}: {e}"
                    );
                }
            }
            Ok(None) => {
                tracing::warn!(device_id = %device_id, "device not found for routing rule change");
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    device_id = %device_id,
                    "failed to look up device for routing rule change: {e}"
                );
            }
        }
        Ok(())
    }

    async fn apply_rule_for_discovered_device(
        &self,
        device_id: Uuid,
        ip: &str,
    ) -> Result<(), AppError> {
        auth_context::require_admin()?;
        match self
            .devices
            .find_rule_for_device(&device_id.to_string())
            .await
        {
            Ok(Some(rule)) => {
                if let Err(e) = self.apply_rule(device_id, ip, &rule.target).await {
                    tracing::warn!(
                        error = %e,
                        device_id = %device_id,
                        "failed to apply rule for newly discovered device {device_id}: {e}"
                    );
                }
            }
            Ok(None) => {
                // No routing rule for this device — nothing to do.
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    device_id = %device_id,
                    "failed to look up routing rule for discovered device {device_id}: {e}"
                );
            }
        }
        Ok(())
    }

    async fn set_default_policy(&self, policy: &str) -> Result<(), AppError> {
        auth_context::require_admin()?;

        if policy != "direct" && policy.parse::<Uuid>().is_err() {
            return Err(AppError::BadRequest(format!(
                "default_policy must be \"direct\" or a tunnel UUID, got {policy}"
            )));
        }

        self.system_config
            .set_default_policy(policy)
            .await
            .map_err(AppError::Internal)?;

        match self.default_policy.write() {
            Ok(mut guard) => policy.clone_into(&mut guard),
            Err(e) => {
                tracing::error!(error = %e, "default_policy lock poisoned during write");
                return Err(AppError::Internal(anyhow::anyhow!(
                    "default_policy lock poisoned"
                )));
            }
        }

        tracing::info!(policy, "default routing policy updated");

        // Announce the change so the Network-Zone enforcer (#736) can
        // re-validate `Default`-ruled devices against their zones and unbind
        // any tunnel binding a device's zone now forbids. Published *before*
        // the re-apply sweep below so the enforcer never observes a stale
        // policy; the enforcer only reads zones + devices, so ordering against
        // the re-apply is benign.
        self.events.publish(WardnetEvent::DefaultPolicyChanged {
            policy: policy.to_owned(),
            timestamp: chrono::Utc::now(),
        });

        // Re-apply every device whose *stored DB rule* is
        // RoutingTarget::Default. The cached `applied` entry holds the
        // already-resolved target (e.g. Direct) which apply_rule's
        // phase-1 short-circuit compares against — without this walk,
        // already-routed devices keep flowing through the *previous*
        // policy until something else (IP change, tunnel up/down)
        // triggers a re-apply. The policy switch is supposed to take
        // effect immediately, not "next time something happens to the
        // device", so iterate now.
        //
        // Errors per device are logged and swallowed — one device
        // failing to re-route shouldn't abort the policy change for
        // the rest. apply_rule already falls back to direct on its
        // own internal failures.
        let devices = self.devices.find_all().await.map_err(AppError::Internal)?;
        let mut reapplied = 0u32;
        for device in &devices {
            let rule = match self
                .devices
                .find_rule_for_device(&device.id.to_string())
                .await
            {
                Ok(Some(rule)) => rule,
                Ok(None) => continue,
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        device_id = %device.id,
                        "failed to load routing rule while re-applying default policy"
                    );
                    continue;
                }
            };
            if !matches!(rule.target, RoutingTarget::Default) {
                continue;
            }
            if let Err(e) = self
                .apply_rule(device.id, &device.last_ip, &RoutingTarget::Default)
                .await
            {
                tracing::warn!(
                    error = %e,
                    device_id = %device.id,
                    "failed to re-apply default policy for device"
                );
            } else {
                reapplied += 1;
            }
        }
        tracing::info!(
            reapplied,
            total_devices = devices.len(),
            "re-applied default routing policy across devices"
        );

        Ok(())
    }

    async fn default_policy(&self) -> Result<String, AppError> {
        auth_context::require_admin()?;
        Ok(self.current_default_policy())
    }

    fn dns_upstream_snapshot(&self) -> Arc<ArcSwap<HashMap<IpAddr, UpstreamId>>> {
        Arc::clone(&self.dns_upstream_snapshot)
    }

    async fn rebuild_dns_upstream_snapshot(&self) -> Result<(), AppError> {
        // No auth guard — invoked from the routing listener on
        // `TunnelDnsOverrideChanged`, which already runs inside an
        // `auth_context::with_context(Admin)` wrapper.
        let mut state = self.state.lock().await;

        // Re-fetch the override flag for every tunnel currently referenced
        // by an applied rule. Caching by tunnel id avoids repeating the
        // repo lookup when multiple devices share a tunnel.
        let mut override_cache: HashMap<Uuid, bool> = HashMap::new();
        for rule in state.applied.values_mut() {
            let Some(tunnel_id) = rule.tunnel_id else {
                rule.dns_upstream = UpstreamId::Default;
                continue;
            };
            let active = if let Some(v) = override_cache.get(&tunnel_id) {
                *v
            } else {
                let v = match self
                    .tunnel_repo
                    .find_config_by_id(&tunnel_id.to_string())
                    .await
                {
                    Ok(Some(cfg)) => cfg.override_default_dns && !cfg.dns.is_empty(),
                    Ok(None) => false,
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            tunnel_id = %tunnel_id,
                            "failed to load tunnel config during DNS upstream rebuild"
                        );
                        false
                    }
                };
                override_cache.insert(tunnel_id, v);
                v
            };
            rule.dns_upstream = if active {
                UpstreamId::Tunnel(tunnel_id)
            } else {
                UpstreamId::Default
            };
        }

        self.refresh_dns_upstream_snapshot(&state);
        Ok(())
    }
}
