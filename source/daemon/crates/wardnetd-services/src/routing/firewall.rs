use async_trait::async_trait;

/// Abstraction over firewall operations for Wardnet policy routing.
///
/// Manages NAT masquerade (postrouting) rules for tunnel interfaces and
/// transient TCP-reset rules used during routing changes. Enables mocking
/// in tests. The production implementation uses nftables via the `nft`
/// command.
#[async_trait]
pub trait FirewallManager: Send + Sync {
    /// Initialize the firewall table and base chains (idempotent).
    async fn init_wardnet_table(&self) -> anyhow::Result<()>;

    /// Flush all Wardnet-managed rules (keeps the table and chains intact).
    async fn flush_wardnet_table(&self) -> anyhow::Result<()>;

    /// Add a masquerade rule for traffic exiting through the given tunnel interface.
    async fn add_masquerade(&self, interface: &str) -> anyhow::Result<()>;

    /// Remove the masquerade rule for the given tunnel interface.
    async fn remove_masquerade(&self, interface: &str) -> anyhow::Result<()>;

    /// One-shot startup cleanup: enumerate the prerouting chain and delete
    /// every rule whose comment matches the legacy `wardnet:dns:*` DNS
    /// redirect pattern. Idempotent; logs one info line per removal.
    /// Without this, daemons upgraded across the issue #342 fix would
    /// keep the bypass until the host reboots.
    async fn cleanup_legacy_dns_redirects(&self) -> anyhow::Result<()>;

    /// Add a temporary rule that rejects TCP packets from a device with TCP RST.
    ///
    /// Used when switching a device's routing target: the device's existing TCP
    /// sockets are stale (the remote server no longer recognises the flow after
    /// the source IP changed). Without an explicit RST, the device's TCP stack
    /// retransmits for 30-60s before timing out. This rule causes the Pi
    /// (acting as gateway) to send RST back to the device for any forwarded
    /// TCP traffic, prompting immediate socket teardown.
    async fn add_tcp_reset_reject(&self, device_ip: &str) -> anyhow::Result<()>;

    /// Remove the temporary TCP RST reject rule for a device.
    async fn remove_tcp_reset_reject(&self, device_ip: &str) -> anyhow::Result<()>;

    /// Verify that the required firewall tools are available on the system.
    async fn check_tools_available(&self) -> anyhow::Result<()>;

    /// Delete the entire Wardnet firewall table (cleanup on shutdown).
    async fn destroy_wardnet_table(&self) -> anyhow::Result<()>;
}
