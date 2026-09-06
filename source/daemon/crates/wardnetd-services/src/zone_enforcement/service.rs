//! Network-Zone packet enforcer (issue #736 — Phase 1 / CI-2).
//!
//! Translates each device's [`NetworkZone`] into nftables rules that make the
//! zone *mean something* on a flat shared subnet, even when Wardnet is not the
//! DHCP server:
//!
//! - **Egress gate** — a device may only egress via a routing-target *kind* its
//!   zone permits. A forbidden path is dropped in the forward chain (`wg_ward*`
//!   for a tunnel-forbidding zone, the LAN/WAN interface for a direct-forbidding
//!   one).
//! - **Admin-UI gate** — a zone with `admin_ui_reachable = false` has its
//!   devices refused (TCP reset) on the Pi's :443/:7411 admin surfaces while
//!   DNS (:53) and DHCP still pass.
//!
//! Rules are keyed by device IP via nftables comment UDATA
//! (`wardnet:zone:<kind>:<ip>`), so they survive daemon restarts, and are
//! live-reloaded on zone/device events with no restart. After each live change
//! the enforcer flushes conntrack for the affected IP so already-open flows are
//! re-evaluated immediately.
//!
//! The enforcer also closes the one edge the #735 write-time zone gate cannot
//! catch: a change to the **global default routing policy** re-resolves every
//! `Default`-ruled device at once, which can bind a device to a target its zone
//! forbids. On [`WardnetEvent::DefaultPolicyChanged`] (and on startup) the
//! enforcer unbinds any such device back to direct via the routing service.
//!
//! **Honest limitation:** same-subnet peer↔peer traffic is *not* affected here.
//! On a flat L2 segment the daemon never sees it; peer isolation is delegated to
//! the access point (or the `IsolateMembers` rung, #737). See
//! `adr-network-zone-enforcement.md` and the epic-#244 ADR.
//!
//! [`WardnetEvent::DefaultPolicyChanged`]: wardnet_common::event::WardnetEvent::DefaultPolicyChanged

use std::collections::{BTreeSet, HashMap, HashSet};
use std::net::Ipv4Addr;
use std::sync::Arc;

use async_trait::async_trait;
use ipnetwork::Ipv4Network;
use uuid::Uuid;
use wardnet_common::device::Device;
use wardnet_common::network_zone::{AllowedTargetKind, NetworkZone};
use wardnet_common::routing::RoutingTarget;
use wardnet_common::zone_exception::{ExceptionEndpoint, ExceptionEndpointKind};

use wardnetd_data::repository::{
    DeviceRepository, NetworkZoneRepository, SystemConfigRepository, ZoneExceptionRepository,
};

use crate::auth_context;
use crate::dhcp::DhcpService;
use crate::error::AppError;
use crate::routing::RoutingService;
use crate::routing::firewall::{ExceptionAllow, FirewallManager, ZoneIsolationRules, ZoneRules};
use crate::routing::policy_router::PolicyRouter;

/// Applies and live-reloads per-device Network-Zone nftables enforcement.
///
/// All methods are admin-guarded and are driven by `ZoneEnforcementListener`
/// (in `wardnetd`) from the domain event bus, plus a startup [`Self::reconcile`].
#[async_trait]
pub trait ZoneEnforcementService: Send + Sync {
    /// Reconcile kernel zone rules with the database on startup.
    ///
    /// Installs every current device's zone rules, drops orphaned rules for IPs
    /// no longer backed by a device, and clamps any forbidden `Default`-policy
    /// binding to direct. Must run *after* `RoutingService::reconcile` (which
    /// (re)creates + flushes the shared `wardnet` table, including the input
    /// chain, and applies stored routing rules).
    async fn reconcile(&self) -> Result<(), AppError>;

    /// Recompute and re-apply zone rules for a single device (its zone, IP, or
    /// discovery changed).
    async fn apply_device(&self, device_id: Uuid) -> Result<(), AppError>;

    /// Recompute and re-apply zone rules for every device currently in a zone
    /// (the zone's `allowed_targets` / `admin_ui_reachable` changed).
    async fn apply_zone(&self, zone_id: Uuid) -> Result<(), AppError>;

    /// Re-key a device's zone rules after its IP changed: drop the old-IP rules
    /// and install rules for the new IP.
    async fn handle_ip_change(
        &self,
        device_id: Uuid,
        old_ip: &str,
        new_ip: &str,
    ) -> Result<(), AppError>;

    /// Remove a departed device's zone rules.
    async fn remove_device(&self, device_id: Uuid, last_ip: &str) -> Result<(), AppError>;

    /// React to a global default-policy change by unbinding any `Default`-ruled
    /// device whose zone forbids the newly-resolved target kind (see the module
    /// docs). Packet rules are unaffected — they depend only on the zone.
    async fn handle_default_policy_changed(&self, policy: &str) -> Result<(), AppError>;

    /// React to a device being reassigned to a different zone (issue #737):
    /// release its DHCP lease and flush its conntrack so it re-IPs into the new
    /// zone's subnet, then re-apply its per-device #736 rules + host route and
    /// recompute the whole L3 isolation state.
    async fn handle_zone_change(&self, device_id: Uuid) -> Result<(), AppError>;

    /// React to a cross-zone exception being created, updated, or deleted (issue
    /// #737): recompute and atomically rebuild the whole L3 isolation state.
    async fn handle_exceptions_changed(&self) -> Result<(), AppError>;
}

/// Default [`ZoneEnforcementService`] implementation.
///
/// Reads zones + devices from their repositories, installs nftables rules via
/// the shared [`FirewallManager`], flushes conntrack via the shared
/// [`PolicyRouter`], and calls back into the [`RoutingService`] to unbind
/// forbidden default-policy bindings.
/// A canonical cross-zone switchback snapshot: `(device_id, device_ip, sorted
/// target CIDRs)` over all managed devices, sorted by id. Compared against the
/// last-pushed value to debounce the per-device push loop.
type SwitchbackSnapshot = Vec<(Uuid, String, Vec<String>)>;

pub struct ZoneEnforcementServiceImpl {
    zones: Arc<dyn NetworkZoneRepository>,
    devices: Arc<dyn DeviceRepository>,
    system_config: Arc<dyn SystemConfigRepository>,
    /// Cross-zone exceptions — the ACCEPT rules that punch through the
    /// cross-subnet default-deny (issue #737).
    exceptions: Arc<dyn ZoneExceptionRepository>,
    firewall: Arc<dyn FirewallManager>,
    policy_router: Arc<dyn PolicyRouter>,
    routing: Arc<dyn RoutingService>,
    /// DHCP service, used to release a moved device's lease so it re-IPs into
    /// its new zone subnet (issue #737 `handle_zone_change`).
    dhcp: Arc<dyn DhcpService>,
    /// WAN-facing egress interface, used for the direct-egress drop and (issue
    /// #737) the per-zone gateway aliases + host routes.
    lan_interface: String,
    /// Wardnet's own LAN IP, the base of the default `/24` subnet and the
    /// address never removed by the gateway-alias reconciler (issue #737).
    lan_ip: Ipv4Addr,
    /// The last L3 isolation state actually applied to the kernel: the sorted
    /// [`ZoneIsolationRules`] plus the desired gateway-alias set. A startup
    /// burst of N device events all compute the same desired state; comparing
    /// against this lets `reconcile_isolation` collapse them into one real
    /// rebuild + (N-1) cheap no-ops (issue #737, FIX 6).
    last_isolation:
        tokio::sync::Mutex<Option<(ZoneIsolationRules, std::collections::BTreeSet<String>)>>,
    /// The last cross-zone switchback state pushed to the routing service: a
    /// canonical `(device_id, device_ip, sorted target CIDRs)` snapshot over ALL
    /// managed devices, sorted by id. A device-zone change alters switchback
    /// membership without changing the isolation `rules`, so this is tracked
    /// separately from [`Self::last_isolation`]; comparing against it lets the
    /// per-device push loop run only when the desired switchback set actually
    /// changed (avoiding an O(N²) push over a startup burst).
    last_switchback: tokio::sync::Mutex<Option<SwitchbackSnapshot>>,
}

impl ZoneEnforcementServiceImpl {
    /// Create a new enforcer from its repositories, shared backends, and the
    /// routing service it clamps forbidden default bindings through.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        zones: Arc<dyn NetworkZoneRepository>,
        devices: Arc<dyn DeviceRepository>,
        system_config: Arc<dyn SystemConfigRepository>,
        exceptions: Arc<dyn ZoneExceptionRepository>,
        firewall: Arc<dyn FirewallManager>,
        policy_router: Arc<dyn PolicyRouter>,
        routing: Arc<dyn RoutingService>,
        dhcp: Arc<dyn DhcpService>,
        lan_interface: String,
        lan_ip: Ipv4Addr,
    ) -> Self {
        Self {
            zones,
            devices,
            system_config,
            exceptions,
            firewall,
            policy_router,
            routing,
            dhcp,
            lan_interface,
            lan_ip,
            last_isolation: tokio::sync::Mutex::new(None),
            last_switchback: tokio::sync::Mutex::new(None),
        }
    }

    /// The packet policy a zone implies: its `allowed_targets` become the
    /// egress allows, and its `admin_ui_reachable` flag the admin-UI allow.
    fn zone_rules(zone: &NetworkZone) -> ZoneRules {
        ZoneRules {
            allow_direct: zone.permits_kind(AllowedTargetKind::Direct),
            allow_tunnel: zone.permits_kind(AllowedTargetKind::Tunnel),
            admin_ui_reachable: zone.admin_ui_reachable,
        }
    }

    /// True if `ip` is an address the Pi itself owns — the primary LAN IP or
    /// any per-zone gateway alias — in which case the caller must do nothing
    /// at all with it.
    ///
    /// Enforcement is keyed by device IP, so a device row claiming one of the
    /// Pi's own addresses turns every enforcement action against the Pi itself
    /// (see the module notes on issue #886: a proxy-ARP'd claim of the LAN IP
    /// drove a host-route removal that blackholed the box). The gateway aliases
    /// `reconcile_isolation` installs are just as much ours as the primary
    /// address — flushing conntrack for one kills every live flow through that
    /// zone.
    ///
    /// Discovery refuses to record our own IP, but a row can survive in an
    /// older database, so enforcement refuses independently. Loud, because it
    /// always means the device inventory is lying about a real IP conflict.
    async fn is_own_address(&self, ip: &str, op: &str) -> bool {
        let Ok(parsed) = ip.parse::<Ipv4Addr>() else {
            return false;
        };
        let is_ours = (!self.lan_ip.is_unspecified() && parsed == self.lan_ip)
            || self.is_zone_gateway(parsed).await;
        if is_ours {
            tracing::warn!(
                ip,
                op,
                lan_ip = %self.lan_ip,
                "zone enforcer: refusing to enforce on {ip} during {op} — it is one of the \
                 daemon's own addresses and a device record claims it (issue #886)",
                ip = ip,
                op = op,
            );
        }
        is_ours
    }

    /// True if `addr` is the gateway alias of any zone subnet — an address
    /// `reconcile_isolation` puts on the Pi's own interface. A zones-repository
    /// failure degrades to `false` (the primary-LAN-IP check above still holds).
    async fn is_zone_gateway(&self, addr: Ipv4Addr) -> bool {
        let zones = match self.zones.find_all().await {
            Ok(zones) => zones,
            Err(e) => {
                tracing::warn!(error = %e, "zone enforcer: failed to load zones for own-address check");
                return false;
            }
        };
        crate::subnet::parse_zone_subnets(&zones)
            .into_iter()
            .any(|(_, net)| crate::subnet::gateway_for(net) == addr)
    }

    /// Install a device IP's zone rules. On a live change (`flush = true`) the
    /// device's conntrack is flushed so already-open flows re-evaluate at once;
    /// on bulk reconcile (`flush = false`) it is skipped — the table was just
    /// flushed and there are no meaningful flows to tear down.
    async fn apply_one(
        &self,
        device_ip: &str,
        zone: &NetworkZone,
        flush: bool,
    ) -> Result<(), AppError> {
        if self.is_own_address(device_ip, "apply_zone_rules").await {
            return Ok(());
        }
        let rules = Self::zone_rules(zone);
        self.firewall
            .apply_zone_rules(device_ip, rules, &self.lan_interface)
            .await
            .map_err(AppError::Internal)?;
        if flush && let Err(e) = self.policy_router.flush_conntrack(device_ip).await {
            tracing::warn!(
                error = %e,
                device_ip,
                "zone enforcer: failed to flush conntrack after apply (live flows may lag)"
            );
        }
        Ok(())
    }

    /// Load a device and its zone. Returns `None` (with a log) if the device or
    /// its referenced zone is gone — a benign race during discovery/teardown.
    async fn load_device_and_zone(
        &self,
        device_id: Uuid,
    ) -> Result<Option<(Device, NetworkZone)>, AppError> {
        let Some(device) = self
            .devices
            .find_by_id(&device_id.to_string())
            .await
            .map_err(AppError::Internal)?
        else {
            tracing::debug!(device_id = %device_id, "zone enforcer: device not found, skipping");
            return Ok(None);
        };
        let Some(zone) = self
            .zones
            .find_by_id(&device.zone_id.to_string())
            .await
            .map_err(AppError::Internal)?
        else {
            tracing::warn!(
                device_id = %device_id,
                zone_id = %device.zone_id,
                "zone enforcer: device references unknown zone, skipping"
            );
            return Ok(None);
        };
        // Chokepoint for every device-keyed entry point, present and future: a
        // row without a usable address, or one claiming an address the Pi
        // itself owns, is not a device to enforce on (issue #886). The
        // IP-keyed teardown paths (`remove_device`, `handle_ip_change`) carry
        // their own guards, as do `apply_one`/`manage_host_route` for the
        // IP-keyed reconcile loop.
        if device.last_ip.parse::<Ipv4Addr>().is_err() {
            tracing::debug!(
                device_id = %device_id,
                last_ip = %device.last_ip,
                "zone enforcer: device has no usable IP, skipping"
            );
            return Ok(None);
        }
        if self
            .is_own_address(&device.last_ip, "load_device_and_zone")
            .await
        {
            return Ok(None);
        }
        Ok(Some((device, zone)))
    }

    /// Unbind every `Default`-ruled device whose zone forbids the target `policy`
    /// resolves to, pinning it back to direct through the routing service. The
    /// one edge the #735 write-time gate cannot catch (a policy flip re-resolves
    /// all `Default` rules at once). A device whose zone forbids *direct* too
    /// (a tunnel-only zone under a `direct` policy) has no valid fallback, so its
    /// binding is left for the forward-chain drop to handle.
    async fn clamp_default_bindings(&self, policy: &str) -> Result<(), AppError> {
        let resolved = RoutingTarget::from_default_policy(policy);
        let Some(kind) = AllowedTargetKind::of_target(&resolved) else {
            // `from_default_policy` never yields `Default`; unreachable.
            return Ok(());
        };
        let devices = self.devices.find_all().await.map_err(AppError::Internal)?;
        let mut clamped = 0u32;
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
                        "zone enforcer: failed to load rule while clamping default bindings"
                    );
                    continue;
                }
            };
            // Only stored `Default` rules re-resolve on a policy flip; explicit
            // Direct/Tunnel rules were already validated by the #735 write gate.
            if !matches!(rule.target, RoutingTarget::Default) {
                continue;
            }
            let Some(zone) = self
                .zones
                .find_by_id(&device.zone_id.to_string())
                .await
                .ok()
                .flatten()
            else {
                continue;
            };
            if zone.permits_kind(kind) {
                continue;
            }
            if !zone.permits_kind(AllowedTargetKind::Direct) {
                tracing::warn!(
                    device_id = %device.id,
                    zone = %zone.name,
                    policy,
                    "zone enforcer: default policy forbidden by zone which also forbids direct; \
                     leaving binding for the packet-layer drop"
                );
                continue;
            }
            tracing::info!(
                device_id = %device.id,
                zone = %zone.name,
                policy,
                "zone enforcer: clamping Default binding to direct (zone forbids resolved target)"
            );
            if let Err(e) = self
                .routing
                .apply_rule_for_device(device.id, &RoutingTarget::Direct)
                .await
            {
                tracing::warn!(
                    error = %e,
                    device_id = %device.id,
                    "zone enforcer: failed to clamp default binding to direct"
                );
            } else {
                clamped += 1;
            }
        }
        if clamped > 0 {
            tracing::info!(
                clamped,
                "zone enforcer: clamped default-policy bindings to direct"
            );
        }
        Ok(())
    }

    /// Read the persisted global default policy, defaulting to `"direct"`.
    async fn current_default_policy(&self) -> Result<String, AppError> {
        Ok(self
            .system_config
            .get_default_policy()
            .await
            .map_err(AppError::Internal)?
            .unwrap_or_else(|| "direct".to_owned()))
    }

    /// Is Wardnet the authoritative DHCP server? The L3 isolation surface (per-
    /// zone subnets, gateway aliases, member isolation) is inert unless Wardnet
    /// owns DHCP — otherwise it cannot hand devices the subnetted addresses the
    /// enforcement assumes. Missing key ⇒ disabled.
    async fn dhcp_enabled(&self) -> Result<bool, AppError> {
        Ok(self
            .system_config
            .get("dhcp_enabled")
            .await
            .map_err(AppError::Internal)?
            .as_deref()
            == Some("true"))
    }

    /// The base LAN subnet — Wardnet's own IP with the *configured* DHCP subnet
    /// mask (issue #737, FIX 3). An operator running a non-`/24` LAN (e.g. a
    /// `/16`) would otherwise have the base subnet mis-sized as a `/24`, which
    /// both mis-scopes the cross-subnet deny pairs and lets base-subnet gateway
    /// aliases outside the assumed `/24` be treated as stale.
    ///
    /// Reads `dhcp_subnet_mask` (default `255.255.255.0`), converts it to a
    /// prefix via `count_ones()`, and builds `lan_ip/prefix`. Any parse error or
    /// invalid prefix falls back to `/24`.
    async fn base_cidr(&self) -> Ipv4Network {
        let prefix = self
            .system_config
            .get("dhcp_subnet_mask")
            .await
            .ok()
            .flatten()
            .and_then(|m| m.parse::<Ipv4Addr>().ok())
            .map_or(24, |mask| {
                u8::try_from(u32::from(mask).count_ones()).unwrap_or(24)
            });
        Ipv4Network::new(self.lan_ip, prefix).unwrap_or_else(|_| {
            tracing::warn!(
                prefix,
                "zone enforcer: failed to build base subnet, using lan_ip/24"
            );
            Ipv4Network::new(self.lan_ip, 24).unwrap_or_else(|_| {
                Ipv4Network::new(self.lan_ip, 32).expect("a /32 is always valid")
            })
        })
    }

    /// Resolve one exception endpoint to a CIDR string:
    /// - a device to `last_ip/32` (skip+warn if the device is gone);
    /// - a zone to its subnet CIDR, or the base subnet if the zone has none
    ///   (skip+warn if the zone is gone).
    async fn resolve_endpoint_cidr(
        &self,
        endpoint: &ExceptionEndpoint,
        base_cidr: &Ipv4Network,
    ) -> Option<String> {
        match endpoint.kind {
            ExceptionEndpointKind::Device => {
                match self.devices.find_by_id(&endpoint.id.to_string()).await {
                    // Discovery clears `last_ip` the moment a device departs, so
                    // an endpoint that is merely switched off arrives here with
                    // no address. Rendering it anyway yields a bare "/32", which
                    // the firewall rejects — and `apply_zone_isolation` fails as
                    // a unit, so one absent device would drop every unrelated
                    // cross-zone deny and leave the network with no isolation.
                    // Skipping the exception costs only that exception, and it
                    // returns by itself when the device is seen again.
                    Ok(Some(device)) => {
                        let Ok(ip) = device.last_ip.parse::<Ipv4Addr>() else {
                            tracing::warn!(
                                device_id = %endpoint.id,
                                "zone enforcer: exception endpoint has no address, skipping"
                            );
                            return None;
                        };
                        Some(format!("{ip}/32"))
                    }
                    Ok(None) => {
                        tracing::warn!(
                            device_id = %endpoint.id,
                            "zone enforcer: exception references unknown device, skipping"
                        );
                        None
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, device_id = %endpoint.id, "zone enforcer: failed to load exception device");
                        None
                    }
                }
            }
            ExceptionEndpointKind::Zone => {
                match self.zones.find_by_id(&endpoint.id.to_string()).await {
                    Ok(Some(zone)) => Some(
                        zone.subnet
                            .map_or_else(|| crate::subnet::canonical_cidr(*base_cidr), |s| s.cidr),
                    ),
                    Ok(None) => {
                        tracing::warn!(
                            zone_id = %endpoint.id,
                            "zone enforcer: exception references unknown zone, skipping"
                        );
                        None
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, zone_id = %endpoint.id, "zone enforcer: failed to load exception zone");
                        None
                    }
                }
            }
        }
    }

    /// Add or remove a device's `/32` host route: an isolate-members device in a
    /// subnetted zone (under DHCP-mode) needs an on-link path from the Pi so the
    /// proxy-ARP'd peer traffic can be forwarded/filtered; otherwise the route
    /// is removed. Errors are warn-logged, never fatal.
    async fn manage_host_route(&self, device: &Device, zone: &NetworkZone, dhcp_enabled: bool) {
        // Never install *or* remove a host route for our own address: the
        // removal path is what deleted the kernel's local route and locked the
        // box out of itself (#886).
        if self
            .is_own_address(&device.last_ip, "manage_host_route")
            .await
        {
            return;
        }
        // The route's preferred source must be the device's own zone gateway
        // (#1198), so a zone whose CIDR won't parse yields no gateway and we
        // remove rather than install — far better than installing a `/32` that
        // silently breaks source selection for the whole zone. Warn on the way
        // past, matching `parse_zone_subnets`: without it a corrupt CIDR row
        // would stop member isolation forwarding for every device in the zone
        // with nothing in the logs to say why.
        let gateway = if dhcp_enabled && zone.member_isolation {
            zone.subnet.as_ref().and_then(|s| {
                let net = match s.cidr.parse::<Ipv4Network>() {
                    Ok(net) => net,
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            zone_id = %zone.id,
                            cidr = %s.cidr,
                            device_ip = %device.last_ip,
                            "zone enforcer: unparseable zone subnet, skipping host route"
                        );
                        return None;
                    }
                };
                // The device must actually be inside the zone's subnet, the
                // same guard `reconcile_isolation` applies to proxy-neighbour
                // entries. A device freshly moved into the zone keeps its old
                // base-subnet address until it re-DHCPs, and pointing its `/32`
                // at this zone's gateway would source replies from an address
                // outside the device's own subnet — reintroducing the very
                // wrong-source-IP failure this route exists to fix (#1198).
                let parsed = device.last_ip.parse::<Ipv4Addr>().ok()?;
                if !net.contains(parsed) {
                    tracing::debug!(
                        zone_id = %zone.id,
                        cidr = %s.cidr,
                        device_ip = %device.last_ip,
                        "zone enforcer: device address outside its zone subnet, no host route"
                    );
                    return None;
                }
                Some(crate::subnet::gateway_for(net))
            })
        } else {
            None
        };
        let want = gateway.is_some();
        let res = if let Some(pref_src) = gateway {
            self.policy_router
                .add_host_route(&device.last_ip, &self.lan_interface, pref_src)
                .await
        } else {
            self.policy_router
                .remove_host_route(&device.last_ip, &self.lan_interface)
                .await
        };
        if let Err(e) = res {
            tracing::warn!(
                error = %e,
                device_ip = %device.last_ip,
                want,
                "zone enforcer: failed to manage host route"
            );
        }
    }

    /// Bring the LAN interface's IPv4 proxy-neighbour entries to exactly
    /// `desired` (issue #1107): add the missing entries, prune the rest. The Pi
    /// owns the LAN interface's IPv4 pneigh entries — a stale one is exactly
    /// the "answers ARP for an address nobody isolates" failure of #1107, so
    /// pruning is unconditional. The diff runs against the kernel's own listing
    /// (never a stored snapshot), so a purged or half-applied state heals on
    /// the next pass. If the listing fails, `existing` degrades to empty: the
    /// idempotent adds still run and pruning naturally no-ops until a listing
    /// succeeds. Errors are warn-logged, never fatal.
    async fn reconcile_proxy_neigh(&self, desired: BTreeSet<String>) {
        let existing: BTreeSet<String> = match self
            .policy_router
            .list_neigh_proxies(&self.lan_interface)
            .await
        {
            Ok(entries) => entries.into_iter().collect(),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    interface = %self.lan_interface,
                    "zone enforcer: failed to list proxy-neighbour entries on {interface}: {e}",
                    interface = self.lan_interface,
                    e = e,
                );
                BTreeSet::new()
            }
        };
        if desired == existing {
            return;
        }
        // The per-entry netlink calls are independent, so run each direction
        // concurrently instead of serializing the round-trips.
        futures::future::join_all(desired.difference(&existing).map(|ip| async move {
            if let Err(e) = self
                .policy_router
                .add_neigh_proxy(ip, &self.lan_interface)
                .await
            {
                tracing::warn!(
                    error = %e,
                    ip,
                    "zone enforcer: failed to add proxy-neighbour entry for {ip}: {e}",
                    ip = ip,
                    e = e,
                );
            }
        }))
        .await;
        futures::future::join_all(existing.difference(&desired).map(|ip| async move {
            if let Err(e) = self
                .policy_router
                .remove_neigh_proxy(ip, &self.lan_interface)
                .await
            {
                tracing::warn!(
                    error = %e,
                    ip,
                    "zone enforcer: failed to remove stale proxy-neighbour entry for {ip}: {e}",
                    ip = ip,
                    e = e,
                );
            }
        }))
        .await;
        let members = desired.len();
        tracing::info!(
            members,
            "zone enforcer: proxy-neighbour entries reconciled ({members} members)",
        );
    }

    /// Recompute the FULL desired L3 isolation state and apply it atomically:
    /// the `zone_isolation` chain (allows → cross-subnet denies → member denies),
    /// the per-zone gateway aliases, and the per-member proxy-neighbour entries
    /// (issue #1107). See the module design notes.
    ///
    /// When Wardnet does not own DHCP the whole surface degrades gracefully to
    /// the #736 baseline: an empty isolation chain, no gateway aliases, no
    /// proxy-neighbour entries. All firewall/policy errors are warn-logged,
    /// never fatal.
    #[allow(clippy::too_many_lines)]
    async fn reconcile_isolation(&self) -> Result<(), AppError> {
        let base_cidr = self.base_cidr().await;

        // All managed devices, loaded once. Used to enumerate the source devices
        // of each cross-zone exception (for the switchback carve-outs) and to
        // push every device its (possibly empty) target set at the end.
        let all_devices = self.devices.find_all().await.map_err(AppError::Internal)?;

        // Cross-zone switchback carve-outs accumulated from the exception set:
        // `device_id → (device_ip, target CIDRs)`. A tunnel-bound source device
        // of a casting exception must reach the other endpoint's CIDR directly,
        // bypassing its per-tunnel table (see `RoutingService::set_switchback_targets`).
        let mut switchback_acc: HashMap<Uuid, (String, BTreeSet<String>)> = HashMap::new();

        // Compute the FULL desired state first (rules + gateway aliases), then
        // diff it against the last-applied state so a startup burst of identical
        // device events collapses to one real rebuild + (N-1) no-ops (FIX 6).
        // The desired rules and gateway set are computed in both the DHCP-off
        // (empty) and DHCP-on branches so the skip covers every path.

        let dhcp_enabled = self.dhcp_enabled().await?;

        // Desired zone subnets, gateway aliases, isolation rules.
        let mut zone_nets: HashMap<Uuid, Ipv4Network> = HashMap::new();
        let mut rules;
        let mut desired_alias_map: std::collections::BTreeMap<Ipv4Addr, u8> =
            std::collections::BTreeMap::new();
        // Per-member proxy-neighbour entries (issue #1107): the IPs the Pi
        // should answer ARP for on the LAN interface — exactly the members of
        // isolate-members subnetted zones, nothing else. Empty when Wardnet
        // does not own DHCP.
        let mut desired_proxy_neigh: BTreeSet<String> = BTreeSet::new();

        if dhcp_enabled {
            let zones = self.zones.find_all().await.map_err(AppError::Internal)?;
            let exceptions = self
                .exceptions
                .find_all()
                .await
                .map_err(AppError::Internal)?;

            // Parse the subnet CIDR of every zone that has one.
            zone_nets.extend(crate::subnet::parse_zone_subnets(&zones));

            // The set of distinct subnets = base + each zone subnet.
            let mut all_subnets: Vec<String> = vec![crate::subnet::canonical_cidr(base_cidr)];
            for net in zone_nets.values() {
                let s = crate::subnet::canonical_cidr(*net);
                if !all_subnets.contains(&s) {
                    all_subnets.push(s);
                }
            }

            // Cross-subnet default-deny: every ordered pair of distinct subnets.
            let mut deny_pairs: Vec<(String, String)> = Vec::new();
            for a in &all_subnets {
                for b in &all_subnets {
                    if a != b {
                        deny_pairs.push((a.clone(), b.clone()));
                    }
                }
            }

            // Member-isolation subnets: the subnet of each zone whose flag is set.
            let member_isolation_subnets: Vec<String> = zones
                .iter()
                .filter(|z| z.member_isolation)
                .filter_map(|z| {
                    zone_nets
                        .get(&z.id)
                        .map(|n| crate::subnet::canonical_cidr(*n))
                })
                .collect();

            // Cross-zone allows: resolve each exception's endpoints to CIDRs and
            // expand its service into one ACCEPT per (proto, port-range).
            let mut allows: Vec<ExceptionAllow> = Vec::new();
            for exc in &exceptions {
                let Some(from_cidr) = self.resolve_endpoint_cidr(&exc.from, &base_cidr).await
                else {
                    continue;
                };
                let Some(to_cidr) = self.resolve_endpoint_cidr(&exc.to, &base_cidr).await else {
                    continue;
                };
                for port in exc.service.resolve_ports() {
                    allows.push(ExceptionAllow {
                        from_cidr: from_cidr.clone(),
                        to_cidr: to_cidr.clone(),
                        proto: port.proto.as_str().to_owned(),
                        port_start: port.from,
                        port_end: port.to,
                        bidirectional: exc.bidirectional,
                    });
                }

                // Switchback carve-outs: every source DEVICE of this exception's
                // FROM endpoint must reach the TO endpoint's CIDR directly. A
                // bidirectional exception additionally lets the TO endpoint's
                // devices reach the FROM CIDR. A device is a source of a Zone
                // endpoint when its `zone_id` matches; of a Device endpoint when
                // its `id` matches.
                for dev in endpoint_source_devices(&exc.from, &all_devices) {
                    add_switchback_target(&mut switchback_acc, dev, &to_cidr);
                }
                if exc.bidirectional {
                    for dev in endpoint_source_devices(&exc.to, &all_devices) {
                        add_switchback_target(&mut switchback_acc, dev, &from_cidr);
                    }
                }
            }

            // NAT-exemption pairs: one `(from, to)` per exception (the firewall
            // renders both directions). The per-port allow expansion produces
            // duplicate CIDR pairs, so collapse to a unique set.
            let nat_exempt_pairs: Vec<(String, String)> = allows
                .iter()
                .map(|a| (a.from_cidr.clone(), a.to_cidr.clone()))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();

            rules = ZoneIsolationRules {
                allows,
                deny_pairs,
                member_isolation_subnets,
                nat_exempt_pairs,
            };

            // Gateway aliases: the `.1` of each zone subnet.
            for net in zone_nets.values() {
                desired_alias_map.insert(crate::subnet::gateway_for(*net), net.prefix());
            }

            // Per-member proxy-neighbour entries: every device in an
            // isolate-members subnetted zone whose address actually lies inside
            // that zone's subnet. NEVER the interface-wide `proxy_arp` sysctl —
            // its FIB-lookup semantics made the Pi answer ARP for any address a
            // tunnel-bound device probed, while never firing for the
            // same-interface case it was meant to serve (issue #1107). The
            // in-subnet check matters: a device freshly moved into the zone can
            // still hold its old base-subnet address until it re-DHCPs, and a
            // pneigh entry for that address would answer ARP for a base-subnet
            // IP the DHCP pool may re-lease — the very duplicate-IP failure
            // this mechanism replaces. Own addresses are excluded for the same
            // reason as everywhere else (#886).
            let member_zone_nets: HashMap<Uuid, Ipv4Network> = zones
                .iter()
                .filter(|z| z.member_isolation)
                .filter_map(|z| zone_nets.get(&z.id).map(|net| (z.id, *net)))
                .collect();
            for dev in &all_devices {
                let Some(net) = member_zone_nets.get(&dev.zone_id) else {
                    continue;
                };
                let Ok(parsed) = dev.last_ip.parse::<Ipv4Addr>() else {
                    continue;
                };
                if !net.contains(parsed) {
                    continue;
                }
                if parsed == self.lan_ip || desired_alias_map.contains_key(&parsed) {
                    continue;
                }
                desired_proxy_neigh.insert(dev.last_ip.clone());
            }
        } else {
            // Graceful degrade when Wardnet is not the DHCP authority: empty
            // isolation chain, no gateway aliases, no proxy-neighbour entries.
            rules = ZoneIsolationRules::default();
        }

        // Sort every vector so the applied nftables order is deterministic and
        // the equality check against the last-applied state is order-stable.
        rules.allows.sort();
        rules.deny_pairs.sort();
        rules.member_isolation_subnets.sort();
        rules.nat_exempt_pairs.sort();

        // Canonical string form of the desired gateway-alias set, used both for
        // the skip check and (kept as a set) for stale-alias removal.
        let desired_alias_strings: std::collections::BTreeSet<String> = desired_alias_map
            .keys()
            .map(std::string::ToString::to_string)
            .collect();

        // Canonical switchback snapshot over ALL managed devices: each device's
        // (id, ip, sorted desired CIDRs — empty when it has none), sorted by id.
        // This is BOTH the change key and the push list. Including every device
        // (not just those with targets) makes a device dropping to empty a
        // detectable change.
        let mut switchback_snapshot: SwitchbackSnapshot = all_devices
            .iter()
            .map(|d| {
                let targets: Vec<String> = switchback_acc
                    .get(&d.id)
                    .map(|(_, cidrs)| cidrs.iter().cloned().collect())
                    .unwrap_or_default();
                (d.id, d.last_ip.clone(), targets)
            })
            .collect();
        switchback_snapshot.sort_by_key(|e| e.0);

        // Compute both change flags up front. The isolation state and the
        // switchback state change independently: a device-zone move alters
        // switchback membership without changing the isolation `rules` (zone
        // endpoints resolve to a stable subnet CIDR), while a subnet/exception
        // edit changes the rules. Debounce each against its own last-applied
        // snapshot so a startup burst collapses to one push + one rebuild.
        let switchback_changed =
            self.last_switchback.lock().await.as_ref() != Some(&switchback_snapshot);
        let iso_changed = self.last_isolation.lock().await.as_ref()
            != Some(&(rules.clone(), desired_alias_strings.clone()));

        // Reconcile the per-member proxy-neighbour entries on EVERY pass, before
        // the change-skip below: unlike nftables rules there is no in-memory
        // snapshot to debounce against, because the kernel itself forgets pneigh
        // entries (a link flap purges them via `pneigh_ifdown`) and a netlink
        // call can fail transiently — a stored "last applied" set would mask
        // both and never retry. Diffing against the kernel's own listing makes
        // every reconcile self-healing, and costs one cheap netlink dump when
        // nothing changed. (Honest limit: a link flap with no subsequent
        // zone/device event is only repaired by the next event or restart.)
        self.reconcile_proxy_neigh(desired_proxy_neigh).await;

        // Nothing else to do when nothing changed (FIX 6, extended to
        // switchback).
        if !iso_changed && !switchback_changed {
            tracing::debug!("zone enforcer: isolation + switchback unchanged, skipping");
            return Ok(());
        }

        // Push switchback carve-out targets only when they changed. A device with
        // no targets is passed an empty set, which removes its carve-outs.
        if switchback_changed {
            for (device_id, device_ip, targets) in &switchback_snapshot {
                if let Err(e) = self
                    .routing
                    .set_switchback_targets(*device_id, device_ip.clone(), targets.clone())
                    .await
                {
                    tracing::warn!(
                        error = %e,
                        device_id = %device_id,
                        "zone enforcer: failed to push switchback targets"
                    );
                }
            }
            *self.last_switchback.lock().await = Some(switchback_snapshot);
        }

        // The expensive nftables/alias kernel work only runs when the isolation
        // state itself changed.
        if iso_changed {
            let summary = (
                rules.allows.len(),
                rules.deny_pairs.len(),
                rules.member_isolation_subnets.len(),
            );

            if let Err(e) = self.firewall.apply_zone_isolation(rules.clone()).await {
                tracing::warn!(error = %e, "zone enforcer: failed to apply zone isolation");
            }

            // Add desired gateway aliases, then drop any no longer backed by a zone.
            let desired_gateways: HashSet<Ipv4Addr> = desired_alias_map.keys().copied().collect();
            for (gw, prefix) in &desired_alias_map {
                if let Err(e) = self
                    .policy_router
                    .add_interface_alias(&self.lan_interface, &gw.to_string(), *prefix)
                    .await
                {
                    tracing::warn!(error = %e, gateway = %gw, "zone enforcer: failed to add gateway alias");
                }
            }
            self.remove_stale_gateway_aliases(&desired_gateways, &base_cidr)
                .await;

            // Record the applied state so the next identical reconcile is a no-op.
            *self.last_isolation.lock().await = Some((rules, desired_alias_strings));

            tracing::info!(
                allows = summary.0,
                deny_pairs = summary.1,
                member_subnets = summary.2,
                gateways = desired_gateways.len(),
                "zone enforcer: L3 isolation reconciled"
            );
        }
        Ok(())
    }

    /// Remove any wardnet-managed gateway alias on the LAN interface that is no
    /// longer a desired zone gateway. The primary LAN IP and any base-subnet
    /// address are never touched. Errors are warn-logged.
    ///
    /// FIX 3: the removal guard is tightened so an *operator-added* secondary IP
    /// is never deleted. In addition to sparing the primary, any desired
    /// gateway, and base-subnet addresses, an alias is only removed when it also
    /// *looks like* a Wardnet-managed zone gateway — i.e. it is the `.1`
    /// (first-host) of its own subnet at `base_prefix`. An operator's arbitrary
    /// secondary such as `10.0.5.5/24` (where `.5` is not the first host) is
    /// therefore left in place.
    async fn remove_stale_gateway_aliases(
        &self,
        desired_gateways: &HashSet<Ipv4Addr>,
        base_cidr: &Ipv4Network,
    ) {
        let aliases = match self
            .policy_router
            .list_interface_aliases(&self.lan_interface)
            .await
        {
            Ok(a) => a,
            Err(e) => {
                tracing::warn!(error = %e, "zone enforcer: failed to list interface aliases");
                return;
            }
        };
        for (ip_str, prefix) in aliases {
            let Ok(ip) = ip_str.parse::<Ipv4Addr>() else {
                continue;
            };
            // Never remove the primary, a desired gateway, or a base-subnet addr.
            if ip == self.lan_ip || desired_gateways.contains(&ip) || base_cidr.contains(ip) {
                continue;
            }
            // Only remove an alias that looks Wardnet-managed: the first host of
            // its own subnet. Preserves any operator secondary that is not a `.1`.
            let looks_managed =
                Ipv4Network::new(ip, prefix).is_ok_and(|net| ip == crate::subnet::gateway_for(net));
            if !looks_managed {
                tracing::debug!(alias = %ip_str, "zone enforcer: preserving non-gateway secondary IP");
                continue;
            }
            if let Err(e) = self
                .policy_router
                .remove_interface_alias(&self.lan_interface, &ip_str, prefix)
                .await
            {
                tracing::warn!(error = %e, alias = %ip_str, "zone enforcer: failed to remove stale gateway alias");
            }
        }
    }
}

#[async_trait]
impl ZoneEnforcementService for ZoneEnforcementServiceImpl {
    async fn reconcile(&self) -> Result<(), AppError> {
        auth_context::require_admin()?;
        tracing::info!("reconciling network-zone enforcement with kernel");

        // Migration (#1107): older daemons enabled interface-wide `proxy_arp`
        // for member isolation, which made the Pi answer ARP for any address a
        // tunnel-bound device probed. Member isolation now uses per-member
        // proxy-neighbour entries, so force the legacy sysctl off. First thing,
        // before anything fallible can abort reconcile: nothing else re-touches
        // the sysctl, and leaving it set is the #1107 bug itself. Best-effort:
        // a failure must not abort reconcile.
        if let Err(e) = self
            .policy_router
            .set_proxy_arp(&self.lan_interface, false)
            .await
        {
            tracing::warn!(
                error = %e,
                interface = %self.lan_interface,
                "zone enforcer: failed to clear legacy interface-wide proxy-arp on {interface}: {e}",
                interface = self.lan_interface,
                e = e,
            );
        }

        let devices = self.devices.find_all().await.map_err(AppError::Internal)?;
        let all_zones = self.zones.find_all().await.map_err(AppError::Internal)?;
        let zone_by_id: HashMap<Uuid, &NetworkZone> = all_zones.iter().map(|z| (z.id, z)).collect();

        let mut applied = 0u32;
        let mut live_ips: HashSet<String> = HashSet::with_capacity(devices.len());
        for device in &devices {
            // A repaired own-IP row (issue #886) has an empty `last_ip` until
            // its next observation — nothing to enforce and nothing to key
            // rules on.
            if device.last_ip.parse::<Ipv4Addr>().is_err() {
                tracing::debug!(
                    device_id = %device.id,
                    "zone enforcer: device has no usable IP during reconcile, skipping"
                );
                continue;
            }
            live_ips.insert(device.last_ip.clone());
            let Some(zone) = zone_by_id.get(&device.zone_id) else {
                tracing::warn!(
                    device_id = %device.id,
                    zone_id = %device.zone_id,
                    "zone enforcer: device references unknown zone during reconcile, skipping"
                );
                continue;
            };
            if let Err(e) = self.apply_one(&device.last_ip, zone, false).await {
                tracing::warn!(
                    error = %e,
                    device_id = %device.id,
                    "zone enforcer: failed to apply zone rules during reconcile"
                );
            } else {
                applied += 1;
            }
        }

        // Drop orphaned zone rules for IPs no longer backed by a device.
        match self.firewall.list_zone_rule_ips().await {
            Ok(ips) => {
                let mut orphans = 0u32;
                for ip in ips {
                    if !live_ips.contains(&ip) {
                        tracing::info!(device_ip = %ip, "zone enforcer: removing orphaned zone rules");
                        if let Err(e) = self.firewall.remove_zone_rules(&ip).await {
                            tracing::warn!(
                                error = %e,
                                device_ip = %ip,
                                "zone enforcer: failed to remove orphaned zone rules"
                            );
                        } else {
                            orphans += 1;
                        }
                    }
                }
                if orphans > 0 {
                    tracing::info!(orphans, "zone enforcer: removed orphaned zone rules");
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "zone enforcer: failed to list zone rules for orphan cleanup");
            }
        }

        // Close the default-policy caveat on startup: a policy set while the
        // daemon was down (or a device booting into a forbidden Default binding)
        // is clamped now, after RoutingService::reconcile applied stored rules.
        let policy = self.current_default_policy().await?;
        self.clamp_default_bindings(&policy).await?;

        // Recompute the whole L3 isolation state (cross-subnet deny + exception
        // allows + member isolation + gateway aliases + proxy-neighbour
        // entries) from scratch.
        self.reconcile_isolation().await?;

        // Host routes last, once `reconcile_isolation` has installed the zone
        // gateway aliases each route names as its preferred source (the kernel
        // rejects a non-local `RTA_PREFSRC`).
        //
        // This pass is what heals a box upgrading into the #1198 fix. Older
        // builds installed these `/32`s with no preferred source, and every
        // other host-route call site is device-event-driven — so without a
        // reconcile pass an already-broken device would keep its bad route (and
        // stay without DNS) until it happened to re-DHCP or an admin touched
        // it. `add_host_route` replaces rather than skips, so this repairs a
        // stale route in place.
        // Propagate rather than defaulting: `false` would take
        // `manage_host_route` down its *removal* path, so a transient
        // `system_config` read failure would silently strip every member's host
        // route. Aborting leaves the existing routes in place for the next
        // reconcile, and matches how every other repository read here behaves.
        let dhcp_enabled = self.dhcp_enabled().await?;
        for device in &devices {
            if device.last_ip.parse::<Ipv4Addr>().is_err() {
                continue;
            }
            let Some(zone) = zone_by_id.get(&device.zone_id) else {
                continue;
            };
            // Only a member-isolated zone can own a host route. Running the
            // rest through `manage_host_route` would take its removal path, and
            // `remove_host_route` streams the whole IPv4 route table per call —
            // a few hundred serial dumps on a box with a few hundred device
            // rows, on every startup. Removal on transition is handled by the
            // event paths (`apply_device` / `handle_ip_change` /
            // `handle_zone_change` / `apply_zone`), which fire on exactly the
            // changes that strand a route. Known gap: a transition that happens
            // while the daemon is down leaves a stale `/32` until that device's
            // next event — pruning it here would need a single bulk route
            // listing rather than one per device.
            // `dhcp_enabled` matters for the same reason: with DHCP off no
            // device can own a host route, so every one of them would take the
            // removal path and its route-table dump.
            if !dhcp_enabled || !zone.member_isolation {
                continue;
            }
            self.manage_host_route(device, zone, dhcp_enabled).await;
        }

        tracing::info!(
            applied,
            total_devices = devices.len(),
            "network-zone enforcement reconcile complete"
        );
        Ok(())
    }

    async fn apply_device(&self, device_id: Uuid) -> Result<(), AppError> {
        auth_context::require_admin()?;
        tracing::debug!(device_id = %device_id, "zone enforcer: apply_device");
        let pending = if let Some((device, zone)) = self.load_device_and_zone(device_id).await? {
            self.apply_one(&device.last_ip, &zone, true).await?;
            Some((device, zone))
        } else {
            None
        };
        // Isolation first, host route second: `reconcile_isolation` is what
        // installs the zone's `.1` gateway alias, and the kernel rejects a
        // route whose `RTA_PREFSRC` is not a local address (`EINVAL` from
        // `fib_create_info`). Managing the route before the alias exists would
        // fail the add outright on a zone's first application (#1198).
        self.reconcile_isolation().await?;
        if let Some((device, zone)) = pending {
            let dhcp_enabled = self.dhcp_enabled().await?;
            self.manage_host_route(&device, &zone, dhcp_enabled).await;
        }
        Ok(())
    }

    async fn apply_zone(&self, zone_id: Uuid) -> Result<(), AppError> {
        auth_context::require_admin()?;
        tracing::debug!(zone_id = %zone_id, "zone enforcer: apply_zone");
        let Some(zone) = self
            .zones
            .find_by_id(&zone_id.to_string())
            .await
            .map_err(AppError::Internal)?
        else {
            // Zone deleted — its (now unreferenced) members will be re-applied
            // when they are reassigned; nothing to install here.
            tracing::debug!(zone_id = %zone_id, "zone enforcer: zone not found, skipping");
            return Ok(());
        };
        let devices = self.devices.find_all().await.map_err(AppError::Internal)?;
        let mut applied = 0u32;
        for device in &devices {
            if device.zone_id != zone_id {
                continue;
            }
            if let Err(e) = self.apply_one(&device.last_ip, &zone, true).await {
                tracing::warn!(
                    error = %e,
                    device_id = %device.id,
                    "zone enforcer: failed to apply zone rules for member"
                );
            } else {
                applied += 1;
            }
        }
        tracing::debug!(zone_id = %zone_id, applied, "zone enforcer: apply_zone complete");
        self.reconcile_isolation().await?;

        // Re-manage every member's host route after the aliases are settled.
        // A zone edit is exactly what invalidates these: changing the subnet
        // moves the gateway alias, so each member's `/32` is left naming an
        // address that is no longer local — and the kernel drops those routes
        // when the old alias is deleted (`fib_sync_down_addr`), silently
        // removing the on-link path member isolation depends on. Toggling
        // `member_isolation` on has the mirror problem: proxy-neighbour entries
        // appear but no `/32` is ever installed. Neither transition raises a
        // per-device event, so without this pass both wait for an unrelated
        // device event or a daemon restart (#1198).
        //
        // On a *subnet* edit this removes rather than re-points: every member
        // still holds an address in the old subnet (the lease release has only
        // just gone out), so the in-subnet guard yields no gateway and the
        // stale `/32` is dropped. That is the safe direction — the alternative
        // is a route naming a gateway the kernel is about to delete — and each
        // device gets its route back through `handle_ip_change` as it re-DHCPs
        // into the new subnet.
        // Propagate rather than defaulting to `false` — see `reconcile`: the
        // default is the destructive direction, not the safe one.
        let dhcp_enabled = self.dhcp_enabled().await?;
        for device in &devices {
            if device.zone_id != zone_id || device.last_ip.parse::<Ipv4Addr>().is_err() {
                continue;
            }
            self.manage_host_route(device, &zone, dhcp_enabled).await;
        }
        Ok(())
    }

    async fn handle_ip_change(
        &self,
        device_id: Uuid,
        old_ip: &str,
        new_ip: &str,
    ) -> Result<(), AppError> {
        auth_context::require_admin()?;
        tracing::debug!(device_id = %device_id, old_ip, new_ip, "zone enforcer: handle_ip_change");
        // A device flapping off our own address must not drag our own state down
        // with it: we never enforced on that IP, so there is nothing to tear
        // down, and tearing down anyway deletes the kernel's local route (#886).
        // The `new_ip` side is guarded inside apply_one/manage_host_route.
        if !self.is_own_address(old_ip, "handle_ip_change/old_ip").await {
            // Drop the stale-IP rules first so they never outlive the device's move.
            self.firewall
                .remove_zone_rules(old_ip)
                .await
                .map_err(AppError::Internal)?;
            // The old IP's `/32` host route is no longer valid — drop it before
            // the new one is (conditionally) installed below.
            if let Err(e) = self
                .policy_router
                .remove_host_route(old_ip, &self.lan_interface)
                .await
            {
                tracing::warn!(error = %e, old_ip, "zone enforcer: failed to remove old-IP host route");
            }
        }
        let pending = if let Some((device, zone)) = self.load_device_and_zone(device_id).await? {
            // Key on the event's new IP rather than the row's `last_ip`, which
            // may not have been observed yet.
            self.apply_one(new_ip, &zone, true).await?;
            // Install the new-IP host route from the event's IP (the row's
            // `last_ip` may lag), so build a device view keyed on `new_ip`.
            let device = Device {
                last_ip: new_ip.to_owned(),
                ..device
            };
            Some((device, zone))
        } else {
            None
        };
        // Isolation before the host route so the zone gateway alias the route
        // prefers as source already exists — see `apply_device` (#1198).
        self.reconcile_isolation().await?;
        if let Some((device, zone)) = pending {
            let dhcp_enabled = self.dhcp_enabled().await?;
            self.manage_host_route(&device, &zone, dhcp_enabled).await;
        }
        Ok(())
    }

    async fn remove_device(&self, device_id: Uuid, last_ip: &str) -> Result<(), AppError> {
        auth_context::require_admin()?;
        tracing::debug!(device_id = %device_id, last_ip, "zone enforcer: remove_device");
        // Nothing was ever enforced on our own address, so there is nothing to
        // remove — and removing anyway takes the kernel's local route with it
        // (#886). Still recompute isolation: the device is gone either way.
        if !self.is_own_address(last_ip, "remove_device").await {
            self.firewall
                .remove_zone_rules(last_ip)
                .await
                .map_err(AppError::Internal)?;
            if let Err(e) = self
                .policy_router
                .remove_host_route(last_ip, &self.lan_interface)
                .await
            {
                tracing::warn!(error = %e, last_ip, "zone enforcer: failed to remove host route");
            }
        }
        self.reconcile_isolation().await?;
        Ok(())
    }

    async fn handle_default_policy_changed(&self, policy: &str) -> Result<(), AppError> {
        auth_context::require_admin()?;
        tracing::debug!(policy, "zone enforcer: handle_default_policy_changed");
        // Packet rules depend only on the zone, so a policy flip changes no zone
        // rule — only which target a device's `Default` rule resolves to. Clamp
        // any now-forbidden binding back to direct.
        self.clamp_default_bindings(policy).await
    }

    async fn handle_zone_change(&self, device_id: Uuid) -> Result<(), AppError> {
        auth_context::require_admin()?;
        tracing::debug!(device_id = %device_id, "zone enforcer: handle_zone_change");
        let mut pending = None;
        // A row claiming one of the Pi's own addresses is filtered inside
        // `load_device_and_zone` (#886) — the conntrack flush below would
        // otherwise tear down our own live admin sessions.
        if let Some((device, zone)) = self.load_device_and_zone(device_id).await? {
            // Force the device to re-IP into its new zone's subnet: release its
            // DHCP lease and flush its conntrack. There is a brief connectivity
            // blip until the device renews (typically seconds for a cooperating
            // client honouring lease renewal); a client ignoring the release
            // keeps its old IP until the lease expires — best-effort, non-fatal.
            if let Err(e) = self.dhcp.release_lease(&device.mac).await {
                tracing::warn!(error = %e, device_id = %device_id, "zone enforcer: failed to release lease on zone change");
            }
            if let Err(e) = self.policy_router.flush_conntrack(&device.last_ip).await {
                tracing::warn!(error = %e, device_id = %device_id, "zone enforcer: failed to flush conntrack on zone change");
            }
            // Re-apply the device's #736 packet rules for the new zone.
            self.apply_one(&device.last_ip, &zone, true).await?;
            pending = Some((device, zone));
        }
        // Isolation before the host route so the new zone's gateway alias the
        // route prefers as source already exists — see `apply_device` (#1198).
        self.reconcile_isolation().await?;
        if let Some((device, zone)) = pending {
            let dhcp_enabled = self.dhcp_enabled().await?;
            self.manage_host_route(&device, &zone, dhcp_enabled).await;
        }
        Ok(())
    }

    async fn handle_exceptions_changed(&self) -> Result<(), AppError> {
        auth_context::require_admin()?;
        tracing::debug!("zone enforcer: handle_exceptions_changed");
        self.reconcile_isolation().await
    }
}

/// The devices that are a SOURCE of a cross-zone exception endpoint: the single
/// device for a `Device` endpoint, or every device in the zone for a `Zone`
/// endpoint.
fn endpoint_source_devices<'a>(
    endpoint: &ExceptionEndpoint,
    devices: &'a [Device],
) -> Vec<&'a Device> {
    match endpoint.kind {
        ExceptionEndpointKind::Device => devices.iter().filter(|d| d.id == endpoint.id).collect(),
        ExceptionEndpointKind::Zone => devices
            .iter()
            .filter(|d| d.zone_id == endpoint.id)
            .collect(),
    }
}

/// Record that `device` must reach `target_cidr` directly across zones, keyed by
/// the device's id and remembering its IP. A target equal to the device's own
/// `/32` is skipped (a device never needs a carve-out to itself).
fn add_switchback_target(
    acc: &mut HashMap<Uuid, (String, BTreeSet<String>)>,
    device: &Device,
    target_cidr: &str,
) {
    if target_cidr == format!("{}/32", device.last_ip) {
        return;
    }
    acc.entry(device.id)
        .or_insert_with(|| (device.last_ip.clone(), BTreeSet::new()))
        .1
        .insert(target_cidr.to_owned());
}
