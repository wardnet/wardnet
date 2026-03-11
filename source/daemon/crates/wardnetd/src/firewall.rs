use async_trait::async_trait;

/// Abstraction over firewall operations for Wardnet policy routing.
///
/// Manages NAT masquerade (postrouting) and DNS redirect (prerouting) rules
/// for tunnel interfaces and routed devices. Enables mocking in tests.
/// The production implementation uses nftables via the `nft` command.
#[async_trait]
pub trait FirewallManager: Send + Sync {
    /// Initialize the firewall table and base chains (idempotent).
    ///
    /// Creates the necessary table structure for masquerade and DNS redirect rules.
    async fn init_wardnet_table(&self) -> anyhow::Result<()>;

    /// Flush all Wardnet-managed rules (keeps the table and chains intact).
    async fn flush_wardnet_table(&self) -> anyhow::Result<()>;

    /// Add a masquerade rule for traffic exiting through the given tunnel interface.
    async fn add_masquerade(&self, interface: &str) -> anyhow::Result<()>;

    /// Remove the masquerade rule for the given tunnel interface.
    async fn remove_masquerade(&self, interface: &str) -> anyhow::Result<()>;

    /// Add a DNS redirect rule for a device, rewriting its DNS traffic to the tunnel's DNS server.
    async fn add_dns_redirect(&self, device_ip: &str, dns_ip: &str) -> anyhow::Result<()>;

    /// Remove the DNS redirect rule for the given device IP.
    async fn remove_dns_redirect(&self, device_ip: &str) -> anyhow::Result<()>;

    /// Verify that the required firewall tools are available on the system.
    async fn check_tools_available(&self) -> anyhow::Result<()>;

    /// Delete the entire Wardnet firewall table (cleanup on shutdown).
    async fn destroy_wardnet_table(&self) -> anyhow::Result<()>;
}
