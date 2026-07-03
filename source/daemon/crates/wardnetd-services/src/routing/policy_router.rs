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

    /// Add a source-based routing rule that directs traffic from `src_ip` through the given table.
    async fn add_ip_rule(&self, src_ip: &str, table: u32) -> anyhow::Result<()>;

    /// Remove a source-based routing rule for `src_ip` and the given table.
    async fn remove_ip_rule(&self, src_ip: &str, table: u32) -> anyhow::Result<()>;

    /// List all Wardnet-managed routing rules (tables >= 100).
    ///
    /// Returns tuples of (`source_ip`, `table_number`).
    async fn list_wardnet_rules(&self) -> anyhow::Result<Vec<(String, u32)>>;

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

    /// Enable or disable proxy-ARP on `interface`
    /// (`/proc/sys/net/ipv4/conf/<iface>/proxy_arp`).
    ///
    /// An isolate-members zone hands each device a `/32`, so the device treats
    /// every peer as off-link and ARPs the gateway for it; proxy-ARP makes the
    /// Pi answer, pulling intra-subnet peer traffic through the forward chain
    /// where it can be filtered. Cooperating devices only (see the ADR).
    async fn set_proxy_arp(&self, interface: &str, enabled: bool) -> anyhow::Result<()>;

    /// Add a `/32` host route for `ip` via `interface` so the Pi has an on-link
    /// path to an isolate-members device. Idempotent.
    async fn add_host_route(&self, ip: &str, interface: &str) -> anyhow::Result<()>;

    /// Remove the `/32` host route for `ip` via `interface`. Idempotent.
    async fn remove_host_route(&self, ip: &str, interface: &str) -> anyhow::Result<()>;
}
