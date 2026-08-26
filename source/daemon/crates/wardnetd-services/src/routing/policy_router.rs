use std::net::Ipv4Addr;

use async_trait::async_trait;

/// Abstraction over policy routing operations (routing tables, source-based rules, forwarding).
///
/// Manages per-tunnel routing tables and per-device source-based routing rules so that
/// each device's traffic is directed through its assigned tunnel. Enables mocking in tests.
/// The production implementation uses the `ip` and `sysctl` commands.
#[async_trait]
pub trait PolicyRouter: Send + Sync {
    /// Enable IPv4 forwarding on the host.
    async fn enable_ip_forwarding(&self) -> anyhow::Result<()>;

    /// Add a default route through the given interface in the specified routing table.
    async fn add_route_table(&self, interface: &str, table: u32) -> anyhow::Result<()>;

    /// Remove the default route from the specified routing table.
    async fn remove_route_table(&self, table: u32) -> anyhow::Result<()>;

    /// Check whether a default route exists in the specified routing table.
    async fn has_route_table(&self, table: u32) -> anyhow::Result<bool>;

    /// Add a source-based routing rule that directs traffic from `src_ip`
    /// through the given table, at `priority`.
    ///
    /// The priority must be passed explicitly rather than left to the kernel:
    /// `fib_default_rule_pref` derives it from whatever is already installed,
    /// which lets a narrower carve-out added earlier push this rule ahead of
    /// itself and defeat its own purpose.
    async fn add_ip_rule(&self, src_ip: &str, table: u32, priority: u32) -> anyhow::Result<()>;

    /// Remove the source-based routing rule for `src_ip`, `table` and
    /// `priority`. The priority is part of the match so this can never delete a
    /// carve-out that happens to share the same source and table.
    async fn remove_ip_rule(&self, src_ip: &str, table: u32, priority: u32) -> anyhow::Result<()>;

    /// List all Wardnet-managed routing rules (tables >= 100).
    ///
    /// Returns tuples of (`source_ip`, `table_number`, `priority`). The priority
    /// is reported so reconcile can recognise a rule left at a priority this
    /// version no longer writes and rebuild it, rather than seeing a rule at the
    /// right source and table and assuming it is correct.
    async fn list_wardnet_rules(&self) -> anyhow::Result<Vec<(String, u32, u32)>>;

    // --- Cross-zone switchback carve-outs (pass-switchback) ---
    //
    // A tunnel-bound device carries an `ip rule from <ip> lookup <tunnelTable>`
    // that captures ALL its traffic — including cross-zone LAN traffic that a
    // casting exception is meant to allow. These carve-out rules re-assert the
    // `main` table for specific cross-zone destinations at a priority band
    // ABOVE the kernel's per-tunnel source rules, so the cast packet reaches the
    // forward chain (and the zone_isolation allows) instead of the tunnel.

    /// Install `ip rule from <src_ip> to <dst_cidr> lookup main priority
    /// <priority>` so `src_ip`'s traffic to `dst_cidr` uses the `main` table
    /// (254) instead of its per-tunnel table. Idempotent: an already-present
    /// rule is treated as success.
    async fn add_switchback_rule(
        &self,
        src_ip: &str,
        dst_cidr: &str,
        priority: u32,
    ) -> anyhow::Result<()>;

    /// Remove the switchback rule for `(src_ip, dst_cidr, priority)`. Idempotent:
    /// a missing rule is treated as success.
    async fn remove_switchback_rule(
        &self,
        src_ip: &str,
        dst_cidr: &str,
        priority: u32,
    ) -> anyhow::Result<()>;

    /// List every rule at the switchback priority band as
    /// `(src_ip, dst_cidr, priority)`, so the routing service can prune stale
    /// carve-outs on reconcile.
    async fn list_switchback_rules(&self) -> anyhow::Result<Vec<(String, String, u32)>>;

    // --- Domain routing (per-domain routing profiles) ---
    //
    // When the local DNS server resolves a domain matched by a device's routing
    // profile, the destination IP is pinned to a specific routing table for that
    // device: `ip rule from <device_ip>/32 to <resolved_ip>/32 lookup <table>
    // priority <priority>`. `table` is a per-tunnel table (route the domain
    // through that tunnel) or `main` (254, carve the domain out of the device's
    // tunnel back to the WAN). The priority sits above the kernel's per-tunnel
    // source rules so the per-destination decision wins, and in its own band
    // (distinct from switchback's) so the two never prune each other.

    /// Install `ip rule from <src_ip>/32 to <dst_ip>/32 lookup <table> priority
    /// <priority>`. Idempotent: an already-present rule is treated as success.
    async fn add_domain_route_rule(
        &self,
        src_ip: &str,
        dst_ip: &str,
        table: u32,
        priority: u32,
    ) -> anyhow::Result<()>;

    /// Remove the domain-route rule for `(src_ip, dst_ip, table, priority)`.
    /// Idempotent: a missing rule is treated as success.
    async fn remove_domain_route_rule(
        &self,
        src_ip: &str,
        dst_ip: &str,
        table: u32,
        priority: u32,
    ) -> anyhow::Result<()>;

    /// List every rule at the given domain-route priority as
    /// `(src_ip, dst_ip, table)`, so the routing service can prune expired or
    /// orphaned per-destination rules on reconcile.
    async fn list_domain_route_rules(
        &self,
        priority: u32,
    ) -> anyhow::Result<Vec<(String, String, u32)>>;

    /// Flush conntrack entries whose original source matches `src_ip`.
    ///
    /// Changing an `ip rule` only affects *new* flows — existing connections
    /// stay pinned to their original route via conntrack/NAT state. Without
    /// this, switching a device between tunnels (or back to direct) has no
    /// visible effect until existing flows time out. Non-fatal on failure.
    async fn flush_conntrack(&self, src_ip: &str) -> anyhow::Result<()>;

    /// Flush the kernel's route cache.
    ///
    /// When traffic has been flowing via one interface, the kernel may cache
    /// next-hop decisions (including ICMP-redirect hints) that keep pointing
    /// at the old path even after `ip rule`/route changes. Without flushing
    /// this, new packets can get misrouted for a brief window. Non-fatal.
    async fn flush_route_cache(&self) -> anyhow::Result<()>;

    /// Verify that required routing tools are available on the system.
    async fn check_tools_available(&self) -> anyhow::Result<()>;

    // --- Network-Zone DHCP-mode surface (issue #737) ---
    //
    // These give the recorded-only per-zone `subnet` / `member_isolation` fields
    // teeth when Wardnet owns DHCP: a per-zone gateway alias on the LAN
    // interface, proxy-ARP for isolate-members, and per-device `/32` host routes.
    // They are inert unless a zone has a subnet and DHCP-mode is on.

    /// Add a secondary IPv4 address (a per-zone gateway alias) to `interface`.
    ///
    /// `ip` is the gateway host address (e.g. the `.1` of a zone subnet) and
    /// `prefix` the subnet prefix length. Idempotent: an already-present address
    /// is treated as success.
    async fn add_interface_alias(
        &self,
        interface: &str,
        ip: &str,
        prefix: u8,
    ) -> anyhow::Result<()>;

    /// Remove a secondary IPv4 address from `interface`. Idempotent: a missing
    /// address is treated as success.
    async fn remove_interface_alias(
        &self,
        interface: &str,
        ip: &str,
        prefix: u8,
    ) -> anyhow::Result<()>;

    /// List the IPv4 addresses currently configured on `interface` as
    /// `(ip, prefix)` pairs (includes the primary address and every alias). Used
    /// by the gateway-alias reconciler to drop aliases no longer backed by a zone.
    async fn list_interface_aliases(&self, interface: &str) -> anyhow::Result<Vec<(String, u8)>>;

    /// Enable or disable interface-wide proxy-ARP on `interface`
    /// (`/proc/sys/net/ipv4/conf/<iface>/proxy_arp`).
    ///
    /// Retained only so startup reconcile can clear a `proxy_arp=1` left behind
    /// by older daemon versions. The interface-wide sysctl must never be
    /// re-enabled: its FIB-lookup semantics let a tunnel-bound device's policy
    /// rule make the Pi answer ARP for *any* address that device probes
    /// (macOS "duplicate IP", LAN-peer hijack — issue #1107). Member isolation
    /// uses per-member proxy-neighbour entries instead (see
    /// [`Self::add_neigh_proxy`]).
    async fn set_proxy_arp(&self, interface: &str, enabled: bool) -> anyhow::Result<()>;

    /// Add a proxy-neighbour (pneigh) entry for `ip` on `interface`
    /// (`ip neigh add proxy <ip> dev <iface>`). Idempotent.
    ///
    /// An isolate-members zone hands each device a `/32`, so the device treats
    /// every peer as off-link and ARPs the gateway for it; a pneigh entry makes
    /// the Pi answer ARP for exactly that member's address — regardless of the
    /// route's egress interface — pulling intra-subnet peer traffic through the
    /// forward chain where it can be filtered. Unlike the interface-wide
    /// `proxy_arp` sysctl it never answers for arbitrary targets (issue #1107).
    /// Cooperating devices only (see the ADR).
    async fn add_neigh_proxy(&self, ip: &str, interface: &str) -> anyhow::Result<()>;

    /// Remove the proxy-neighbour entry for `ip` on `interface`. Idempotent:
    /// a missing entry is treated as success.
    async fn remove_neigh_proxy(&self, ip: &str, interface: &str) -> anyhow::Result<()>;

    /// List the IPv4 proxy-neighbour entries currently present on `interface`
    /// (`ip neigh show proxy`), so reconcile can prune entries no longer backed
    /// by an isolate-members device.
    async fn list_neigh_proxies(&self, interface: &str) -> anyhow::Result<Vec<String>>;

    /// Add a `/32` host route for `ip` via `interface` so the Pi has an on-link
    /// path to an isolate-members device.
    ///
    /// `pref_src` must be the gateway address of the device's own zone. The
    /// `/32` shadows that zone's `/24`, so without an explicit preferred source
    /// the kernel falls back to the output interface's primary address — which
    /// on a multi-zone LAN interface belongs to another zone — and replies to
    /// the device leave with the wrong source address (#1198).
    ///
    /// Idempotent, and repairs an existing route whose preferred source is
    /// missing or wrong rather than leaving it in place.
    async fn add_host_route(
        &self,
        ip: &str,
        interface: &str,
        pref_src: Ipv4Addr,
    ) -> anyhow::Result<()>;

    /// Remove the `/32` host route for `ip` via `interface`. Idempotent.
    async fn remove_host_route(&self, ip: &str, interface: &str) -> anyhow::Result<()>;
}
