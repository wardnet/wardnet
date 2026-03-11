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

    /// Verify that required routing tools are available on the system.
    async fn check_tools_available(&self) -> anyhow::Result<()>;
}
