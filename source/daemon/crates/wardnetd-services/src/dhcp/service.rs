use std::net::Ipv4Addr;
use std::str::FromStr;
use std::sync::Arc;

use async_trait::async_trait;
use ipnetwork::Ipv4Network;
use uuid::Uuid;
use wardnet_common::api::{
    CreateDhcpReservationRequest, CreateDhcpReservationResponse, DeleteDhcpReservationResponse,
    DhcpConfigResponse, DhcpStatusResponse, ListDhcpLeasesResponse, ListDhcpReservationsResponse,
    PreviewDhcpConfigRequest, PreviewDhcpConfigResponse, RevokeDhcpLeaseResponse,
    ToggleDhcpRequest, UpdateDhcpConfigRequest,
};
use wardnet_common::dhcp::{DhcpConfig, DhcpLease, DhcpLeaseStatus, DhcpScope};

use crate::auth_context;
use crate::dns::service::DNS_ENABLED_KEY;
use crate::error::AppError;
use crate::event::EventPublisher;
use wardnet_common::event::WardnetEvent;

use wardnetd_data::repository::SystemConfigRepository;
use wardnetd_data::repository::{
    DeviceRepository, DhcpLeaseLogRow, DhcpLeaseRow, DhcpRepository, DhcpReservationRow,
    NetworkZoneRepository,
};

/// Public resolvers used when the Wardnet DNS server is off and the admin has
/// configured no upstream of their own. These are what the Pi's own resolver
/// forwards to; they are only ever handed to *clients* when there is no Wardnet
/// resolver for them to use. Never advertise the Pi in that state — nothing is
/// listening on :53 and the LAN would lose name resolution entirely.
const DEFAULT_UPSTREAM_DNS: [Ipv4Addr; 2] = [Ipv4Addr::new(1, 1, 1, 1), Ipv4Addr::new(8, 8, 8, 8)];

/// DHCP lease and reservation management.
///
/// Handles DHCP configuration, lease lifecycle, and static reservations.
/// All operations require admin authentication.
#[async_trait]
pub trait DhcpService: Send + Sync {
    /// Get the current DHCP configuration.
    async fn get_config(&self) -> Result<DhcpConfigResponse, AppError>;

    /// Update the DHCP pool configuration.
    async fn update_config(
        &self,
        req: UpdateDhcpConfigRequest,
    ) -> Result<DhcpConfigResponse, AppError>;

    /// Dry-run a pool-range change: report the active leases that would be
    /// revoked because their IP would fall outside the proposed pool (and are
    /// not pinned by a reservation). Mutates nothing.
    async fn preview_config(
        &self,
        req: PreviewDhcpConfigRequest,
    ) -> Result<PreviewDhcpConfigResponse, AppError>;

    /// Enable or disable the DHCP server.
    async fn toggle(&self, req: ToggleDhcpRequest) -> Result<DhcpConfigResponse, AppError>;

    /// List all active DHCP leases.
    async fn list_leases(&self) -> Result<ListDhcpLeasesResponse, AppError>;

    /// Revoke an active lease.
    async fn revoke_lease(&self, id: Uuid) -> Result<RevokeDhcpLeaseResponse, AppError>;

    /// List all static reservations.
    async fn list_reservations(&self) -> Result<ListDhcpReservationsResponse, AppError>;

    /// Create a new static reservation.
    async fn create_reservation(
        &self,
        req: CreateDhcpReservationRequest,
    ) -> Result<CreateDhcpReservationResponse, AppError>;

    /// Delete a static reservation.
    async fn delete_reservation(&self, id: Uuid)
    -> Result<DeleteDhcpReservationResponse, AppError>;

    /// Get DHCP server status (running, pool usage).
    async fn status(&self) -> Result<DhcpStatusResponse, AppError>;

    // ── Runtime methods (called by the DHCP server, not HTTP handlers) ──

    /// Assign a lease for a DHCP DISCOVER -- used by the DHCP server runtime.
    ///
    /// Checks reservations first (by MAC), otherwise allocates the first
    /// available IP in the pool range. Requires admin auth context.
    async fn assign_lease(&self, mac: &str, hostname: Option<&str>) -> Result<DhcpLease, AppError>;

    /// Renew/confirm a lease for a DHCP REQUEST -- used by the DHCP server runtime.
    ///
    /// Extends the existing lease if one is active, otherwise assigns a new one.
    /// `hostname` is the option-12 value from the DHCPREQUEST packet; when
    /// present and different from the stored value, the lease record is updated
    /// so downstream consumers (device registry, lease list UI) reflect the
    /// latest client-supplied identity. Requires admin auth context.
    async fn renew_lease(&self, mac: &str, hostname: Option<&str>) -> Result<DhcpLease, AppError>;

    /// Release a lease for a DHCP RELEASE -- used by the DHCP server runtime.
    ///
    /// Marks the active lease for the given MAC as released.
    /// Requires admin auth context.
    async fn release_lease(&self, mac: &str) -> Result<(), AppError>;

    /// Look up the active lease currently recorded for a MAC, if any.
    ///
    /// Used by the DHCP server runtime to authorize a DHCPRELEASE: because the
    /// wire `chaddr` is attacker-controllable it is never treated as proof of
    /// ownership (CWE-639). Per RFC 2131 a legitimate release is unicast from
    /// the client's own leased address, so the runtime releases only when the
    /// packet's UDP source address matches the IP recorded here. Requires admin
    /// auth context.
    ///
    /// Defaults to `Ok(None)` so stubs/mocks need not implement it; the real
    /// [`DhcpServiceImpl`] overrides it with the authenticated repository
    /// lookup. The default is fail-secure: a runtime backed by a
    /// non-overriding impl simply never authorises a release. Any real override
    /// MUST still open with `auth_context::require_admin()?`, like every other
    /// method on this trait — the fail-secure default is not licence to skip it.
    async fn active_lease(&self, _mac: &str) -> Result<Option<DhcpLease>, AppError> {
        Ok(None)
    }

    /// Expire all stale leases whose `lease_end` is in the past.
    ///
    /// Called periodically by the DHCP runner. Returns the number of expired leases.
    /// Requires admin auth context.
    async fn cleanup_expired(&self) -> Result<u64, AppError>;

    /// Load the current DHCP configuration (public for the DHCP server runtime).
    ///
    /// Requires admin auth context.
    async fn get_dhcp_config(&self) -> Result<DhcpConfig, AppError>;

    /// Resolve the effective DHCP scope for a MAC (public for the DHCP server
    /// runtime, which uses it to render the response options).
    ///
    /// Derives the scope from the device's Network Zone subnet (issue #737),
    /// falling back to the base pool when the zone has no subnet or when Wardnet
    /// is not authoritative. Requires admin auth context.
    async fn scope_for_mac(&self, mac: &str) -> Result<DhcpScope, AppError>;
}

/// Default implementation of [`DhcpService`].
pub struct DhcpServiceImpl {
    dhcp: Arc<dyn DhcpRepository>,
    system_config: Arc<dyn SystemConfigRepository>,
    events: Arc<dyn EventPublisher>,
    /// Devices, for MAC → Network Zone resolution when scoping a lease (#737).
    devices: Arc<dyn DeviceRepository>,
    /// Network Zones, for per-zone subnet resolution (#737).
    zones: Arc<dyn NetworkZoneRepository>,
    /// Wardnet's own LAN IP, auto-detected at startup.
    gateway_ip: Ipv4Addr,
}

impl DhcpServiceImpl {
    /// Create a new DHCP service with the given dependencies.
    pub fn new(
        dhcp: Arc<dyn DhcpRepository>,
        system_config: Arc<dyn SystemConfigRepository>,
        events: Arc<dyn EventPublisher>,
        devices: Arc<dyn DeviceRepository>,
        zones: Arc<dyn NetworkZoneRepository>,
        gateway_ip: Ipv4Addr,
    ) -> Self {
        Self {
            dhcp,
            system_config,
            events,
            devices,
            zones,
            gateway_ip,
        }
    }

    /// Normalise an option-12 hostname: empty / whitespace-only values count
    /// as absent (clients sometimes send empty strings to mean "no hostname").
    fn normalised_hostname(hostname: Option<&str>) -> Option<&str> {
        hostname.map(str::trim).filter(|h| !h.is_empty())
    }

    /// Load the current DHCP configuration from `system_config`.
    async fn load_config(&self) -> Result<DhcpConfig, AppError> {
        // Derive subnet-aware defaults from the detected gateway IP.
        let gw = self.gateway_ip.octets();
        let default_pool_start = format!("{}.{}.{}.100", gw[0], gw[1], gw[2]);
        let default_pool_end = format!("{}.{}.{}.250", gw[0], gw[1], gw[2]);

        let enabled = self
            .system_config
            .get("dhcp_enabled")
            .await
            .map_err(AppError::Internal)?
            .unwrap_or_else(|| "false".to_owned())
            == "true";

        let pool_start: Ipv4Addr = self
            .system_config
            .get("dhcp_pool_start")
            .await
            .map_err(AppError::Internal)?
            .unwrap_or(default_pool_start)
            .parse()
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid pool_start: {e}")))?;

        let pool_end: Ipv4Addr = self
            .system_config
            .get("dhcp_pool_end")
            .await
            .map_err(AppError::Internal)?
            .unwrap_or(default_pool_end)
            .parse()
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid pool_end: {e}")))?;

        let subnet_mask: Ipv4Addr = self
            .system_config
            .get("dhcp_subnet_mask")
            .await
            .map_err(AppError::Internal)?
            .unwrap_or_else(|| "255.255.255.0".to_owned())
            .parse()
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid subnet_mask: {e}")))?;

        let upstream_dns_json = self
            .system_config
            .get("dhcp_upstream_dns")
            .await
            .map_err(AppError::Internal)?
            .unwrap_or_else(|| {
                // Key absent (pre-seed install): same default the migration
                // seeds and `advertised_dns` falls back to — one source of
                // truth for the default resolver set.
                serde_json::to_string(&DEFAULT_UPSTREAM_DNS.map(|ip| ip.to_string()))
                    .expect("serialize default upstream DNS")
            });
        let upstream_dns: Vec<Ipv4Addr> = serde_json::from_str::<Vec<String>>(&upstream_dns_json)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid upstream_dns: {e}")))?
            .iter()
            .map(|s| s.parse())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid upstream_dns IP: {e}")))?;

        let lease_duration_secs: u32 = self
            .system_config
            .get("dhcp_lease_duration_secs")
            .await
            .map_err(AppError::Internal)?
            .unwrap_or_else(|| "86400".to_owned())
            .parse()
            .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid lease_duration_secs: {e}")))?;

        let router_ip_str = self
            .system_config
            .get("dhcp_router_ip")
            .await
            .map_err(AppError::Internal)?
            .unwrap_or_default();
        let router_ip = if router_ip_str.is_empty() {
            None
        } else {
            Some(
                router_ip_str
                    .parse()
                    .map_err(|e| AppError::Internal(anyhow::anyhow!("invalid router_ip: {e}")))?,
            )
        };

        Ok(DhcpConfig {
            enabled,
            gateway_ip: self.gateway_ip,
            pool_start,
            pool_end,
            subnet_mask,
            upstream_dns,
            lease_duration_secs,
            router_ip,
        })
    }

    /// Resolve the Network Zone a MAC leases from: the device's assigned zone
    /// for a known device, or the default-for-new zone for an unknown MAC.
    ///
    /// Returns `None` (with a debug log) on any repository error or missing
    /// zone so [`resolve_scope`](Self::resolve_scope) can degrade to the base
    /// scope — a lease is never failed over a zone lookup.
    async fn resolve_zone_for_mac(
        &self,
        mac: &str,
    ) -> Option<wardnet_common::network_zone::NetworkZone> {
        match self.devices.find_by_mac(mac).await {
            Ok(Some(dev)) => match self.zones.find_by_id(&dev.zone_id.to_string()).await {
                Ok(Some(zone)) => Some(zone),
                Ok(None) => {
                    tracing::debug!(mac, zone_id = %dev.zone_id, "device zone not found, using base DHCP scope");
                    None
                }
                Err(e) => {
                    tracing::debug!(mac, error = %e, "zone lookup failed, using base DHCP scope");
                    None
                }
            },
            Ok(None) => match self.zones.find_default_for_new().await {
                Ok(zone) => Some(zone),
                Err(e) => {
                    tracing::debug!(mac, error = %e, "default-for-new zone lookup failed, using base DHCP scope");
                    None
                }
            },
            Err(e) => {
                tracing::debug!(mac, error = %e, "device lookup failed, using base DHCP scope");
                None
            }
        }
    }

    /// Is the Wardnet DNS server switched on? Read straight from `system_config`
    /// rather than plumbed through `DnsService`, to avoid a service-to-service
    /// dependency on the DHCP lease path. Uses the shared [`DNS_ENABLED_KEY`]
    /// so this reader cannot drift from `DnsService::load_config`/`toggle`.
    /// Defaults to off when the key is absent, matching `DnsService`.
    async fn wardnet_dns_enabled(&self) -> Result<bool, AppError> {
        Ok(self
            .system_config
            .get(DNS_ENABLED_KEY)
            .await
            .map_err(AppError::Internal)?
            .unwrap_or_else(|| "false".to_owned())
            == "true")
    }

    /// The DNS servers advertised to clients in DHCP option 6.
    ///
    /// While the Wardnet DNS server is running, that is **always** the Pi — its
    /// address in whatever scope the client is leasing from. `dhcp_upstream_dns`
    /// is what the Pi's *own* resolver forwards to (seeded `1.1.1.1`/`8.8.8.8`);
    /// handing it to clients tells them to resolve via Cloudflare or Google
    /// directly, so they never ask the Pi and every blocklist, allowlist and
    /// parental control is silently bypassed. The DHCP config card already
    /// documents this contract to the admin — "the daemon will advertise
    /// Wardnet's own IP to clients regardless of what's saved here" — and shows
    /// "Wardnet DNS" in the read view; the daemon simply never honoured it.
    ///
    /// The stored list is not dead config: with the DNS server switched off
    /// there is nothing on the Pi to answer queries, so clients need a real
    /// resolver or they lose DNS entirely.
    ///
    /// Never returns an empty list. An empty `scope.dns` makes the DHCP server
    /// fall back to advertising the Pi — which, with the DNS server off, is a
    /// host with nothing listening on :53, so the whole LAN would lose name
    /// resolution. The stored list *can* be empty: the config card only exposes
    /// the field for editing while Wardnet DNS is off, which is exactly the
    /// state in which clearing it would be fatal.
    fn advertised_dns(
        wardnet_dns_enabled: bool,
        scope_gateway: Ipv4Addr,
        upstream: &[Ipv4Addr],
    ) -> Vec<Ipv4Addr> {
        if wardnet_dns_enabled {
            return vec![scope_gateway];
        }
        if upstream.is_empty() {
            // resolve_scope runs on every DHCP message; warn once, not once
            // per packet, or a lease storm floods the log with duplicates.
            static EMPTY_UPSTREAM_WARN: std::sync::Once = std::sync::Once::new();
            EMPTY_UPSTREAM_WARN.call_once(|| {
                tracing::warn!(
                    "Wardnet DNS is disabled and no upstream DNS is configured; advertising \
                     {DEFAULT_UPSTREAM_DNS:?} to DHCP clients so they keep working name resolution"
                );
            });
            return DEFAULT_UPSTREAM_DNS.to_vec();
        }
        upstream.to_vec()
    }

    /// Resolve the effective DHCP scope for a MAC (issue #737).
    ///
    /// The scope determines which subnet a device leases from and which options
    /// it is advertised. It is derived from the device's Network Zone: a zone
    /// with a configured subnet yields a per-zone scope (gateway at `.1`, pool
    /// `.10`–`broadcast-6`, DNS pointed at the gateway alias); otherwise the
    /// base pool from `system_config` is used.
    ///
    /// Zone lookup never fails a lease: any repository error, missing zone, or
    /// unparseable/too-small subnet degrades to the base scope with a log line.
    async fn resolve_scope(&self, mac: &str) -> Result<DhcpScope, AppError> {
        // Independent reads — overlap them rather than paying a second serial
        // round-trip to `system_config` on every lease.
        let (base, wardnet_dns) = tokio::try_join!(self.load_config(), self.wardnet_dns_enabled())?;
        let base_scope = DhcpScope {
            gateway_ip: base.gateway_ip,
            pool_start: base.pool_start,
            pool_end: base.pool_end,
            subnet_mask: base.subnet_mask,
            dns: Self::advertised_dns(wardnet_dns, base.gateway_ip, &base.upstream_dns),
            lease_duration_secs: base.lease_duration_secs,
            router_ip: base.router_ip,
            member_isolation: false,
            subnet_prefix: None,
        };

        // Resolve the device's zone; any failure degrades to the base scope.
        let Some(zone) = self.resolve_zone_for_mac(mac).await else {
            return Ok(base_scope);
        };

        // No per-zone subnet -> keep today's base behaviour.
        let Some(subnet) = zone.subnet.as_ref() else {
            return Ok(base_scope);
        };

        let net = match Ipv4Network::from_str(&subnet.cidr) {
            Ok(net) => net,
            Err(e) => {
                tracing::warn!(mac, zone = %zone.name, cidr = %subnet.cidr, error = %e, "unparseable zone subnet, using base DHCP scope");
                return Ok(base_scope);
            }
        };

        let gateway = crate::subnet::gateway_for(net);
        let Some((pool_start, pool_end)) = crate::subnet::pool_bounds(net) else {
            tracing::warn!(mac, zone = %zone.name, cidr = %subnet.cidr, "zone subnet too small for a DHCP pool, using base scope");
            return Ok(base_scope);
        };

        // Isolate-members zones advertise a /32 so peers appear off-link; the
        // real mask is still used for allocation (the pool stays inside the /N).
        let subnet_mask = if zone.member_isolation {
            Ipv4Addr::BROADCAST
        } else {
            net.mask()
        };

        Ok(DhcpScope {
            gateway_ip: gateway,
            pool_start,
            pool_end,
            subnet_mask,
            // The Pi's alias in this subnet, so per-zone DNS filtering still
            // reaches the Pi. With the DNS server off there is nothing there to
            // answer, so fall back to the upstream list like the base scope.
            //
            // Known limit: a WAN-forbidden zone's egress gate drops direct
            // client→public-resolver :53 traffic (only DNS *to the Pi* is
            // exempted), so with Wardnet DNS off such zones still have no
            // working resolver — same as before this change, when they were
            // handed the dead Pi alias. Tracked in #898.
            dns: Self::advertised_dns(wardnet_dns, gateway, &base.upstream_dns),
            lease_duration_secs: base.lease_duration_secs,
            // The gateway alias is the only router for a zone subnet.
            router_ip: None,
            member_isolation: zone.member_isolation,
            subnet_prefix: Some(net.prefix()),
        })
    }

    /// Compute the total number of IPs in the pool.
    fn pool_size(start: Ipv4Addr, end: Ipv4Addr) -> u64 {
        let s = u32::from(start);
        let e = u32::from(end);
        if e >= s { u64::from(e - s + 1) } else { 0 }
    }

    /// Find the first available IP in the given pool range that is not
    /// currently assigned to an active lease or a static reservation.
    async fn find_available_ip(
        &self,
        pool_start: Ipv4Addr,
        pool_end: Ipv4Addr,
    ) -> Result<Ipv4Addr, AppError> {
        let active_leases = self
            .dhcp
            .list_active_leases()
            .await
            .map_err(AppError::Internal)?;
        let reservations = self
            .dhcp
            .list_reservations()
            .await
            .map_err(AppError::Internal)?;

        let used_ips: std::collections::HashSet<Ipv4Addr> = active_leases
            .iter()
            .map(|l| l.ip_address)
            .chain(reservations.iter().map(|r| r.ip_address))
            .collect();

        let start = u32::from(pool_start);
        let end = u32::from(pool_end);

        for ip_num in start..=end {
            let candidate = Ipv4Addr::from(ip_num);
            if !used_ips.contains(&candidate) {
                return Ok(candidate);
            }
        }

        Err(AppError::Conflict(
            "DHCP pool exhausted - no available IP addresses".to_owned(),
        ))
    }

    /// Whether `ip` falls within the given dynamic pool range.
    fn ip_in_pool(ip: Ipv4Addr, pool_start: Ipv4Addr, pool_end: Ipv4Addr) -> bool {
        let n = u32::from(ip);
        n >= u32::from(pool_start) && n <= u32::from(pool_end)
    }

    /// Parse and validate a proposed DHCP pool range: both endpoints must be
    /// valid IPv4 addresses and `pool_end >= pool_start`. Shared by
    /// `update_config` and `preview_config` so the rule lives in one place.
    fn parse_pool_range(
        pool_start: &str,
        pool_end: &str,
    ) -> Result<(Ipv4Addr, Ipv4Addr), AppError> {
        let start: Ipv4Addr = pool_start
            .parse()
            .map_err(|_| AppError::BadRequest("invalid pool_start IP address".to_owned()))?;
        let end: Ipv4Addr = pool_end
            .parse()
            .map_err(|_| AppError::BadRequest("invalid pool_end IP address".to_owned()))?;
        if u32::from(end) < u32::from(start) {
            return Err(AppError::BadRequest(
                "pool_end must be >= pool_start".to_owned(),
            ));
        }
        // LAN addressing must be RFC 1918 private — a public range would hand
        // out addresses that belong to real internet hosts.
        if !start.is_private() {
            return Err(AppError::BadRequest(format!(
                "pool_start must be {}",
                wardnet_common::net::PRIVATE_RANGE_HINT
            )));
        }
        if !end.is_private() {
            return Err(AppError::BadRequest(format!(
                "pool_end must be {}",
                wardnet_common::net::PRIVATE_RANGE_HINT
            )));
        }
        Ok((start, end))
    }

    /// Look up the active lease for `mac` and confirm it still reflects
    /// the current configuration. A lease is valid when either a reservation
    /// for the same MAC points to its IP, or no reservation exists for the
    /// MAC and the IP sits inside the dynamic pool.
    ///
    /// An invalid lease is "orphaned" — typically because its reservation
    /// was deleted or changed, or the pool was narrowed away from it. The
    /// helper marks it expired so the caller can fall through to a fresh
    /// allocation; returning the stale lease as-is would pin the device to
    /// an IP the configuration no longer justifies.
    ///
    /// Returns `Some(lease)` when the existing lease is valid, `None` when
    /// there's no active lease or it was just expired.
    async fn lease_if_still_valid(
        &self,
        mac: &str,
        pool_start: Ipv4Addr,
        pool_end: Ipv4Addr,
    ) -> Result<Option<DhcpLease>, AppError> {
        let Some(existing) = self
            .dhcp
            .find_active_lease_by_mac(mac)
            .await
            .map_err(AppError::Internal)?
        else {
            return Ok(None);
        };

        let reservation = self
            .dhcp
            .find_reservation_by_mac(mac)
            .await
            .map_err(AppError::Internal)?;

        let still_valid = match &reservation {
            Some(r) => r.ip_address == existing.ip_address,
            None => Self::ip_in_pool(existing.ip_address, pool_start, pool_end),
        };

        if still_valid {
            return Ok(Some(existing));
        }

        let detail = match &reservation {
            Some(r) => format!("superseded by reservation for {}", r.ip_address),
            None => format!(
                "orphaned: ip {} has no reservation and is outside pool {pool_start}-{pool_end}",
                existing.ip_address
            ),
        };
        tracing::info!(
            mac,
            old_ip = %existing.ip_address,
            "expiring stale lease so a fresh allocation can run"
        );
        self.dhcp
            .update_lease_status(&existing.id.to_string(), "expired")
            .await
            .map_err(AppError::Internal)?;
        self.dhcp
            .insert_lease_log(&DhcpLeaseLogRow {
                lease_id: existing.id.to_string(),
                mac_address: existing.mac_address.clone(),
                event_type: "expired".to_owned(),
                details: Some(detail),
            })
            .await
            .map_err(AppError::Internal)?;
        Ok(None)
    }

    /// Active leases whose IP falls outside `[pool_start, pool_end]` and are
    /// not pinned by a reservation for the same MAC. A reservation is a
    /// deliberate static pin, so it survives a pool change even when its IP
    /// sits outside the dynamic pool — mirroring `lease_if_still_valid`.
    async fn leases_outside_pool(
        &self,
        pool_start: Ipv4Addr,
        pool_end: Ipv4Addr,
    ) -> Result<Vec<DhcpLease>, AppError> {
        let leases = self
            .dhcp
            .list_active_leases()
            .await
            .map_err(AppError::Internal)?;
        let reservations = self
            .dhcp
            .list_reservations()
            .await
            .map_err(AppError::Internal)?;

        // Map each reserved MAC to its pinned IP. Casing is canonicalised at
        // the repository boundary (issue #312) so a direct comparison is safe.
        let reserved: std::collections::HashMap<String, Ipv4Addr> = reservations
            .into_iter()
            .map(|r| (r.mac_address, r.ip_address))
            .collect();

        let start = u32::from(pool_start);
        let end = u32::from(pool_end);
        Ok(leases
            .into_iter()
            .filter(|l| {
                let n = u32::from(l.ip_address);
                let in_pool = n >= start && n <= end;
                let pinned = reserved.get(&l.mac_address) == Some(&l.ip_address);
                !in_pool && !pinned
            })
            .collect())
    }

    /// Expire every active lease outside `[pool_start, pool_end]` (skipping
    /// reservation-pinned ones) and write an audit-log row for each. Returns
    /// the number of leases revoked.
    async fn revoke_leases_outside_pool(
        &self,
        pool_start: Ipv4Addr,
        pool_end: Ipv4Addr,
    ) -> Result<u64, AppError> {
        let stranded = self.leases_outside_pool(pool_start, pool_end).await?;
        let mut revoked = 0u64;
        for lease in stranded {
            self.dhcp
                .update_lease_status(&lease.id.to_string(), "expired")
                .await
                .map_err(AppError::Internal)?;
            self.dhcp
                .insert_lease_log(&DhcpLeaseLogRow {
                    lease_id: lease.id.to_string(),
                    mac_address: lease.mac_address.clone(),
                    event_type: "expired".to_owned(),
                    details: Some(format!(
                        "out of range after pool change: ip {} outside {pool_start}-{pool_end}",
                        lease.ip_address
                    )),
                })
                .await
                .map_err(AppError::Internal)?;
            let mac = &lease.mac_address;
            let ip = lease.ip_address;
            let lease_id = lease.id;
            tracing::info!(
                %mac,
                %ip,
                %lease_id,
                "expiring lease stranded outside new DHCP pool: mac={mac}, ip={ip}, lease_id={lease_id}"
            );
            revoked += 1;
        }
        Ok(revoked)
    }
}

#[async_trait]
impl DhcpService for DhcpServiceImpl {
    async fn get_config(&self) -> Result<DhcpConfigResponse, AppError> {
        auth_context::require_admin()?;
        let config = self.load_config().await?;
        Ok(DhcpConfigResponse { config })
    }

    async fn update_config(
        &self,
        req: UpdateDhcpConfigRequest,
    ) -> Result<DhcpConfigResponse, AppError> {
        auth_context::require_admin()?;

        // Validate IP addresses.
        let (pool_start, pool_end) = Self::parse_pool_range(&req.pool_start, &req.pool_end)?;
        let _subnet_mask: Ipv4Addr = req
            .subnet_mask
            .parse()
            .map_err(|_| AppError::BadRequest("invalid subnet_mask IP address".to_owned()))?;

        for dns in &req.upstream_dns {
            let _: Ipv4Addr = dns.parse().map_err(|_| {
                AppError::BadRequest(format!("invalid upstream DNS address: {dns}"))
            })?;
        }

        if let Some(ref router_ip) = req.router_ip {
            let parsed: Ipv4Addr = router_ip
                .parse()
                .map_err(|_| AppError::BadRequest("invalid router_ip address".to_owned()))?;
            if !parsed.is_private() {
                return Err(AppError::BadRequest(format!(
                    "router_ip must be {}",
                    wardnet_common::net::PRIVATE_RANGE_HINT
                )));
            }
        }

        // Store validated config.
        self.system_config
            .set("dhcp_pool_start", &req.pool_start)
            .await
            .map_err(AppError::Internal)?;
        self.system_config
            .set("dhcp_pool_end", &req.pool_end)
            .await
            .map_err(AppError::Internal)?;
        self.system_config
            .set("dhcp_subnet_mask", &req.subnet_mask)
            .await
            .map_err(AppError::Internal)?;
        let dns_json =
            serde_json::to_string(&req.upstream_dns).map_err(|e| AppError::Internal(e.into()))?;
        self.system_config
            .set("dhcp_upstream_dns", &dns_json)
            .await
            .map_err(AppError::Internal)?;
        self.system_config
            .set(
                "dhcp_lease_duration_secs",
                &req.lease_duration_secs.to_string(),
            )
            .await
            .map_err(AppError::Internal)?;
        self.system_config
            .set("dhcp_router_ip", req.router_ip.as_deref().unwrap_or(""))
            .await
            .map_err(AppError::Internal)?;

        // Invalidate the passively-learned upstream router MAC. Any
        // change to `dhcp_router_ip` (including a no-op rewrite) means
        // the previously stored MAC may no longer correspond to the
        // gateway that lives at this IP. Discovery will repopulate
        // `garp_router_mac` next time it observes a packet from the
        // (possibly new) router IP. See issue #213, decision 1.
        self.system_config
            .set("garp_router_mac", "")
            .await
            .map_err(AppError::Internal)?;

        // Revoke leases stranded outside the new pool (issue #227). A device
        // holding an out-of-range IP would otherwise keep it — with routing
        // rules still targeting the old address — until its lease happened to
        // expire. Expiring them now, plus the DHCPNAK path in `evaluate_renewal`,
        // forces each device to re-acquire an in-range lease at its next
        // renewal. Reservations are left untouched: a static pin is deliberate
        // even when it sits outside the dynamic pool.
        let revoked = self
            .revoke_leases_outside_pool(pool_start, pool_end)
            .await?;
        if revoked > 0 {
            tracing::info!(
                revoked,
                %pool_start,
                %pool_end,
                "revoked {revoked} DHCP leases outside new pool range {pool_start}-{pool_end}"
            );
        }

        let config = self.load_config().await?;
        Ok(DhcpConfigResponse { config })
    }

    async fn preview_config(
        &self,
        req: PreviewDhcpConfigRequest,
    ) -> Result<PreviewDhcpConfigResponse, AppError> {
        auth_context::require_admin()?;

        let (pool_start, pool_end) = Self::parse_pool_range(&req.pool_start, &req.pool_end)?;
        let affected = self.leases_outside_pool(pool_start, pool_end).await?;
        Ok(PreviewDhcpConfigResponse { affected })
    }

    async fn toggle(&self, req: ToggleDhcpRequest) -> Result<DhcpConfigResponse, AppError> {
        auth_context::require_admin()?;

        self.system_config
            .set("dhcp_enabled", if req.enabled { "true" } else { "false" })
            .await
            .map_err(AppError::Internal)?;

        let config = self.load_config().await?;
        Ok(DhcpConfigResponse { config })
    }

    async fn list_leases(&self) -> Result<ListDhcpLeasesResponse, AppError> {
        auth_context::require_admin()?;
        let leases = self
            .dhcp
            .list_active_leases()
            .await
            .map_err(AppError::Internal)?;
        Ok(ListDhcpLeasesResponse { leases })
    }

    async fn revoke_lease(&self, id: Uuid) -> Result<RevokeDhcpLeaseResponse, AppError> {
        auth_context::require_admin()?;

        let lease = self
            .dhcp
            .find_lease_by_id(&id.to_string())
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::NotFound(format!("lease {id} not found")))?;

        if lease.status != DhcpLeaseStatus::Active {
            return Err(AppError::BadRequest("lease is not active".to_owned()));
        }

        self.dhcp
            .update_lease_status(&id.to_string(), "released")
            .await
            .map_err(AppError::Internal)?;

        self.dhcp
            .insert_lease_log(&DhcpLeaseLogRow {
                lease_id: id.to_string(),
                mac_address: lease.mac_address.clone(),
                event_type: "released".to_owned(),
                details: Some("admin revoked".to_owned()),
            })
            .await
            .map_err(AppError::Internal)?;

        Ok(RevokeDhcpLeaseResponse {
            message: format!("lease {id} revoked"),
        })
    }

    async fn list_reservations(&self) -> Result<ListDhcpReservationsResponse, AppError> {
        auth_context::require_admin()?;
        let reservations = self
            .dhcp
            .list_reservations()
            .await
            .map_err(AppError::Internal)?;
        Ok(ListDhcpReservationsResponse { reservations })
    }

    async fn create_reservation(
        &self,
        req: CreateDhcpReservationRequest,
    ) -> Result<CreateDhcpReservationResponse, AppError> {
        auth_context::require_admin()?;

        // Casing is canonicalised at the repository boundary (issue #312);
        // use the request value as-is.
        let mac = req.mac_address.as_str();

        // Validate IP.
        let _: Ipv4Addr = req
            .ip_address
            .parse()
            .map_err(|_| AppError::BadRequest("invalid ip_address".to_owned()))?;

        // Check for duplicate MAC.
        if self
            .dhcp
            .find_reservation_by_mac(mac)
            .await
            .map_err(AppError::Internal)?
            .is_some()
        {
            return Err(AppError::Conflict(format!(
                "reservation for MAC {mac} already exists",
            )));
        }

        // Check for duplicate IP.
        if self
            .dhcp
            .find_reservation_by_ip(&req.ip_address)
            .await
            .map_err(AppError::Internal)?
            .is_some()
        {
            return Err(AppError::Conflict(format!(
                "reservation for IP {} already exists",
                req.ip_address
            )));
        }

        let id = Uuid::new_v4();
        let row = DhcpReservationRow {
            id: id.to_string(),
            mac_address: mac.to_owned(),
            ip_address: req.ip_address.clone(),
            hostname: req.hostname.clone(),
            description: req.description.clone(),
        };

        self.dhcp
            .insert_reservation(&row)
            .await
            .map_err(AppError::Internal)?;

        // Pinning an address is an admin configuration act, so the device it
        // names is promoted to managed (issue #1181) and becomes exempt from
        // the retention prune.
        //
        // Reservations are MAC-keyed and may legitimately pre-register a MAC no
        // device row exists for yet; that lookup simply misses and nothing is
        // promoted. Nothing is orphaned in that case either — the reservation
        // row is keyed by MAC, not by `devices.id`, so it survives and keeps
        // working regardless of the device row's fate.
        if let Some(device) = self
            .devices
            .find_by_mac(mac)
            .await
            .map_err(AppError::Internal)?
        {
            self.devices
                .set_managed(&device.id.to_string(), true)
                .await
                .map_err(AppError::Internal)?;
        }

        let reservation = self
            .dhcp
            .find_reservation_by_mac(mac)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| {
                AppError::Internal(anyhow::anyhow!("reservation not found after insert"))
            })?;

        Ok(CreateDhcpReservationResponse {
            reservation,
            message: "reservation created".to_owned(),
        })
    }

    async fn delete_reservation(
        &self,
        id: Uuid,
    ) -> Result<DeleteDhcpReservationResponse, AppError> {
        auth_context::require_admin()?;

        let reservations = self
            .dhcp
            .list_reservations()
            .await
            .map_err(AppError::Internal)?;
        if !reservations.iter().any(|r| r.id == id) {
            return Err(AppError::NotFound(format!("reservation {id} not found")));
        }

        self.dhcp
            .delete_reservation(&id.to_string())
            .await
            .map_err(AppError::Internal)?;

        Ok(DeleteDhcpReservationResponse {
            message: format!("reservation {id} deleted"),
        })
    }

    async fn status(&self) -> Result<DhcpStatusResponse, AppError> {
        auth_context::require_admin()?;

        let config = self.load_config().await?;
        let leases = self
            .dhcp
            .list_active_leases()
            .await
            .map_err(AppError::Internal)?;
        let reservations = self
            .dhcp
            .list_reservations()
            .await
            .map_err(AppError::Internal)?;
        let pool_total = Self::pool_size(config.pool_start, config.pool_end);

        // Count reservations whose IP falls within the pool range.
        let reservations_in_pool = reservations
            .iter()
            .filter(|r| {
                let ip = u32::from(r.ip_address);
                ip >= u32::from(config.pool_start) && ip <= u32::from(config.pool_end)
            })
            .count() as u64;
        let pool_used = leases.len() as u64 + reservations_in_pool;

        Ok(DhcpStatusResponse {
            enabled: config.enabled,
            running: config.enabled, // For now, running == enabled. DhcpRunner will refine this later.
            active_lease_count: leases.len() as u64,
            pool_total,
            pool_used,
        })
    }

    #[allow(clippy::too_many_lines)]
    async fn assign_lease(&self, mac: &str, hostname: Option<&str>) -> Result<DhcpLease, AppError> {
        auth_context::require_admin()?;
        // Casing is canonicalised at the repository boundary (issue #312);
        // pass the runtime-supplied MAC through verbatim.
        let hostname = Self::normalised_hostname(hostname);

        // Resolve the device's per-zone DHCP scope once (#737); allocation and
        // lease-validity checks below all run against this scope's pool.
        let scope = self.resolve_scope(mac).await?;

        // Reuse an existing active lease when it still reflects the current
        // configuration. An orphaned lease (reservation removed or pool
        // narrowed away from the IP — including a device that moved zones) is
        // expired inside the helper so the fall-through allocates a fresh IP
        // inside the resolved scope instead of pinning the device.
        if let Some(mut existing) = self
            .lease_if_still_valid(mac, scope.pool_start, scope.pool_end)
            .await?
        {
            // Detect a real option-12 change so we only update + emit an
            // event when something downstream consumers care about has
            // actually moved. Plain DISCOVER retransmits with the same
            // hostname produce no work.
            let hostname_changed = match hostname {
                Some(new_h) => existing.hostname.as_deref() != Some(new_h),
                None => false,
            };

            if hostname_changed {
                let new_h = hostname.expect("hostname_changed implies Some");
                let old = existing.hostname.clone();
                self.dhcp
                    .update_lease_hostname(&existing.id.to_string(), Some(new_h))
                    .await
                    .map_err(AppError::Internal)?;
                self.dhcp
                    .insert_lease_log(&DhcpLeaseLogRow {
                        lease_id: existing.id.to_string(),
                        mac_address: mac.to_owned(),
                        event_type: "assigned".to_owned(),
                        details: Some(format!(
                            "hostname updated: {} -> {new_h}",
                            old.as_deref().unwrap_or("<none>")
                        )),
                    })
                    .await
                    .map_err(AppError::Internal)?;
                tracing::info!(
                    mac,
                    lease_id = %existing.id,
                    old = old.as_deref().unwrap_or("<none>"),
                    new = new_h,
                    "DHCP lease hostname updated on cached lease"
                );
                existing.hostname = Some(new_h.to_owned());
            }
            tracing::debug!(mac, ip = %existing.ip_address, "reusing existing active lease");

            // Only emit an event when state actually changed — quiets the
            // listener for routine retransmits while still fanning out a
            // genuine rename.
            if hostname_changed {
                self.events.publish(WardnetEvent::DhcpLeaseAssigned {
                    lease_id: existing.id,
                    mac: mac.to_owned(),
                    ip: existing.ip_address.to_string(),
                    hostname: existing.hostname.clone(),
                    timestamp: chrono::Utc::now(),
                });
            }

            return Ok(existing);
        }

        // Check for a static reservation first. A reservation is honoured only
        // when it is compatible with the device's resolved scope: for a zone
        // subnet (`subnet_prefix = Some(p)`) the reserved IP must fall inside
        // that subnet, otherwise the client would be handed an address in one
        // subnet with a gateway/mask for another. Such a reservation is skipped
        // (with a warn) and the device gets a pool IP instead. The base scope
        // (`subnet_prefix = None`) honours reservations exactly as before.
        let reservation = self
            .dhcp
            .find_reservation_by_mac(mac)
            .await
            .map_err(AppError::Internal)?
            .filter(|reservation| {
                let compatible = match scope.subnet_prefix {
                    Some(prefix) => Ipv4Network::new(scope.gateway_ip, prefix)
                        .map_or(true, |net| net.contains(reservation.ip_address)),
                    None => true,
                };
                if !compatible {
                    tracing::warn!(
                        mac,
                        reservation_ip = %reservation.ip_address,
                        gateway = %scope.gateway_ip,
                        "static reservation is outside the device's zone subnet, ignoring; allocating from pool"
                    );
                }
                compatible
            });

        let ip = if let Some(reservation) = reservation {
            tracing::info!(mac, ip = %reservation.ip_address, "using static reservation");
            reservation.ip_address
        } else {
            // Find first available IP in the resolved scope's pool range.
            self.find_available_ip(scope.pool_start, scope.pool_end)
                .await?
        };

        let now = chrono::Utc::now();
        let lease_end = now + chrono::Duration::seconds(i64::from(scope.lease_duration_secs));
        let id = Uuid::new_v4();

        let row = DhcpLeaseRow {
            id: id.to_string(),
            mac_address: mac.to_owned(),
            ip_address: ip.to_string(),
            hostname: hostname.map(ToOwned::to_owned),
            lease_start: now.to_rfc3339(),
            lease_end: lease_end.to_rfc3339(),
            status: "active".to_owned(),
            device_id: None,
        };

        self.dhcp
            .insert_lease(&row)
            .await
            .map_err(AppError::Internal)?;

        self.dhcp
            .insert_lease_log(&DhcpLeaseLogRow {
                lease_id: id.to_string(),
                mac_address: mac.to_owned(),
                event_type: "assigned".to_owned(),
                details: hostname.map(|h| format!("hostname: {h}")),
            })
            .await
            .map_err(AppError::Internal)?;

        tracing::info!(mac, %ip, lease_id = %id, "DHCP lease assigned");

        self.events.publish(WardnetEvent::DhcpLeaseAssigned {
            lease_id: id,
            mac: mac.to_owned(),
            ip: ip.to_string(),
            hostname: hostname.map(ToOwned::to_owned),
            timestamp: chrono::Utc::now(),
        });

        // Return the newly created lease.
        self.dhcp
            .find_lease_by_id(&id.to_string())
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("lease not found after insert")))
    }

    async fn renew_lease(&self, mac: &str, hostname: Option<&str>) -> Result<DhcpLease, AppError> {
        auth_context::require_admin()?;
        // Casing is canonicalised at the repository boundary (issue #312).
        let hostname = Self::normalised_hostname(hostname);

        // Resolve the device's per-zone DHCP scope once (#737).
        let scope = self.resolve_scope(mac).await?;

        // `lease_if_still_valid` collapses two migration cases into one path:
        // a reservation that no longer matches the lease's IP, and a lease
        // whose IP is no longer in any pool/reservation (orphaned by a
        // reservation deletion, pool change, or a zone move that shifted the
        // device to a new subnet). Either way the stale lease is expired
        // in-place and we fall through to assign_lease, which closes the window
        // where the old IP could be re-handed while the original device still
        // holds it.
        if let Some(mut existing) = self
            .lease_if_still_valid(mac, scope.pool_start, scope.pool_end)
            .await?
        {
            let new_end = chrono::Utc::now()
                + chrono::Duration::seconds(i64::from(scope.lease_duration_secs));

            self.dhcp
                .renew_lease(&existing.id.to_string(), &new_end.to_rfc3339())
                .await
                .map_err(AppError::Internal)?;

            // Refresh the stored option-12 value when the client sent a new one.
            let hostname_changed = match hostname {
                Some(new_h) => existing.hostname.as_deref() != Some(new_h),
                None => false,
            };
            if hostname_changed {
                let new_h = hostname.expect("hostname_changed implies Some");
                let old = existing.hostname.clone();
                self.dhcp
                    .update_lease_hostname(&existing.id.to_string(), Some(new_h))
                    .await
                    .map_err(AppError::Internal)?;
                tracing::info!(
                    mac,
                    lease_id = %existing.id,
                    old = old.as_deref().unwrap_or("<none>"),
                    new = new_h,
                    "DHCP lease hostname updated during renewal"
                );
                existing.hostname = Some(new_h.to_owned());
            }

            // Roll the hostname change into the renewal audit row when
            // applicable so a single log entry captures everything that
            // happened on this REQUEST.
            let renewal_details = if hostname_changed {
                let new_h = hostname.expect("hostname_changed implies Some");
                Some(format!("new expiry: {new_end}; hostname: {new_h}"))
            } else {
                Some(format!("new expiry: {new_end}"))
            };
            self.dhcp
                .insert_lease_log(&DhcpLeaseLogRow {
                    lease_id: existing.id.to_string(),
                    mac_address: mac.to_owned(),
                    event_type: "renewed".to_owned(),
                    details: renewal_details,
                })
                .await
                .map_err(AppError::Internal)?;

            tracing::info!(mac, lease_id = %existing.id, %new_end, "DHCP lease renewed");

            self.events.publish(WardnetEvent::DhcpLeaseRenewed {
                lease_id: existing.id,
                mac: mac.to_owned(),
                ip: existing.ip_address.to_string(),
                hostname: existing.hostname.clone(),
                new_expiry: new_end,
                timestamp: chrono::Utc::now(),
            });

            self.dhcp
                .find_lease_by_id(&existing.id.to_string())
                .await
                .map_err(AppError::Internal)?
                .ok_or_else(|| AppError::Internal(anyhow::anyhow!("lease not found after renew")))
        } else {
            // No valid active lease (none, or just expired as orphan) — assign
            // fresh. Forward the option-12 hostname so it isn't dropped on
            // the renew→assign fall-through.
            tracing::info!(mac, "no active lease for renewal, assigning new lease");
            self.assign_lease(mac, hostname).await
        }
    }

    async fn release_lease(&self, mac: &str) -> Result<(), AppError> {
        auth_context::require_admin()?;
        // Casing is canonicalised at the repository boundary (issue #312).

        let lease = self
            .dhcp
            .find_active_lease_by_mac(mac)
            .await
            .map_err(AppError::Internal)?;

        if let Some(lease) = lease {
            self.dhcp
                .update_lease_status(&lease.id.to_string(), "released")
                .await
                .map_err(AppError::Internal)?;

            self.dhcp
                .insert_lease_log(&DhcpLeaseLogRow {
                    lease_id: lease.id.to_string(),
                    mac_address: lease.mac_address.clone(),
                    event_type: "released".to_owned(),
                    details: Some("client DHCPRELEASE".to_owned()),
                })
                .await
                .map_err(AppError::Internal)?;

            tracing::info!(mac, lease_id = %lease.id, "DHCP lease released");
        } else {
            tracing::debug!(mac, "release requested but no active lease found");
        }

        Ok(())
    }

    async fn active_lease(&self, mac: &str) -> Result<Option<DhcpLease>, AppError> {
        auth_context::require_admin()?;
        // Casing is canonicalised at the repository boundary (issue #312).
        self.dhcp
            .find_active_lease_by_mac(mac)
            .await
            .map_err(AppError::Internal)
    }

    async fn cleanup_expired(&self) -> Result<u64, AppError> {
        auth_context::require_admin()?;

        let count = self
            .dhcp
            .expire_stale_leases()
            .await
            .map_err(AppError::Internal)?;

        if count > 0 {
            tracing::info!(count, "expired stale DHCP leases");
        }

        Ok(count)
    }

    async fn get_dhcp_config(&self) -> Result<DhcpConfig, AppError> {
        auth_context::require_admin()?;
        self.load_config().await
    }

    async fn scope_for_mac(&self, mac: &str) -> Result<DhcpScope, AppError> {
        auth_context::require_admin()?;
        self.resolve_scope(mac).await
    }
}
