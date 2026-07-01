use async_trait::async_trait;

/// Desired per-device Network-Zone packet policy (issue #736).
///
/// Every field is derived straight from the device's [`NetworkZone`]:
/// `allow_direct` / `allow_tunnel` mirror the zone's `allowed_targets`, and
/// `admin_ui_reachable` mirrors the zone flag of the same name. The firewall
/// translates a `false` on any field into a drop/reject rule keyed by the
/// device's IP; an all-`true`/reachable zone yields no rules.
///
/// [`NetworkZone`]: wardnet_common::network_zone::NetworkZone
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZoneRules {
    /// May the device egress directly (out the WAN)? When `false`, forwarded
    /// packets leaving via the LAN/WAN interface are dropped.
    pub allow_direct: bool,
    /// May the device egress via any tunnel? When `false`, forwarded packets
    /// leaving via a `wg_ward*` tunnel interface are dropped.
    pub allow_tunnel: bool,
    /// May the device reach the Pi's admin surfaces (:443/:7411)? When `false`,
    /// device→Pi connections to those ports are refused (TCP reset) while DNS
    /// (:53) and DHCP still pass.
    pub admin_ui_reachable: bool,
}

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

    /// Replace the Network-Zone packet rules for a single device IP (issue #736).
    ///
    /// Atomically drops this IP's existing `wardnet:zone:*:<device_ip>` rules and
    /// installs the set implied by `rules`:
    /// - a forward-chain **drop** of egress via any forbidden path — `wg_ward*`
    ///   (tunnel prefix match) when `!allow_tunnel`, `lan_interface` when
    ///   `!allow_direct`;
    /// - input-chain **reject-with-tcp-reset** of device→Pi :443 and :7411 when
    ///   `!admin_ui_reachable` (DNS/DHCP are left to pass).
    ///
    /// `lan_interface` is the WAN-facing egress interface used for the
    /// direct-drop. Idempotent: re-applying the same `rules` converges to the
    /// same rule set. A fully-permissive, admin-reachable zone installs nothing
    /// (and clears any prior rules for the IP).
    async fn apply_zone_rules(
        &self,
        device_ip: &str,
        rules: ZoneRules,
        lan_interface: &str,
    ) -> anyhow::Result<()>;

    /// Delete every Network-Zone rule (`wardnet:zone:*:<device_ip>`, both egress
    /// and admin-UI) for the given device IP. Used when a device departs or
    /// changes IP. Idempotent; no-op when the device has no zone rules.
    async fn remove_zone_rules(&self, device_ip: &str) -> anyhow::Result<()>;

    /// List the distinct device IPs that currently carry any Network-Zone rule
    /// (`wardnet:zone:*`) across the forward and input chains. Used by the
    /// enforcer's startup reconcile to drop rules for IPs no longer backed by a
    /// device.
    async fn list_zone_rule_ips(&self) -> anyhow::Result<Vec<String>>;

    /// Verify that the required firewall tools are available on the system.
    async fn check_tools_available(&self) -> anyhow::Result<()>;

    /// Delete the entire Wardnet firewall table (cleanup on shutdown).
    async fn destroy_wardnet_table(&self) -> anyhow::Result<()>;
}
