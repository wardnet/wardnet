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
}
