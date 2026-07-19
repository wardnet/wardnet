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

/// One resolved cross-zone allow (issue #737), emitted ahead of the zone denies.
///
/// Rendered as an `ACCEPT` in the `zone_isolation` chain so a curated
/// cross-subnet service (e.g. a phone casting to a TV in another zone) survives
/// the otherwise default-deny between subnets. Endpoints are already resolved to
/// CIDRs (a device to its `/32`, a zone to its subnet) by the enforcer.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExceptionAllow {
    /// Source CIDR, e.g. `"10.44.2.5/32"` or `"10.44.2.0/24"`.
    pub from_cidr: String,
    /// Destination CIDR, same forms as `from_cidr`.
    pub to_cidr: String,
    /// Transport protocol, `"tcp"` or `"udp"`.
    pub proto: String,
    /// Inclusive destination-port range start (a single port has `start == end`).
    pub port_start: u16,
    /// Inclusive destination-port range end.
    pub port_end: u16,
    /// When `true` the firewall also renders the reverse direction (`to → from`
    /// on the same ports) so the return/handshake traffic is allowed.
    pub bidirectional: bool,
}

/// The full desired L3 isolation state (issue #737), rebuilt atomically into the
/// `zone_isolation` forward sub-chain.
///
/// The enforcer owns the *whole* chain: on any relevant change it recomputes
/// this whole-state value and the firewall flushes + rebuilds the chain in a
/// fixed order — allows first, then cross-subnet denies, then member-isolation
/// denies. There is no per-rule identity bookkeeping, which makes the
/// allows-before-denies ordering trivially correct. An empty value leaves the
/// chain empty (no L3 isolation — the graceful-degrade state used when Wardnet
/// does not own DHCP).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ZoneIsolationRules {
    /// Cross-zone `ACCEPT`s, evaluated first.
    pub allows: Vec<ExceptionAllow>,
    /// Ordered `(src_cidr, dst_cidr)` pairs whose forwarded traffic is dropped
    /// (the cross-subnet default-deny).
    pub deny_pairs: Vec<(String, String)>,
    /// Subnets whose forwarded intra-subnet peer traffic is dropped
    /// (member isolation).
    pub member_isolation_subnets: Vec<String>,
    /// Deduped `(from_cidr, to_cidr)` CIDR pairs whose cross-zone exception
    /// traffic must NOT be source-NAT'd, so the far endpoint sees the sender's
    /// real IP (required for mirroring/local-file casting, harmless for the
    /// rest). The firewall renders BOTH directions per pair, so each `(from, to)`
    /// is listed once.
    pub nat_exempt_pairs: Vec<(String, String)>,
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

    /// Add an `input`-chain rule accepting inbound `WireGuard` UDP handshakes on
    /// `port` (issue #809).
    ///
    /// The `input` chain already defaults to an accept policy, so this rule is
    /// currently redundant in practice. It is installed anyway as a
    /// forward-looking safety net: if the input policy is ever tightened to
    /// drop, the inbound server must still be reachable on its listen port.
    async fn add_inbound_wg_accept(&self, port: u16) -> anyhow::Result<()>;

    /// Remove the inbound-`WireGuard` UDP accept rule (issue #809). Idempotent.
    async fn remove_inbound_wg_accept(&self) -> anyhow::Result<()>;

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

    /// Atomically rebuild the `zone_isolation` forward sub-chain from `rules`
    /// (issue #737): flush it, then install every allow (`ACCEPT`), then every
    /// cross-subnet deny (`DROP`), then every member-isolation intra-subnet deny
    /// (`DROP`). Empty `rules` leaves the chain empty (no L3 isolation — the
    /// graceful-degrade state when Wardnet does not own DHCP). The enforcer owns
    /// the whole chain, so this is a whole-state replace with no per-rule
    /// identity bookkeeping.
    async fn apply_zone_isolation(&self, rules: ZoneIsolationRules) -> anyhow::Result<()>;

    /// Verify that the required firewall tools are available on the system.
    async fn check_tools_available(&self) -> anyhow::Result<()>;

    /// Delete the entire Wardnet firewall table (cleanup on shutdown).
    async fn destroy_wardnet_table(&self) -> anyhow::Result<()>;
}
