use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use arc_swap::ArcSwap;
use async_trait::async_trait;
use tokio::sync::Mutex;
use tokio::time::Instant;
use uuid::Uuid;
use wardnet_common::device::Device;
use wardnet_common::dns::UpstreamId;
use wardnet_common::event::WardnetEvent;
use wardnet_common::network_zone::{AllowedTargetKind, NetworkZone};
use wardnet_common::routing::{RoutingRule, RoutingTarget};
use wardnet_common::routing_profile::DomainRoutingTarget;
use wardnet_common::tunnel::TunnelStatus;

use crate::TunnelService;
use crate::auth_context;
use crate::error::AppError;
use crate::event::EventPublisher;
use crate::routing::firewall::FirewallManager;
use crate::routing::policy_router::PolicyRouter;
use wardnetd_data::repository::{
    DeviceRepository, NetworkZoneRepository, SystemConfigRepository, TunnelRepository,
};

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
    /// Admin-gated like every other method — the impl calls
    /// `auth_context::require_admin()?` first. The listener drives it under
    /// `auth_context::with_context(AuthContext::system())` so that guard is
    /// satisfied even though the call originates outside the HTTP request path.
    async fn apply_rule_for_device(
        &self,
        device_id: Uuid,
        target: &RoutingTarget,
    ) -> Result<(), AppError>;

    /// Check if a newly-discovered device has a persisted routing rule and apply it.
    ///
    /// Used by the routing listener to handle `DeviceDiscovered` events.
    /// Admin-gated like every other method — the impl calls
    /// `auth_context::require_admin()?` first. The listener drives it under
    /// `auth_context::with_context(AuthContext::system())` so that guard is
    /// satisfied even though the call originates outside the HTTP request path.
    async fn apply_rule_for_discovered_device(
        &self,
        device_id: Uuid,
        ip: &str,
    ) -> Result<(), AppError>;

    /// Set the desired cross-zone switchback targets for a device.
    ///
    /// The zone enforcer calls this (from `reconcile_isolation`) with the CIDRs a
    /// device must reach directly across zones (its casting-exception targets).
    /// The targets are stored per device and materialized into kernel
    /// `ip rule ... lookup main priority` rules (at
    /// [`SWITCHBACK_RULE_PRIORITY`]) **only while the device is
    /// tunnel-bound** — a tunnel-bound device otherwise sends all its traffic
    /// (including cross-zone LAN) up its per-tunnel table, so the cast packet
    /// never reaches the forward chain where the zone allow-rules live. A direct
    /// or unmanaged device stores the targets but installs no rules (the `main`
    /// table already delivers LAN traffic locally).
    ///
    /// Passing an empty `target_cidrs` clears the device's carve-outs. Idempotent.
    ///
    /// No auth guard beyond `require_admin` (matches
    /// [`apply_rule_for_device`](Self::apply_rule_for_device)) — this is an
    /// internal service-to-service callback, not an API surface; callers run it
    /// inside an established admin `auth_context`.
    async fn set_switchback_targets(
        &self,
        device_id: Uuid,
        device_ip: String,
        target_cidrs: Vec<String>,
    ) -> Result<(), AppError>;

    /// Pin a device's traffic to a set of resolved destination IPs, as decided
    /// by the device's routing profiles when the local DNS server answered a
    /// matched domain.
    ///
    /// For each IPv4 address, installs `ip rule from <device_ip> to <ip>/32
    /// lookup <table> priority [``DOMAIN_ROUTE_RULE_PRIORITY``]`, where `table` is
    /// the target tunnel's table (routing the domain through that tunnel) or
    /// `main` (a `direct` carve-out). The lease expires `ttl_secs` after
    /// installation (clamped), and is renewed on each subsequent resolution.
    /// Re-resolving to the same target only extends the lease; a changed target
    /// swaps the rule. A tunnel target ensures the tunnel's table exists first.
    ///
    /// No auth guard — invoked from the domain-route runner, which runs inside
    /// an established admin `auth_context`.
    async fn route_resolved_domain(
        &self,
        device_ip: &str,
        resolved_ips: &[IpAddr],
        target: &DomainRoutingTarget,
        ttl_secs: u32,
    ) -> Result<(), AppError>;

    /// Remove expired domain-route leases from the kernel and prune any
    /// orphaned rules at [`DOMAIN_ROUTE_RULE_PRIORITY`] with no tracked lease
    /// (e.g. left behind by a crash). Driven on a timer by the domain-route
    /// runner. No auth guard — internal maintenance, run inside an admin
    /// `auth_context`.
    async fn gc_domain_routes(&self) -> Result<(), AppError>;

    /// Update the global default routing policy.
    ///
    /// Validates `policy` (must be `"direct"` or a tunnel UUID),
    /// persists it to `system_config`, updates the in-memory state used
    /// by [`Self::apply_rule`] to resolve [`RoutingTarget::Default`],
    /// and publishes `DefaultPolicyChanged`. Already-routed
    /// `Default`-ruled devices are re-applied by the routing listener
    /// reacting to that event (via
    /// [`Self::handle_default_policy_changed`]), not inline — new
    /// resolves see the new policy immediately, existing kernel state
    /// follows a moment later.
    async fn set_default_policy(&self, policy: &str) -> Result<(), AppError>;

    /// Read the current global default routing policy.
    async fn default_policy(&self) -> Result<String, AppError>;

    /// Re-read the persisted default policy, sync the in-memory copy,
    /// and re-apply every `Default`-ruled device against it.
    ///
    /// This is the single mechanism that makes a policy change take
    /// effect on already-routed devices. [`Self::set_default_policy`] is
    /// one writer; tunnel deletion is the other (a tunnel that *is* the
    /// default policy is reset to `"direct"` so the config never
    /// dangles). Both persist first and then announce via
    /// [`wardnet_common::event::WardnetEvent::DefaultPolicyChanged`];
    /// the routing listener forwards every such event here, and also
    /// calls this after a lagged event-bus receiver may have dropped
    /// one. The persisted value is re-read rather than trusted from the
    /// event payload so out-of-order or dropped events always converge
    /// on the database's state.
    ///
    /// No auth guard — callers wrap this in `auth_context::with_context(...)`.
    async fn handle_default_policy_changed(&self) -> Result<(), AppError>;

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

    /// Lock-free, atomically swappable snapshot of `device_id → UpstreamId`
    /// for the DNS server's per-query upstream selection of
    /// device-authenticated clients (the `DoT` listener's SNI-token
    /// attribution, issue #923).
    ///
    /// Unlike [`Self::dns_upstream_snapshot`], which reflects *applied*
    /// kernel state and therefore empties for a device the moment it
    /// leaves the LAN (`DeviceGone` → `remove_device_routes`), this map is
    /// derived from the **persisted** routing rules — a roaming device
    /// keeps its entry, so its `DoT` queries follow its tunnel binding
    /// even though the transport peer is the relay's loopback. Entries
    /// map a device to `Tunnel(id)` only when its rule (with `Default`
    /// resolved through the global policy) targets a tunnel that is not
    /// `Down` and has `override_default_dns` with at least one DNS
    /// server — and, for `Default` rules, only when the device's zone
    /// permits tunnel egress (mirroring the Network-Zone enforcer's
    /// clamp, which rewrites applied state but never the persisted rule).
    /// Lookup misses implicitly resolve to [`UpstreamId::Default`] — the
    /// same soft fallback the LAN path takes when `handle_tunnel_down`
    /// drops its applied rules.
    fn dns_device_upstream_snapshot(&self) -> Arc<ArcSwap<HashMap<Uuid, UpstreamId>>>;

    /// Force a rebuild + atomic swap of the snapshot returned by
    /// [`Self::dns_device_upstream_snapshot`]. Driven by the daemon's
    /// `DnsDeviceSnapshotListener`, which subscribes to the event bus and
    /// coalesces bursts of relevant events (rule changes, policy flips,
    /// tunnel status transitions, DNS-override toggles, zone changes)
    /// into single rebuilds; `reconcile` also runs it once at startup for
    /// the initial build. Rebuilds are internally serialised so a slow
    /// rebuild can never overwrite a fresher one.
    ///
    /// Admin-gated like every other method — callers outside the HTTP
    /// request path drive it under
    /// `auth_context::with_context(AuthContext::system())`.
    async fn rebuild_dns_device_upstream_snapshot(&self) -> Result<(), AppError>;
}

/// Priority of the cross-zone switchback carve-out `ip rule`s.
///
/// Must be numerically LOWER (evaluated earlier) than the kernel's per-tunnel
/// source rules — which the `rule().add()` builder lets the kernel auto-assign
/// around 32764/32765 — so a carve-out to a cross-zone LAN destination wins over
/// the `from <device_ip> lookup <tunnelTable>` rule. It is higher than the
/// `local` table rule (priority 0), so on-box delivery is unaffected.
pub const SWITCHBACK_RULE_PRIORITY: u32 = 1000;

/// Priority of the per-domain routing carve-out `ip rule`s.
///
/// Sits in its own band, numerically higher than [`SWITCHBACK_RULE_PRIORITY`]
/// (so a switchback carve-out still wins for its narrow cross-zone pair) yet far
/// below the kernel's per-tunnel source rules (~32764), so a `from <device_ip>
/// to <resolved_ip> lookup <table>` decision wins over the device's own
/// `from <device_ip> lookup <deviceTable>` rule for that one destination.
pub const DOMAIN_ROUTE_RULE_PRIORITY: u32 = 2000;

/// The kernel `main` routing table id (254), used as the target table for a
/// `direct` domain rule — carving a domain out of the device's tunnel back to
/// the WAN.
const RT_TABLE_MAIN: u32 = 254;

/// TTL clamp for a domain-route lease. A record's own TTL drives expiry, floored
/// so a 0-TTL CDN answer cannot thrash the kernel and capped so a very long TTL
/// cannot pin a stale IP indefinitely.
const MIN_DOMAIN_ROUTE_TTL_SECS: u64 = 30;
const MAX_DOMAIN_ROUTE_TTL_SECS: u64 = 3600;

/// A per-destination domain-route lease: the routing table the destination is
/// pinned to, and when the lease expires (from the DNS record's TTL).
struct DomainRouteEntry {
    table: u32,
    expiry: Instant,
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
    /// Desired cross-zone switchback targets per device: `device_id → (device_ip,
    /// sorted target CIDRs)`. Pushed by the zone enforcer via
    /// [`RoutingService::set_switchback_targets`]. Materialized into kernel
    /// `ip rule`s only while the device is tunnel-bound.
    switchback_targets: HashMap<Uuid, (String, Vec<String>)>,
    /// Carve-out CIDRs currently installed in the kernel per device, so the diff
    /// on the next reconcile knows what to add/remove. Present only for
    /// tunnel-bound devices with a non-empty desired set.
    /// Carve-outs believed to be in the kernel, as `(device_ip, dst_cidrs)`.
    ///
    /// Only CIDRs whose `add_switchback_rule` returned `Ok` are recorded, and
    /// the IP they were installed under travels with them. Both matter for the
    /// diff to be a truthful picture of the kernel: an add that failed must
    /// stay absent so the next reconcile retries it, and an entry installed
    /// under a stale IP must be rebuilt rather than counted as present.
    applied_switchback: HashMap<Uuid, (String, Vec<String>)>,
    /// Per-destination domain-route leases, keyed by `(device_ip, dst_ip)`.
    /// Installed by [`RoutingService::route_resolved_domain`] as the DNS server
    /// resolves matched domains, and expired by
    /// [`RoutingService::gc_domain_routes`] once their TTL lapses.
    domain_routes: HashMap<(String, String), DomainRouteEntry>,
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
    /// `device_id → UpstreamId` snapshot consulted lock-free by the DNS
    /// server for device-authenticated (`DoT`) clients (#923). Derived
    /// from the persisted routing rules, not from applied kernel state,
    /// so roaming devices keep their entries. See
    /// [`RoutingService::dns_device_upstream_snapshot`].
    dns_device_upstream_snapshot: Arc<ArcSwap<HashMap<Uuid, UpstreamId>>>,
    /// Serialises [`RoutingService::rebuild_dns_device_upstream_snapshot`]:
    /// the rebuild is load-then-store across many awaits, so without this a
    /// slow rebuild started before a policy/tunnel change could store its
    /// stale map *after* a fresh rebuild stored the correct one — and no
    /// further event would correct it.
    device_snapshot_rebuild: tokio::sync::Mutex<()>,
    /// Zone repository, used by the device-keyed snapshot rebuild to mirror
    /// the Network-Zone enforcer's clamp of `Default` bindings (#736): the
    /// enforcer rewrites *applied* kernel state only, never the persisted
    /// rule, so the rebuild must apply the same clamp itself or it would
    /// resolve a clamped device's rule straight through the forbidden
    /// policy.
    zones: Arc<dyn NetworkZoneRepository>,
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
        zones: Arc<dyn NetworkZoneRepository>,
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
                switchback_targets: HashMap::new(),
                applied_switchback: HashMap::new(),
                domain_routes: HashMap::new(),
            }),
            dns_upstream_snapshot: Arc::new(ArcSwap::from_pointee(HashMap::new())),
            dns_device_upstream_snapshot: Arc::new(ArcSwap::from_pointee(HashMap::new())),
            device_snapshot_rebuild: tokio::sync::Mutex::new(()),
            zones,
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

    /// Best-effort wrapper around
    /// [`RoutingService::rebuild_dns_device_upstream_snapshot`] for
    /// `reconcile`'s initial build — a failed build there must not abort
    /// startup; the daemon's snapshot listener converges the map on the
    /// next relevant event.
    async fn refresh_dns_device_upstream_snapshot(&self) {
        if let Err(e) = self.rebuild_dns_device_upstream_snapshot().await {
            tracing::warn!(
                error = %e,
                "failed to rebuild device-keyed DNS upstream snapshot: {e}"
            );
        }
    }

    /// The config half of the tunnel DNS-override gate, shared by
    /// `apply_rule` and both snapshot rebuilds so the predicate cannot
    /// drift between the applied-state and persisted-rule maps: does
    /// `tunnel_id`'s stored config mark it as a DNS upstream
    /// (`override_default_dns` with at least one DNS server)?
    /// `Some(false)` for a missing config (a definite "no"); `None` when
    /// the lookup failed — the caller decides how to degrade, because
    /// "unknown" and "off" must not be conflated by the device-keyed
    /// rebuild (a transient repo error would otherwise silently drop
    /// every device on the tunnel to the default upstream).
    async fn tunnel_dns_override_active(&self, tunnel_id: Uuid) -> Option<bool> {
        match self
            .tunnel_repo
            .find_config_by_id(&tunnel_id.to_string())
            .await
        {
            Ok(Some(cfg)) => Some(cfg.override_default_dns && !cfg.dns.is_empty()),
            Ok(None) => Some(false),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    tunnel_id = %tunnel_id,
                    "failed to load tunnel config for DNS override gate of tunnel {tunnel_id}: {e}"
                );
                None
            }
        }
    }

    /// Whether `tunnel_id` currently qualifies as a DNS upstream for the
    /// devices bound to it: the tunnel must exist, must not be `Down`
    /// (its interface is expected present — `Connecting`/`Reconnecting`
    /// keep the iface configured, matching the routing listener's
    /// treatment), and must pass [`Self::tunnel_dns_override_active`]. A
    /// `Down` tunnel drops its devices to the default upstream — the same
    /// soft fallback the LAN path takes when `handle_tunnel_down` removes
    /// applied rules. When per-device kill-switch mode (#235) lands, a
    /// block-mode device must instead keep failing here rather than fall
    /// back.
    ///
    /// Status is read through the *repository*, not `TunnelService` —
    /// the service getter is admin-gated, and a swallowed auth error here
    /// would masquerade as "tunnel inactive" and wipe the map.
    ///
    /// `Some(active)` is a definite answer; `None` means the status
    /// lookup failed and the caller must not treat the tunnel as down.
    async fn tunnel_dns_upstream_gate(&self, tunnel_id: Uuid) -> Option<bool> {
        match self.tunnel_repo.find_by_id(&tunnel_id.to_string()).await {
            Ok(Some(tunnel)) if tunnel.status == TunnelStatus::Down => return Some(false),
            Ok(Some(_)) => {}
            // Deleted tunnel still referenced by a rule — definitely inactive.
            Ok(None) => return Some(false),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    tunnel_id = %tunnel_id,
                    "failed to load tunnel {tunnel_id} for device DNS upstream rebuild: {e}"
                );
                return None;
            }
        }
        self.tunnel_dns_override_active(tunnel_id).await
    }

    /// Mirror of the Network-Zone enforcer's clamp of `Default` bindings
    /// (`ZoneEnforcementServiceImpl::clamp_default_bindings`, #736): true
    /// when the device's zone forbids the tunnel kind but permits direct.
    /// The enforcer clamps such a device's *applied* routing to direct
    /// without rewriting its persisted `Default` rule, so the
    /// persisted-rule rebuild must apply the same clamp or it would hand
    /// the device's roaming DNS to the forbidden tunnel. A zone that
    /// forbids both kinds does NOT clamp (matching the enforcer, which
    /// leaves that binding for the packet-layer drop — applied state
    /// stays tunnel-bound, and this map stays in step with it). A device
    /// with no row or an unresolvable zone is not clamped, again matching
    /// the enforcer's skip.
    async fn zone_clamps_default_to_direct(
        &self,
        zone_cache: &mut HashMap<Uuid, Option<NetworkZone>>,
        zone_id: Option<Uuid>,
    ) -> bool {
        let Some(zone_id) = zone_id else {
            return false;
        };
        let zone = if let Some(z) = zone_cache.get(&zone_id) {
            z.clone()
        } else {
            let z = self
                .zones
                .find_by_id(&zone_id.to_string())
                .await
                .ok()
                .flatten();
            zone_cache.insert(zone_id, z.clone());
            z
        };
        let Some(zone) = zone else {
            return false;
        };
        !zone.permits_kind(AllowedTargetKind::Tunnel)
            && zone.permits_kind(AllowedTargetKind::Direct)
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

    /// Write `policy` into the in-memory default-policy cache, returning
    /// whether the stored value actually changed. Shared by
    /// [`RoutingService::set_default_policy`] and
    /// [`RoutingService::handle_default_policy_changed`] so the
    /// poisoned-lock handling cannot drift between the two write paths
    /// (the read side is likewise centralized in
    /// [`Self::current_default_policy`]).
    fn write_cached_policy(&self, policy: &str) -> Result<bool, AppError> {
        match self.default_policy.write() {
            Ok(mut guard) => {
                if *guard == policy {
                    Ok(false)
                } else {
                    policy.clone_into(&mut guard);
                    Ok(true)
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "default_policy lock poisoned during write");
                Err(AppError::Internal(anyhow::anyhow!(
                    "default_policy lock poisoned"
                )))
            }
        }
    }

    /// Re-apply every device whose *stored DB rule* is
    /// `RoutingTarget::Default`. The cached `applied` entry holds the
    /// already-resolved target (e.g. Direct) which `apply_rule`'s
    /// phase-1 short-circuit compares against — without this walk,
    /// already-routed devices keep flowing through the *previous*
    /// policy until something else (IP change, tunnel up/down)
    /// triggers a re-apply. A policy switch is supposed to take
    /// effect immediately, not "next time something happens to the
    /// device", so iterate now.
    ///
    /// Errors per device are logged and swallowed — one device
    /// failing to re-route shouldn't abort the policy change for
    /// the rest. `apply_rule` already falls back to direct on its
    /// own internal failures.
    async fn reapply_default_ruled_devices(&self) -> Result<(), AppError> {
        // Two batched queries instead of a per-device rule lookup: the
        // rule set is loaded once and joined against the device list in
        // memory.
        let devices = self.devices.find_all().await.map_err(AppError::Internal)?;
        let default_ruled: HashSet<Uuid> = self
            .devices
            .find_all_rules()
            .await
            .map_err(AppError::Internal)?
            .into_iter()
            .filter(|rule| matches!(rule.target, RoutingTarget::Default))
            .map(|rule| rule.device_id)
            .collect();
        let mut reapplied = 0u32;
        for device in &devices {
            if !default_ruled.contains(&device.id) {
                continue;
            }
            if let Err(e) = self
                .apply_rule(device.id, &device.last_ip, &RoutingTarget::Default)
                .await
            {
                let device_id = device.id;
                tracing::warn!(
                    error = %e,
                    device_id = %device_id,
                    "failed to re-apply default policy for device {device_id}: {e}"
                );
            } else {
                reapplied += 1;
            }
        }
        let total_devices = devices.len();
        tracing::info!(
            reapplied,
            total_devices,
            "re-applied default routing policy across devices: {reapplied}/{total_devices}"
        );

        Ok(())
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
    #[allow(clippy::similar_names)]
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
            // Drop any cross-zone switchback carve-outs installed under this
            // device's IP: the binding is being torn down (going direct, tunnel
            // down, IP change, or removal), so the tunnel-capture problem the
            // carve-outs work around no longer applies. The desired target set
            // is left intact so a later re-bind re-materializes them.
            let device_ip = rule.device_ip.clone();
            self.remove_switchback_for_device(state, device_id, &device_ip)
                .await;
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

    /// The `/32` CIDR for a device's own IP, used to skip a self-referential
    /// switchback carve-out target.
    fn self_cidr(device_ip: &str) -> String {
        format!("{device_ip}/32")
    }

    /// Reconcile the installed switchback carve-outs for one device against its
    /// stored desired target set.
    ///
    /// Carve-outs are materialized only while the device is **tunnel-bound**
    /// (its [`AppliedRule::table`] is `Some`); a direct/unmanaged device keeps
    /// the stored targets but holds no kernel rules. Diffs against
    /// [`RoutingState::applied_switchback`] so the operation is idempotent.
    /// Errors are warn-logged, never fatal. Caller holds the state lock.
    #[allow(clippy::similar_names)]
    async fn reconcile_switchback_for_device(&self, state: &mut RoutingState, device_id: Uuid) {
        let Some((device_ip, desired)) = state.switchback_targets.get(&device_id).cloned() else {
            // No desired targets — ensure nothing is installed.
            let ip = state
                .applied
                .get(&device_id)
                .map(|r| r.device_ip.clone())
                .unwrap_or_default();
            self.remove_switchback_for_device(state, device_id, &ip)
                .await;
            return;
        };

        let tunnel_bound = state
            .applied
            .get(&device_id)
            .is_some_and(|r| r.table.is_some());

        // Desired kernel carve-out set: empty unless tunnel-bound; never a
        // self-referential `/32`.
        let desired_set: Vec<String> = if tunnel_bound {
            let own = Self::self_cidr(&device_ip);
            desired.into_iter().filter(|c| *c != own).collect()
        } else {
            Vec::new()
        };

        // A carve-out is only "already present" if it was installed under the
        // IP in play. When the device's IP has moved, tear the old rules down
        // at their own IP first — `remove_switchback_rule` matches on source,
        // so removing them at the new IP would miss, leaking a rule that
        // outlives the address it was written for.
        let applied = match state.applied_switchback.get(&device_id) {
            Some((ip, cidrs)) if *ip == device_ip => cidrs.clone(),
            Some(_) => {
                self.remove_switchback_for_device(state, device_id, "")
                    .await;
                Vec::new()
            }
            None => Vec::new(),
        };

        // Remove stale carve-outs.
        for cidr in &applied {
            if !desired_set.contains(cidr)
                && let Err(e) = self
                    .netlink
                    .remove_switchback_rule(&device_ip, cidr, SWITCHBACK_RULE_PRIORITY)
                    .await
            {
                tracing::warn!(
                    error = %e,
                    device_id = %device_id,
                    device_ip = %device_ip,
                    dst_cidr = %cidr,
                    "failed to remove stale switchback carve-out"
                );
            }
        }

        // Add newly-desired carve-outs, tracking which ones the kernel actually
        // accepted. A device whose DHCP lease has not landed yet reaches here
        // with an empty `device_ip` that netlink rejects; recording that CIDR
        // anyway would make every later reconcile diff it away as already
        // present, and the carve-out would never be installed.
        let mut installed: Vec<String> = Vec::new();
        for cidr in &desired_set {
            if applied.contains(cidr) {
                installed.push(cidr.clone());
                continue;
            }
            match self
                .netlink
                .add_switchback_rule(&device_ip, cidr, SWITCHBACK_RULE_PRIORITY)
                .await
            {
                Ok(()) => installed.push(cidr.clone()),
                Err(e) => tracing::warn!(
                    error = %e,
                    device_id = %device_id,
                    device_ip = %device_ip,
                    dst_cidr = %cidr,
                    "failed to add switchback carve-out"
                ),
            }
        }

        if installed.is_empty() {
            state.applied_switchback.remove(&device_id);
        } else {
            tracing::debug!(
                device_id = %device_id,
                device_ip = %device_ip,
                carve_outs = installed.len(),
                "materialized switchback carve-outs for tunnel-bound device"
            );
            state
                .applied_switchback
                .insert(device_id, (device_ip, installed));
        }
    }

    /// Remove ALL installed switchback carve-outs for a device and drop the
    /// applied-tracking entry. Leaves the stored desired target set untouched.
    /// Caller holds the state lock.
    ///
    /// Each rule is removed at the IP it was installed under, which is the only
    /// IP that can match it. `fallback_ip` is used when nothing was recorded —
    /// pass `""` when the caller has no better IP than the record itself.
    #[allow(clippy::similar_names)]
    async fn remove_switchback_for_device(
        &self,
        state: &mut RoutingState,
        device_id: Uuid,
        fallback_ip: &str,
    ) {
        let Some((installed_ip, applied)) = state.applied_switchback.remove(&device_id) else {
            return;
        };
        let device_ip = if installed_ip.is_empty() {
            fallback_ip
        } else {
            installed_ip.as_str()
        };
        for cidr in &applied {
            if let Err(e) = self
                .netlink
                .remove_switchback_rule(device_ip, cidr, SWITCHBACK_RULE_PRIORITY)
                .await
            {
                tracing::warn!(
                    error = %e,
                    device_id = %device_id,
                    device_ip = %device_ip,
                    dst_cidr = %cidr,
                    "failed to remove switchback carve-out during teardown"
                );
            }
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
    interface_name
        .strip_prefix(crate::tunnel::interface::TUNNEL_INTERFACE_PREFIX)?
        .parse()
        .ok()
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
        // A failed config lookup degrades to "off" here: the applied-state
        // map is per-device and self-corrects on the next apply.
        let tunnel_dns_override = if let Some(ref tunnel) = tunnel_info {
            self.tunnel_dns_override_active(tunnel.id)
                .await
                .unwrap_or(false)
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

        // (Re)materialize this device's cross-zone switchback carve-outs against
        // the new binding: a tunnel-bound device installs them; a direct device
        // installs none. The stored desired target set is the source of truth.
        self.reconcile_switchback_for_device(&mut state, device_id)
            .await;

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
        // The device is gone — drop its desired switchback targets too so a
        // future device reusing the same UUID does not inherit stale carve-outs.
        state.switchback_targets.remove(&device_id);
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
            // Tears down carve-outs installed under the OLD IP.
            self.remove_device_kernel_state(&mut state, device_id).await;
            // Re-key the stored desired targets to the new IP so the re-apply
            // below materializes them under `new_ip`.
            if let Some((ip, _)) = state.switchback_targets.get_mut(&device_id) {
                new_ip.clone_into(ip);
            }
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
                "tunnel down - removing routing for affected devices"
            );
        }

        // Remove kernel state for each affected device, capturing IPs so we
        // can flush their conntrack entries after releasing the lock.
        let mut affected_ips: Vec<String> = Vec::with_capacity(affected.len());
        for device_id in &affected {
            tracing::warn!(
                device_id = %device_id,
                tunnel_id = %tunnel_id,
                "tunnel down - removing routing for device"
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
            "tunnel up - re-applying routing rules for devices"
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

        // Re-establish the Network-Zone base-chain jumps that the flush above may
        // have removed. This MUST run before `add_masquerade`: it restores the
        // POSTROUTING→zone_natexempt jump as rule #0 of the now-empty postrouting
        // chain, so the LAN masquerade appended next lands after it and NAT-exempt
        // cross-zone flows are accepted before masquerade can rewrite them.
        tracing::debug!("ensuring Network-Zone isolation jumps");
        self.nftables
            .ensure_isolation_jumps()
            .await
            .map_err(AppError::Internal)?;

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
            // Kernel ip rules were just flushed conceptually via re-derivation;
            // drop the applied-carve-out tracking so it is rebuilt from scratch.
            // The desired target set (owned by the zone enforcer) is preserved so
            // tunnel-bound devices re-materialize their carve-outs on re-apply.
            state.applied_switchback.clear();
            // Domain-route leases are rebuilt as the DNS server re-resolves
            // matched domains; drop the in-memory tracking here and prune the
            // surviving kernel rules just below.
            state.domain_routes.clear();
        }

        // The nftables flush above does not touch `ip rule`s, so domain-route
        // carve-outs survive a reconcile. With the in-memory tracking cleared,
        // every kernel rule at our priority is now an orphan — remove them so the
        // slate matches; they re-install on the next matched resolution.
        match self
            .netlink
            .list_domain_route_rules(DOMAIN_ROUTE_RULE_PRIORITY)
            .await
        {
            Ok(rules) => {
                for (src, dst, table) in rules {
                    if let Err(e) = self
                        .netlink
                        .remove_domain_route_rule(&src, &dst, table, DOMAIN_ROUTE_RULE_PRIORITY)
                        .await
                    {
                        tracing::warn!(error = %e, src, dst, "failed to prune domain-route rule during reconcile");
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to list domain-route rules during reconcile");
            }
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

        // Prune orphaned switchback carve-outs, then ensure the desired ones for
        // every tunnel-bound device exist. Any carve-out in the kernel at
        // `SWITCHBACK_RULE_PRIORITY` that is not in the union of desired,
        // currently-tunnel-bound device carve-outs is stale and removed.
        tracing::debug!("checking for orphaned switchback carve-outs");
        match self.netlink.list_switchback_rules().await {
            Ok(kernel_carveouts) => {
                let mut state = self.state.lock().await;

                // Build the desired (device_ip, cidr) set for tunnel-bound
                // devices only — the only devices that should carry carve-outs.
                let mut desired: HashSet<(String, String)> = HashSet::new();
                for (device_id, (ip, cidrs)) in &state.switchback_targets {
                    let tunnel_bound = state
                        .applied
                        .get(device_id)
                        .is_some_and(|r| r.table.is_some());
                    if !tunnel_bound {
                        continue;
                    }
                    let own = Self::self_cidr(ip);
                    for cidr in cidrs {
                        if *cidr != own {
                            desired.insert((ip.clone(), cidr.clone()));
                        }
                    }
                }

                let mut orphan_count = 0u32;
                for (src_ip, dst_cidr, priority) in &kernel_carveouts {
                    if *priority != SWITCHBACK_RULE_PRIORITY {
                        continue;
                    }
                    if !desired.contains(&(src_ip.clone(), dst_cidr.clone())) {
                        tracing::warn!(
                            src_ip = %src_ip,
                            dst_cidr = %dst_cidr,
                            priority,
                            "removing orphaned switchback carve-out"
                        );
                        if let Err(e) = self
                            .netlink
                            .remove_switchback_rule(src_ip, dst_cidr, *priority)
                            .await
                        {
                            tracing::warn!(
                                error = %e,
                                src_ip = %src_ip,
                                dst_cidr = %dst_cidr,
                                "failed to remove orphaned switchback carve-out"
                            );
                        } else {
                            orphan_count += 1;
                        }
                    }
                }
                if orphan_count > 0 {
                    tracing::info!(orphan_count, "cleaned up orphaned switchback carve-outs");
                }

                // Ensure desired carve-outs exist for every tunnel-bound device.
                let device_ids: Vec<Uuid> = state.switchback_targets.keys().copied().collect();
                for device_id in device_ids {
                    self.reconcile_switchback_for_device(&mut state, device_id)
                        .await;
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to list kernel switchback rules for orphan cleanup");
            }
        }

        // Initial build of the device-keyed DNS upstream snapshot (#923).
        // The per-IP snapshot was already refreshed by the apply_rule walk
        // above; this builds its roaming companion from the same persisted
        // rules so DoT-authenticated queries route correctly from startup.
        self.refresh_dns_device_upstream_snapshot().await;

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

    #[allow(clippy::similar_names)]
    async fn set_switchback_targets(
        &self,
        device_id: Uuid,
        device_ip: String,
        mut target_cidrs: Vec<String>,
    ) -> Result<(), AppError> {
        auth_context::require_admin()?;

        // Canonicalize the desired set: sorted + deduped so the stored form is
        // order-stable and the diff in `reconcile_switchback_for_device` is clean.
        target_cidrs.sort();
        target_cidrs.dedup();

        tracing::debug!(
            device_id = %device_id,
            device_ip = %device_ip,
            targets = target_cidrs.len(),
            "set_switchback_targets called"
        );

        let mut state = self.state.lock().await;

        if target_cidrs.is_empty() {
            // No desired targets — forget the device and tear down any carve-outs
            // currently installed under this IP.
            state.switchback_targets.remove(&device_id);
            self.remove_switchback_for_device(&mut state, device_id, &device_ip)
                .await;
        } else {
            state
                .switchback_targets
                .insert(device_id, (device_ip, target_cidrs));
            // Materialize now if (and only if) the device is tunnel-bound.
            self.reconcile_switchback_for_device(&mut state, device_id)
                .await;
        }

        Ok(())
    }

    async fn route_resolved_domain(
        &self,
        device_ip: &str,
        resolved_ips: &[IpAddr],
        target: &DomainRoutingTarget,
        ttl_secs: u32,
    ) -> Result<(), AppError> {
        // No auth guard — invoked from the domain-route runner inside an admin
        // `auth_context` (mirrors `handle_default_policy_changed`).

        // Resolve the destination routing table. A tunnel target brings the
        // tunnel up and ensures its table exists before any rule points at it,
        // exactly as `apply_rule` does for a per-device binding.
        // Resolve the destination routing table WITHOUT holding the `state`
        // lock. A tunnel target may require an on-demand bring-up, which can
        // take seconds; holding the serialize-everything `state` mutex across it
        // would stall every other routing operation (device rule changes, tunnel
        // up/down handling, GC). This mirrors `apply_rule`'s "Phase 2: tunnel
        // operations (no lock held)".
        let (table, tunnel_iface) = match target {
            DomainRoutingTarget::Direct => (RT_TABLE_MAIN, None),
            DomainRoutingTarget::Tunnel { tunnel_id } => {
                let tunnel = match self.tunnels.get_tunnel(*tunnel_id).await {
                    Ok(tunnel) => tunnel,
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            tunnel_id = %tunnel_id,
                            "domain route references an unavailable tunnel; skipping"
                        );
                        return Ok(());
                    }
                };
                if tunnel.status == TunnelStatus::Down
                    && let Err(e) = self.tunnels.bring_up_internal(*tunnel_id).await
                {
                    tracing::warn!(
                        error = %e,
                        tunnel_id = %tunnel_id,
                        "failed to bring up tunnel for domain route; skipping"
                    );
                    return Ok(());
                }
                let Some(index) = parse_interface_index(&tunnel.interface_name) else {
                    tracing::warn!(
                        interface = %tunnel.interface_name,
                        "could not parse interface index for domain route; skipping"
                    );
                    return Ok(());
                };
                (table_for_index(index), Some(tunnel.interface_name))
            }
        };

        // Take the lock only for the kernel-state mutations below.
        let mut state = self.state.lock().await;
        if let Some(interface_name) = &tunnel_iface
            && let Err(e) = self
                .ensure_tunnel_table(&mut state, interface_name, table)
                .await
        {
            tracing::warn!(
                error = %e,
                table,
                "failed to ensure tunnel table for domain route; skipping"
            );
            return Ok(());
        }

        let ttl = u64::from(ttl_secs).clamp(MIN_DOMAIN_ROUTE_TTL_SECS, MAX_DOMAIN_ROUTE_TTL_SECS);
        let expiry = Instant::now() + Duration::from_secs(ttl);

        for ip in resolved_ips {
            // v1 enforces IPv4 only — the `ip rule` primitive is v4.
            let IpAddr::V4(v4) = ip else { continue };
            let dst = v4.to_string();
            let key = (device_ip.to_owned(), dst.clone());
            match state.domain_routes.get(&key).map(|e| e.table) {
                Some(existing) if existing == table => {
                    // Same decision — just extend the lease.
                    if let Some(entry) = state.domain_routes.get_mut(&key) {
                        entry.expiry = expiry;
                    }
                }
                Some(existing) => {
                    // Target changed for this destination — swap the rule.
                    if let Err(e) = self
                        .netlink
                        .remove_domain_route_rule(
                            device_ip,
                            &dst,
                            existing,
                            DOMAIN_ROUTE_RULE_PRIORITY,
                        )
                        .await
                    {
                        tracing::warn!(error = %e, device_ip, dst, "failed to remove stale domain-route rule");
                    }
                    if let Err(e) = self
                        .netlink
                        .add_domain_route_rule(device_ip, &dst, table, DOMAIN_ROUTE_RULE_PRIORITY)
                        .await
                    {
                        tracing::warn!(error = %e, device_ip, dst, table, "failed to add domain-route rule");
                        state.domain_routes.remove(&key);
                        continue;
                    }
                    state
                        .domain_routes
                        .insert(key, DomainRouteEntry { table, expiry });
                }
                None => {
                    if let Err(e) = self
                        .netlink
                        .add_domain_route_rule(device_ip, &dst, table, DOMAIN_ROUTE_RULE_PRIORITY)
                        .await
                    {
                        tracing::warn!(error = %e, device_ip, dst, table, "failed to add domain-route rule");
                        continue;
                    }
                    state
                        .domain_routes
                        .insert(key, DomainRouteEntry { table, expiry });
                }
            }
        }

        Ok(())
    }

    async fn gc_domain_routes(&self) -> Result<(), AppError> {
        // No auth guard — periodic maintenance from the domain-route runner.

        // Phase 1 (short lock): expire lapsed leases, removing the map entry and
        // its kernel rule together so the "map entry present ⟺ kernel rule
        // present" invariant that `route_resolved_domain`'s lease-extend fast
        // path relies on is preserved. Snapshot the surviving keys for the
        // orphan sweep below.
        let tracked: HashSet<(String, String)> = {
            let mut state = self.state.lock().await;
            let now = Instant::now();
            let expired: Vec<(String, String, u32)> = state
                .domain_routes
                .iter()
                .filter(|(_, entry)| entry.expiry <= now)
                .map(|((src, dst), entry)| (src.clone(), dst.clone(), entry.table))
                .collect();
            for (src, dst, table) in expired {
                if let Err(e) = self
                    .netlink
                    .remove_domain_route_rule(&src, &dst, table, DOMAIN_ROUTE_RULE_PRIORITY)
                    .await
                {
                    tracing::warn!(error = %e, src, dst, "failed to remove expired domain-route rule");
                }
                state.domain_routes.remove(&(src, dst));
            }
            state.domain_routes.keys().cloned().collect()
        };

        // Phase 2 (no lock): dump the kernel rules at our priority. This is a
        // read-only netlink query, so it must NOT hold the routing `state`
        // mutex — keeping the periodic dump off the lock is what stops the GC
        // tick from stalling every other routing operation.
        let kernel = self
            .netlink
            .list_domain_route_rules(DOMAIN_ROUTE_RULE_PRIORITY)
            .await
            .map_err(AppError::Internal)?;
        let orphans: Vec<(String, String, u32)> = kernel
            .into_iter()
            .filter(|(src, dst, _)| !tracked.contains(&(src.clone(), dst.clone())))
            .collect();
        if orphans.is_empty() {
            return Ok(());
        }

        // Phase 3 (short lock): prune kernel rules with no tracked lease — e.g.
        // left behind by a crash before the lease was persisted. Re-check each
        // candidate against the live map under the lock so a lease inserted by a
        // concurrent `route_resolved_domain` between phases is not mistaken for
        // a crash orphan and removed out from under it.
        let state = self.state.lock().await;
        for (src, dst, table) in orphans {
            if !state
                .domain_routes
                .contains_key(&(src.clone(), dst.clone()))
                && let Err(e) = self
                    .netlink
                    .remove_domain_route_rule(&src, &dst, table, DOMAIN_ROUTE_RULE_PRIORITY)
                    .await
            {
                tracing::warn!(error = %e, src, dst, "failed to prune orphan domain-route rule");
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

        // Update the cache immediately so concurrent resolves and the
        // policy GET never observe the old value between the DB write and
        // the listener's reaction below.
        self.write_cached_policy(policy)?;

        tracing::info!(policy, "default routing policy updated to {policy}");

        // Announce the change. Two consumers react: the Network-Zone
        // enforcer (#736) re-validates `Default`-ruled devices against
        // their zones and unbinds any tunnel binding a device's zone now
        // forbids, and the routing listener re-applies `Default`-ruled
        // devices via `handle_default_policy_changed`. The re-apply is
        // event-driven rather than inline so this admin call returns
        // without waiting on the kernel walk, and so both policy writers
        // (this method and tunnel deletion) converge through one path.
        self.events.publish(WardnetEvent::DefaultPolicyChanged {
            policy: policy.to_owned(),
            timestamp: chrono::Utc::now(),
        });

        Ok(())
    }

    async fn default_policy(&self) -> Result<String, AppError> {
        auth_context::require_admin()?;
        Ok(self.current_default_policy())
    }

    async fn handle_default_policy_changed(&self) -> Result<(), AppError> {
        // No auth guard — invoked from the routing listener, which already
        // runs inside an `auth_context::with_context(Admin)` wrapper.
        //
        // The persisted policy is re-read here instead of trusting the
        // event payload: publishes are not atomic with their DB writes, so
        // a stale payload could otherwise overwrite a newer value. Reading
        // the source of truth makes the last handler run converge on the
        // final persisted state regardless of event ordering or drops.
        let Some(policy) = self
            .system_config
            .get_default_policy()
            .await
            .map_err(AppError::Internal)?
        else {
            // Policy key unset (pre-bootstrap) — nothing to sync against.
            return Ok(());
        };

        if self.write_cached_policy(&policy)? {
            tracing::info!(
                policy,
                "synced default routing policy from persisted change to {policy}"
            );
        }

        // Always sweep, even when the cache was already current: for
        // changes originating in `set_default_policy` the cache is updated
        // inline before the event is published, so an in-sync cache says
        // nothing about whether devices have been re-applied yet.
        // `apply_rule`'s short-circuit makes a redundant sweep cheap.
        // (The device-keyed DNS snapshot is NOT refreshed here — the
        // daemon's snapshot listener reacts to the same
        // `DefaultPolicyChanged` event independently, so a failure in this
        // kernel sweep cannot leave that map pinned to the old policy.)
        self.reapply_default_ruled_devices().await
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
        // repo lookup when multiple devices share a tunnel. A failed lookup
        // degrades to "off" — this map self-corrects on the next per-device
        // apply, unlike the device-keyed map (see
        // `tunnel_dns_override_active`).
        let mut override_cache: HashMap<Uuid, bool> = HashMap::new();
        for rule in state.applied.values_mut() {
            let Some(tunnel_id) = rule.tunnel_id else {
                rule.dns_upstream = UpstreamId::Default;
                continue;
            };
            let active = if let Some(v) = override_cache.get(&tunnel_id) {
                *v
            } else {
                let v = self
                    .tunnel_dns_override_active(tunnel_id)
                    .await
                    .unwrap_or(false);
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

    fn dns_device_upstream_snapshot(&self) -> Arc<ArcSwap<HashMap<Uuid, UpstreamId>>> {
        Arc::clone(&self.dns_device_upstream_snapshot)
    }

    async fn rebuild_dns_device_upstream_snapshot(&self) -> Result<(), AppError> {
        auth_context::require_admin()?;
        // Serialise rebuilds: the load-then-store below spans many awaits,
        // so without this a slow rebuild started before a policy/tunnel
        // change could store its stale map after a fresh rebuild stored the
        // correct one — and nothing would then correct it.
        let _rebuild = self.device_snapshot_rebuild.lock().await;

        // Built from the persisted rules rather than `RoutingState::applied`
        // deliberately: applied state is removed when a device leaves the LAN
        // (`DeviceGone` → `remove_device_routes`), which is exactly the
        // roaming case this snapshot exists to serve (#923).
        let rules = self
            .devices
            .find_all_rules()
            .await
            .map_err(AppError::Internal)?;
        // Device → zone mapping for the zone-clamp mirror below.
        let zone_of: HashMap<Uuid, Uuid> = self
            .devices
            .find_all()
            .await
            .map_err(AppError::Internal)?
            .into_iter()
            .map(|d| (d.id, d.zone_id))
            .collect();

        // On a per-tunnel lookup failure the previous map's entries for the
        // affected devices are carried over: "unknown" must not silently
        // drop a roaming device to the default upstream (fail-open); the
        // next successful rebuild converges.
        let prev = self.dns_device_upstream_snapshot.load_full();
        // Cache the per-tunnel gate and per-zone lookups — devices sharing
        // a tunnel or zone shouldn't repeat the repo round-trips.
        let mut gate_cache: HashMap<Uuid, Option<bool>> = HashMap::new();
        let mut zone_cache: HashMap<Uuid, Option<NetworkZone>> = HashMap::new();
        let mut map: HashMap<Uuid, UpstreamId> = HashMap::new();
        for rule in rules {
            let RoutingTarget::Tunnel { tunnel_id } = self.resolve_target(&rule.target) else {
                continue;
            };

            // Zone gate (#736): a `Default` rule whose resolved policy the
            // device's zone forbids is clamped to direct by the zone
            // enforcer — in applied state only, so this rebuild must mirror
            // the clamp or it would route the device's roaming DNS through
            // the forbidden tunnel. Explicit tunnel rules are the #735
            // write gate's job and pass through, matching applied state.
            if matches!(rule.target, RoutingTarget::Default)
                && self
                    .zone_clamps_default_to_direct(
                        &mut zone_cache,
                        zone_of.get(&rule.device_id).copied(),
                    )
                    .await
            {
                continue;
            }

            let gate = if let Some(g) = gate_cache.get(&tunnel_id) {
                *g
            } else {
                let g = self.tunnel_dns_upstream_gate(tunnel_id).await;
                gate_cache.insert(tunnel_id, g);
                g
            };
            match gate {
                Some(true) => {
                    map.insert(rule.device_id, UpstreamId::Tunnel(tunnel_id));
                }
                Some(false) => {}
                None => {
                    if let Some(entry) = prev.get(&rule.device_id) {
                        map.insert(rule.device_id, *entry);
                    }
                }
            }
        }

        let entry_count = map.len();
        tracing::debug!(
            entry_count,
            "rebuilt device-keyed DNS upstream snapshot from persisted rules: entry_count={entry_count}"
        );
        self.dns_device_upstream_snapshot.store(Arc::new(map));
        Ok(())
    }
}
