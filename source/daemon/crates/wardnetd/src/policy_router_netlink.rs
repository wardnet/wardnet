use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use async_trait::async_trait;
use futures::TryStreamExt;
use rtnetlink::packet_route::AddressFamily;
use rtnetlink::packet_route::address::AddressAttribute;
use rtnetlink::packet_route::neighbour::{
    NeighbourAddress, NeighbourAttribute, NeighbourFlags, NeighbourMessage,
};
use rtnetlink::packet_route::route::{RouteAddress, RouteAttribute, RouteScope};
use rtnetlink::packet_route::rule::{RuleAction, RuleAttribute, RuleMessage};
use rtnetlink::{Handle, RouteMessageBuilder};

use rtnetlink::packet_route::route::{RouteHeader, RouteMessage};

use wardnetd_services::command::{CommandExecutor, CommandOutput};
use wardnetd_services::routing::policy_router::PolicyRouter;

/// The kernel `main` routing table id (254). Switchback carve-outs re-assert it
/// for specific cross-zone destinations so a tunnel-bound device delivers that
/// LAN traffic locally instead of over its per-tunnel table.
const RT_TABLE_MAIN: u8 = 254;

/// Parse an IPv4 `a.b.c.d/p` CIDR into `(address, prefix_length)`. A bare
/// address (no `/p`) is treated as a `/32` host route.
fn parse_ipv4_cidr(cidr: &str) -> anyhow::Result<(Ipv4Addr, u8)> {
    let Some((addr, prefix)) = cidr.split_once('/') else {
        // A bare address is a `/32` host route.
        let ip: Ipv4Addr = cidr
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid CIDR {cidr}: {e}"))?;
        return Ok((ip, 32));
    };
    let ip: Ipv4Addr = addr
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid CIDR address {cidr}: {e}"))?;
    let len: u8 = prefix
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid CIDR prefix {cidr}: {e}"))?;
    if len > 32 {
        anyhow::bail!("invalid CIDR prefix {cidr}: prefix > 32");
    }
    Ok((ip, len))
}

/// The routing table a route belongs to. For tables > 255 the number lives in
/// an attribute; otherwise in the header. The attribute wins so a wide table
/// can't alias onto a small one. Shared with `route_monitor`, which watches
/// these same tables for external deletions.
pub(crate) fn route_table(route: &RouteMessage) -> u32 {
    route
        .attributes
        .iter()
        .find_map(|a| match a {
            RouteAttribute::Table(t) => Some(*t),
            _ => None,
        })
        .unwrap_or_else(|| u32::from(route.header.table))
}

/// The routing table an `ip rule` targets. Mirrors [`route_table`]: a table
/// above 255 rides a `RuleAttribute::Table` attribute, otherwise the number is
/// in `header.table`; the attribute wins so a wide table can't alias onto a
/// small one.
pub(crate) fn rule_table(rule: &RuleMessage) -> u32 {
    rule.attributes
        .iter()
        .find_map(|a| match a {
            RuleAttribute::Table(t) => Some(*t),
            _ => None,
        })
        .unwrap_or_else(|| u32::from(rule.header.table))
}

/// Production [`PolicyRouter`] backed by Linux netlink sockets.
///
/// Route and rule operations go through [`rtnetlink`]; conntrack flushing still
/// shells out to the `conntrack` CLI (no mature pure-Rust alternative). The
/// netlink connection driver task is spawned on construction and runs until the
/// handle is dropped.
pub struct NetlinkPolicyRouter {
    handle: Handle,
    /// CLI executor kept for `conntrack` flush (no netlink crate for conntrack).
    executor: Arc<dyn CommandExecutor>,
}

impl std::fmt::Debug for NetlinkPolicyRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetlinkPolicyRouter").finish()
    }
}

impl NetlinkPolicyRouter {
    /// Create a new netlink-backed policy router.
    ///
    /// Opens a netlink connection and spawns the driver task. The `executor`
    /// is used only for conntrack flush (still CLI-based).
    pub fn new(executor: Arc<dyn CommandExecutor>) -> anyhow::Result<Self> {
        let (connection, handle, _) = rtnetlink::new_connection()?;
        tokio::spawn(connection);
        tracing::info!("netlink policy router initialised");
        Ok(Self { handle, executor })
    }

    /// Resolve a network interface name to its kernel link index.
    async fn link_index(&self, interface: &str) -> anyhow::Result<u32> {
        let mut links = self
            .handle
            .link()
            .get()
            .match_name(interface.to_owned())
            .execute();
        if let Some(link) = links.try_next().await? {
            Ok(link.header.index)
        } else {
            anyhow::bail!("interface {interface} not found")
        }
    }

    /// Build a `RouteMessage` filter for listing IPv4 routes.
    fn ipv4_route_filter() -> rtnetlink::packet_route::route::RouteMessage {
        RouteMessageBuilder::<Ipv4Addr>::new().build()
    }
}

#[async_trait]
impl PolicyRouter for NetlinkPolicyRouter {
    async fn enable_ip_forwarding(&self) -> anyhow::Result<()> {
        const PATH: &str = "/proc/sys/net/ipv4/ip_forward";

        // The daemon's systemd unit runs as User=wardnet with CAP_NET_ADMIN
        // but no CAP_DAC_OVERRIDE, so it cannot open /proc/sys/net/ipv4/ip_forward
        // (root:root 0644) for writing. Operators commonly pre-enable forwarding
        // via the host kernel, sysctl.d, or `docker run --sysctl` — in those
        // cases the daemon has nothing to do, so check before writing.
        if let Ok(value) = tokio::fs::read_to_string(PATH).await
            && value.trim() == "1"
        {
            return Ok(());
        }

        let write_err = match tokio::fs::write(PATH, "1").await {
            Ok(()) => return Ok(()),
            Err(e) => e,
        };

        let output = self
            .executor
            .run("sysctl", &["-w", "net.ipv4.ip_forward=1"])
            .await
            .map_err(|spawn_err| {
                anyhow::anyhow!(
                    "failed to enable IP forwarding: writing {PATH} failed ({write_err}); \
                     sysctl fallback failed to spawn ({spawn_err})"
                )
            })?;

        if !output.success {
            return Err(anyhow::anyhow!(
                "failed to enable IP forwarding: writing {PATH} failed ({write_err}); \
                 sysctl fallback exited unsuccessfully: {}",
                output.stderr.trim()
            ));
        }

        Ok(())
    }

    async fn add_route_table(&self, interface: &str, table: u32) -> anyhow::Result<()> {
        let index = self.link_index(interface).await?;

        // Build the route message: default route via interface in the given table.
        let route_msg = RouteMessageBuilder::<Ipv4Addr>::new()
            .output_interface(index)
            .table_id(table)
            .scope(RouteScope::Link)
            .build();

        let result = self.handle.route().add(route_msg).execute().await;

        match result {
            Ok(()) => Ok(()),
            Err(rtnetlink::Error::NetlinkError(msg)) if msg.to_string().contains("File exists") => {
                tracing::debug!(interface, table, "route already exists, skipping");
                Ok(())
            }
            Err(e) => {
                anyhow::bail!("failed to add default route dev {interface} table {table}: {e}")
            }
        }
    }

    async fn remove_route_table(&self, table: u32) -> anyhow::Result<()> {
        // List routes in this table and delete the default route.
        let mut routes = self.handle.route().get(Self::ipv4_route_filter()).execute();

        while let Some(route) = routes.try_next().await? {
            if u32::from(route.header.table) == table
                || route
                    .attributes
                    .iter()
                    .any(|a| matches!(a, RouteAttribute::Table(t) if *t == table))
            {
                if let Err(e) = self.handle.route().del(route).execute().await {
                    tracing::warn!(table, error = %e, "failed to delete route from table {table}: {e}");
                }
                return Ok(());
            }
        }

        // No route found — nothing to delete.
        tracing::debug!(table, "no default route found in table, nothing to remove");
        Ok(())
    }

    async fn has_route_table(&self, table: u32) -> anyhow::Result<bool> {
        let mut routes = self.handle.route().get(Self::ipv4_route_filter()).execute();

        while let Some(route) = routes.try_next().await? {
            if u32::from(route.header.table) == table
                || route
                    .attributes
                    .iter()
                    .any(|a| matches!(a, RouteAttribute::Table(t) if *t == table))
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn add_ip_rule(&self, src_ip: &str, table: u32) -> anyhow::Result<()> {
        let ip: Ipv4Addr = src_ip
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid IP {src_ip}: {e}"))?;

        self.handle
            .rule()
            .add()
            .v4()
            .source_prefix(ip, 32)
            .table_id(table)
            .action(RuleAction::ToTable)
            .execute()
            .await
            .map_err(|e| {
                anyhow::anyhow!("failed to add ip rule from {src_ip} lookup {table}: {e}")
            })?;

        Ok(())
    }

    async fn remove_ip_rule(&self, src_ip: &str, table: u32) -> anyhow::Result<()> {
        let ip: Ipv4Addr = src_ip
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid IP {src_ip}: {e}"))?;

        // Build a RuleMessage matching the rule to delete.
        let mut rule_msg = RuleMessage::default();
        rule_msg.header.family = AddressFamily::Inet;
        rule_msg.header.src_len = 32;
        rule_msg.attributes.push(RuleAttribute::Source(ip.into()));
        if table > 255 {
            rule_msg.attributes.push(RuleAttribute::Table(table));
        } else {
            rule_msg.header.table = u8::try_from(table).expect("table <= 255 guaranteed by branch");
        }

        self.handle
            .rule()
            .del(rule_msg)
            .execute()
            .await
            .map_err(|e| {
                anyhow::anyhow!("failed to remove ip rule from {src_ip} lookup {table}: {e}")
            })?;

        Ok(())
    }

    async fn list_wardnet_rules(&self) -> anyhow::Result<Vec<(String, u32)>> {
        let mut rules_stream = self.handle.rule().get(rtnetlink::IpVersion::V4).execute();
        let mut result = Vec::new();

        while let Some(rule) = rules_stream.try_next().await? {
            // Extract table number — may be in header or in attributes.
            let table = rule
                .attributes
                .iter()
                .find_map(|a| {
                    if let RuleAttribute::Table(t) = a {
                        Some(*t)
                    } else {
                        None
                    }
                })
                .unwrap_or(u32::from(rule.header.table));

            if table < 100 {
                continue;
            }

            // Extract source IP.
            let src_ip = rule.attributes.iter().find_map(|a| {
                if let RuleAttribute::Source(addr) = a {
                    Some(format!("{addr}"))
                } else {
                    None
                }
            });

            if let Some(ip) = src_ip {
                result.push((ip, table));
            }
        }

        Ok(result)
    }

    async fn add_switchback_rule(
        &self,
        src_ip: &str,
        dst_cidr: &str,
        priority: u32,
    ) -> anyhow::Result<()> {
        let src: Ipv4Addr = src_ip
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid switchback src IP {src_ip}: {e}"))?;
        let (dst, dst_len) = parse_ipv4_cidr(dst_cidr)?;

        // `ip rule from <src>/32 to <dst_cidr> lookup main priority <priority>`.
        // The `main` table (254) already delivers cross-zone LAN traffic locally,
        // so this pulls the exception's destination back off the per-tunnel table.
        let result = self
            .handle
            .rule()
            .add()
            .v4()
            .source_prefix(src, 32)
            .destination_prefix(dst, dst_len)
            .table_id(u32::from(RT_TABLE_MAIN))
            .priority(priority)
            .action(RuleAction::ToTable)
            .execute()
            .await;

        match result {
            Ok(()) => Ok(()),
            Err(rtnetlink::Error::NetlinkError(msg)) if msg.to_string().contains("File exists") => {
                tracing::debug!(
                    src_ip,
                    dst_cidr,
                    priority,
                    "switchback rule already exists, skipping"
                );
                Ok(())
            }
            Err(e) => {
                anyhow::bail!(
                    "failed to add switchback rule from {src_ip} to {dst_cidr} \
                     lookup main priority {priority}: {e}"
                )
            }
        }
    }

    async fn remove_switchback_rule(
        &self,
        src_ip: &str,
        dst_cidr: &str,
        priority: u32,
    ) -> anyhow::Result<()> {
        let src: Ipv4Addr = src_ip
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid switchback src IP {src_ip}: {e}"))?;
        let (dst, dst_len) = parse_ipv4_cidr(dst_cidr)?;

        // Match the rule precisely: src/32 + dst/dst_len + main table + priority.
        // The priority is what disambiguates a carve-out from any other
        // `to <dst> lookup main` rule the box might carry.
        let mut rule_msg = RuleMessage::default();
        rule_msg.header.family = AddressFamily::Inet;
        rule_msg.header.src_len = 32;
        rule_msg.header.dst_len = dst_len;
        rule_msg.header.action = RuleAction::ToTable;
        rule_msg.header.table = RT_TABLE_MAIN;
        rule_msg.attributes.push(RuleAttribute::Source(src.into()));
        rule_msg
            .attributes
            .push(RuleAttribute::Destination(dst.into()));
        rule_msg.attributes.push(RuleAttribute::Priority(priority));

        let result = self.handle.rule().del(rule_msg).execute().await;
        match result {
            Ok(()) => Ok(()),
            Err(rtnetlink::Error::NetlinkError(msg))
                if {
                    let s = msg.to_string();
                    s.contains("No such file or directory") || s.contains("No such process")
                } =>
            {
                tracing::debug!(
                    src_ip,
                    dst_cidr,
                    priority,
                    "switchback rule not present, nothing to remove"
                );
                Ok(())
            }
            Err(e) => {
                anyhow::bail!(
                    "failed to remove switchback rule from {src_ip} to {dst_cidr} \
                     lookup main priority {priority}: {e}"
                )
            }
        }
    }

    async fn list_switchback_rules(&self) -> anyhow::Result<Vec<(String, String, u32)>> {
        let mut rules_stream = self.handle.rule().get(rtnetlink::IpVersion::V4).execute();
        let mut result = Vec::new();

        while let Some(rule) = rules_stream.try_next().await? {
            // A switchback carve-out always looks up `main` (254). Skip anything
            // targeting another table so the reconcile prune can never delete an
            // unrelated priority-matched from/to rule that points elsewhere.
            if rule_table(&rule) != u32::from(RT_TABLE_MAIN) {
                continue;
            }

            // Priority lives only in an attribute; a rule without one is not a
            // wardnet switchback carve-out (which always sets it explicitly).
            let Some(priority) = rule.attributes.iter().find_map(|a| match a {
                RuleAttribute::Priority(p) => Some(*p),
                _ => None,
            }) else {
                continue;
            };

            let src = rule.attributes.iter().find_map(|a| match a {
                RuleAttribute::Source(addr) => Some(format!("{addr}")),
                _ => None,
            });
            let dst = rule.attributes.iter().find_map(|a| match a {
                RuleAttribute::Destination(addr) => Some(format!("{addr}")),
                _ => None,
            });

            if let (Some(src), Some(dst)) = (src, dst) {
                // Re-form the destination as `a.b.c.d/p` from the header prefix.
                let dst_cidr = format!("{dst}/{}", rule.header.dst_len);
                result.push((src, dst_cidr, priority));
            }
        }

        Ok(result)
    }

    async fn add_domain_route_rule(
        &self,
        src_ip: &str,
        dst_ip: &str,
        table: u32,
        priority: u32,
    ) -> anyhow::Result<()> {
        let src: Ipv4Addr = src_ip
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid domain-route src IP {src_ip}: {e}"))?;
        let dst: Ipv4Addr = dst_ip
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid domain-route dst IP {dst_ip}: {e}"))?;

        // `ip rule from <src>/32 to <dst>/32 lookup <table> priority <priority>`.
        let result = self
            .handle
            .rule()
            .add()
            .v4()
            .source_prefix(src, 32)
            .destination_prefix(dst, 32)
            .table_id(table)
            .priority(priority)
            .action(RuleAction::ToTable)
            .execute()
            .await;

        match result {
            Ok(()) => Ok(()),
            Err(rtnetlink::Error::NetlinkError(msg)) if msg.to_string().contains("File exists") => {
                tracing::debug!(
                    src_ip,
                    dst_ip,
                    table,
                    priority,
                    "domain-route rule already exists, skipping"
                );
                Ok(())
            }
            Err(e) => {
                anyhow::bail!(
                    "failed to add domain-route rule from {src_ip} to {dst_ip} \
                     lookup {table} priority {priority}: {e}"
                )
            }
        }
    }

    async fn remove_domain_route_rule(
        &self,
        src_ip: &str,
        dst_ip: &str,
        table: u32,
        priority: u32,
    ) -> anyhow::Result<()> {
        let src: Ipv4Addr = src_ip
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid domain-route src IP {src_ip}: {e}"))?;
        let dst: Ipv4Addr = dst_ip
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid domain-route dst IP {dst_ip}: {e}"))?;

        let table_u8 = u8::try_from(table).ok();
        let mut rule_msg = RuleMessage::default();
        rule_msg.header.family = AddressFamily::Inet;
        rule_msg.header.src_len = 32;
        rule_msg.header.dst_len = 32;
        rule_msg.header.action = RuleAction::ToTable;
        // Tables <= 255 live in the header; wider tables live in an attribute.
        if let Some(t) = table_u8 {
            rule_msg.header.table = t;
        } else {
            rule_msg.attributes.push(RuleAttribute::Table(table));
        }
        rule_msg.attributes.push(RuleAttribute::Source(src.into()));
        rule_msg
            .attributes
            .push(RuleAttribute::Destination(dst.into()));
        rule_msg.attributes.push(RuleAttribute::Priority(priority));

        let result = self.handle.rule().del(rule_msg).execute().await;
        match result {
            Ok(()) => Ok(()),
            Err(rtnetlink::Error::NetlinkError(msg))
                if {
                    let s = msg.to_string();
                    s.contains("No such file or directory") || s.contains("No such process")
                } =>
            {
                tracing::debug!(
                    src_ip,
                    dst_ip,
                    table,
                    priority,
                    "domain-route rule not present, nothing to remove"
                );
                Ok(())
            }
            Err(e) => {
                anyhow::bail!(
                    "failed to remove domain-route rule from {src_ip} to {dst_ip} \
                     lookup {table} priority {priority}: {e}"
                )
            }
        }
    }

    async fn list_domain_route_rules(
        &self,
        priority: u32,
    ) -> anyhow::Result<Vec<(String, String, u32)>> {
        let mut rules_stream = self.handle.rule().get(rtnetlink::IpVersion::V4).execute();
        let mut result = Vec::new();

        while let Some(rule) = rules_stream.try_next().await? {
            // Domain-route rules are identified by their dedicated priority band,
            // so the reconcile prune never touches a switchback or unrelated rule.
            let Some(rule_priority) = rule.attributes.iter().find_map(|a| match a {
                RuleAttribute::Priority(p) => Some(*p),
                _ => None,
            }) else {
                continue;
            };
            if rule_priority != priority {
                continue;
            }

            let src = rule.attributes.iter().find_map(|a| match a {
                RuleAttribute::Source(addr) => Some(format!("{addr}")),
                _ => None,
            });
            let dst = rule.attributes.iter().find_map(|a| match a {
                RuleAttribute::Destination(addr) => Some(format!("{addr}")),
                _ => None,
            });

            if let (Some(src), Some(dst)) = (src, dst) {
                result.push((src, dst, rule_table(&rule)));
            }
        }

        Ok(result)
    }

    async fn flush_conntrack(&self, src_ip: &str) -> anyhow::Result<()> {
        // Conntrack flush via CLI — no mature pure-Rust netlink crate for
        // NFNL_SUBSYS_CTNETLINK. Filed as future work.

        // Helper closure to run one conntrack -D invocation and parse the
        // deleted-entry count from its stderr output.
        let run = |flag: &'static str| {
            let executor = &self.executor;
            async move {
                match executor.run("conntrack", &["-D", flag, src_ip]).await {
                    Ok(o) => Ok(o),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        tracing::warn!(
                            "`conntrack` command not found; install conntrack to enable \
                             conntrack flushing on routing changes"
                        );
                        // Return a synthetic "success with 0 deletions" so the caller
                        // does not treat a missing binary as a hard error.
                        Ok(CommandOutput {
                            success: true,
                            stdout: String::new(),
                            stderr: String::new(),
                        })
                    }
                    Err(e) => Err(anyhow::anyhow!("failed to execute `conntrack`: {e}")),
                }
            }
        };

        let parse_deleted = |output: &CommandOutput| -> u32 {
            output
                .stderr
                .split_whitespace()
                .collect::<Vec<_>>()
                .windows(2)
                .find_map(|w| (w[1] == "flow").then(|| w[0].parse::<u32>().ok()).flatten())
                .unwrap_or(0)
        };

        // Flush flows where the device is the source (outbound NAT entries).
        let src_output = run("-s").await?;
        if !src_output.success && !src_output.stderr.contains("flow entries have been deleted") {
            anyhow::bail!(
                "`conntrack -D -s {}` failed: {}",
                src_ip,
                src_output.stderr.trim()
            );
        }
        let deleted_src = parse_deleted(&src_output);

        // Flush flows where the device is the destination (return-path entries
        // created by server-initiated or push-notification traffic).
        let dst_output = run("-d").await?;
        if !dst_output.success && !dst_output.stderr.contains("flow entries have been deleted") {
            anyhow::bail!(
                "`conntrack -D -d {}` failed: {}",
                src_ip,
                dst_output.stderr.trim()
            );
        }
        let deleted_dst = parse_deleted(&dst_output);

        tracing::info!(
            src_ip,
            deleted_src,
            deleted_dst,
            "flushed conntrack entries for device IP (source + destination)"
        );
        Ok(())
    }

    async fn flush_route_cache(&self) -> anyhow::Result<()> {
        // The kernel route cache was removed in Linux 3.6. On modern kernels
        // this is a no-op. Log at debug so it's visible in traces.
        tracing::debug!("flush_route_cache: no-op on modern kernels (route cache removed in 3.6)");
        Ok(())
    }

    async fn check_tools_available(&self) -> anyhow::Result<()> {
        // Verify netlink works by listing links.
        let mut links = self.handle.link().get().execute();
        let _first = links
            .try_next()
            .await
            .map_err(|e| anyhow::anyhow!("netlink socket not working: {e}"))?;
        tracing::info!("netlink: policy router tools available");
        Ok(())
    }

    async fn add_interface_alias(
        &self,
        interface: &str,
        ip: &str,
        prefix: u8,
    ) -> anyhow::Result<()> {
        let addr: Ipv4Addr = ip
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid alias IP {ip}: {e}"))?;
        let index = self.link_index(interface).await?;

        match self
            .handle
            .address()
            .add(index, IpAddr::V4(addr), prefix)
            .execute()
            .await
        {
            Ok(()) => Ok(()),
            Err(rtnetlink::Error::NetlinkError(msg)) if msg.to_string().contains("File exists") => {
                tracing::debug!(
                    interface,
                    ip,
                    prefix,
                    "interface alias already present, skipping"
                );
                Ok(())
            }
            Err(e) => {
                anyhow::bail!("failed to add alias {ip}/{prefix} dev {interface}: {e}")
            }
        }
    }

    async fn remove_interface_alias(
        &self,
        interface: &str,
        ip: &str,
        prefix: u8,
    ) -> anyhow::Result<()> {
        let addr: Ipv4Addr = ip
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid alias IP {ip}: {e}"))?;
        let index = self.link_index(interface).await?;

        // Find the matching AddressMessage on this link and delete it. Deleting
        // by a hand-built message is brittle across kernels; round-tripping the
        // kernel's own message is the robust idiom.
        let mut addrs = self
            .handle
            .address()
            .get()
            .set_link_index_filter(index)
            .execute();
        while let Some(msg) = addrs.try_next().await? {
            if msg.header.prefix_len != prefix {
                continue;
            }
            let matches = msg.attributes.iter().any(|a| {
                matches!(a, AddressAttribute::Local(IpAddr::V4(v)) if *v == addr)
                    || matches!(a, AddressAttribute::Address(IpAddr::V4(v)) if *v == addr)
            });
            if matches {
                if let Err(e) = self.handle.address().del(msg).execute().await {
                    anyhow::bail!("failed to remove alias {ip}/{prefix} dev {interface}: {e}");
                }
                return Ok(());
            }
        }
        tracing::debug!(
            interface,
            ip,
            prefix,
            "interface alias not present, nothing to remove"
        );
        Ok(())
    }

    async fn list_interface_aliases(&self, interface: &str) -> anyhow::Result<Vec<(String, u8)>> {
        let index = self.link_index(interface).await?;
        let mut addrs = self
            .handle
            .address()
            .get()
            .set_link_index_filter(index)
            .execute();
        let mut out = Vec::new();
        while let Some(msg) = addrs.try_next().await? {
            let prefix = msg.header.prefix_len;
            let ip = msg.attributes.iter().find_map(|a| match a {
                AddressAttribute::Local(IpAddr::V4(v)) => Some(*v),
                _ => None,
            });
            // Fall back to IFA_ADDRESS when IFA_LOCAL is absent (non-ptp links
            // usually carry both; some kernels omit Local for IPv4 aliases).
            let ip = ip.or_else(|| {
                msg.attributes.iter().find_map(|a| match a {
                    AddressAttribute::Address(IpAddr::V4(v)) => Some(*v),
                    _ => None,
                })
            });
            if let Some(v) = ip {
                out.push((v.to_string(), prefix));
            }
        }
        Ok(out)
    }

    async fn set_proxy_arp(&self, interface: &str, enabled: bool) -> anyhow::Result<()> {
        let path = format!("/proc/sys/net/ipv4/conf/{interface}/proxy_arp");
        let want = if enabled { "1" } else { "0" };

        // Same permission dance as `enable_ip_forwarding`: the daemon runs with
        // CAP_NET_ADMIN but not CAP_DAC_OVERRIDE, so the /proc write may be
        // refused; check first, then fall back to `sysctl -w`.
        if let Ok(value) = tokio::fs::read_to_string(&path).await
            && value.trim() == want
        {
            return Ok(());
        }

        let write_err = match tokio::fs::write(&path, want).await {
            Ok(()) => return Ok(()),
            Err(e) => e,
        };

        let key = format!("net.ipv4.conf.{interface}.proxy_arp={want}");
        let output = self
            .executor
            .run("sysctl", &["-w", &key])
            .await
            .map_err(|spawn_err| {
                anyhow::anyhow!(
                    "failed to set proxy_arp on {interface}: writing {path} failed ({write_err}); \
                     sysctl fallback failed to spawn ({spawn_err})"
                )
            })?;
        if !output.success {
            anyhow::bail!(
                "failed to set proxy_arp on {interface}: writing {path} failed ({write_err}); \
                 sysctl fallback exited unsuccessfully: {}",
                output.stderr.trim()
            );
        }
        Ok(())
    }

    async fn add_neigh_proxy(&self, ip: &str, interface: &str) -> anyhow::Result<()> {
        let addr: Ipv4Addr = ip
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid neigh-proxy IP {ip}: {e}"))?;
        let index = self.link_index(interface).await?;

        // `replace()` makes re-adding an existing entry a no-op success, the
        // same idempotency `ip neigh replace proxy` provides.
        match self
            .handle
            .neighbours()
            .add(index, IpAddr::V4(addr))
            .flags(NeighbourFlags::Proxy)
            .replace()
            .execute()
            .await
        {
            Ok(()) => Ok(()),
            Err(e) => anyhow::bail!("failed to add neigh proxy {ip} dev {interface}: {e}"),
        }
    }

    async fn remove_neigh_proxy(&self, ip: &str, interface: &str) -> anyhow::Result<()> {
        let addr: Ipv4Addr = ip
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid neigh-proxy IP {ip}: {e}"))?;
        let index = self.link_index(interface).await?;

        // Unlike an address (where round-tripping the kernel's message preserves
        // attributes we don't model), a pneigh delete is keyed on exactly
        // family + ifindex + NTF_PROXY + destination, so the message can be
        // built directly — no table dump per removal.
        let mut msg = NeighbourMessage::default();
        msg.header.family = AddressFamily::Inet;
        msg.header.ifindex = index;
        msg.header.flags = NeighbourFlags::Proxy;
        msg.attributes
            .push(NeighbourAttribute::Destination(NeighbourAddress::Inet(
                addr,
            )));

        match self.handle.neighbours().del(msg).execute().await {
            Ok(()) => Ok(()),
            // Idempotent: a missing entry is success, like the other removals.
            Err(rtnetlink::Error::NetlinkError(m)) if m.raw_code() == -libc::ENOENT => {
                tracing::debug!(
                    ip,
                    interface,
                    "neigh proxy {ip} dev {interface} not present, nothing to remove",
                    ip = ip,
                    interface = interface,
                );
                Ok(())
            }
            Err(e) => anyhow::bail!("failed to remove neigh proxy {ip} dev {interface}: {e}"),
        }
    }

    async fn list_neigh_proxies(&self, interface: &str) -> anyhow::Result<Vec<String>> {
        let index = self.link_index(interface).await?;
        let mut entries = self
            .handle
            .neighbours()
            .get()
            .proxies()
            .set_family(rtnetlink::IpVersion::V4)
            .execute();
        let mut out = Vec::new();
        while let Some(msg) = entries.try_next().await? {
            if msg.header.ifindex == index
                && let Some(addr) = neigh_destination(&msg)
            {
                out.push(addr.to_string());
            }
        }
        Ok(out)
    }

    async fn add_host_route(
        &self,
        ip: &str,
        interface: &str,
        pref_src: Ipv4Addr,
    ) -> anyhow::Result<()> {
        let addr: Ipv4Addr = ip
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid host-route IP {ip}: {e}"))?;
        let index = self.link_index(interface).await?;

        let route = build_host_route(addr, index, pref_src);

        // `replace()` rather than the default `NLM_F_EXCL`, so this is both
        // idempotent *and* self-healing. Skipping on `File exists` (what we
        // used to do) meant a route installed by an older build — carrying no
        // preferred source — survived every reconcile untouched, and only
        // devices that happened to re-DHCP after the upgrade were repaired.
        //
        // Scope of the replace: `fib_table_insert` matches the existing alias on
        // table + destination/prefix (+ tos/priority) — NOT on `oif` — and
        // replaces the first match. So this overwrites any `main`-table `/32`
        // for this address, including one pointing at a different interface.
        // That is what we want (there should only ever be ours, and a stale one
        // on another interface is exactly what wants replacing), but it is
        // wider than "only the route we installed": a second caller passing a
        // different interface would clobber this one rather than coexist.
        // The kernel's `local`-table route for one of our own addresses lives in
        // a different table and is never a candidate (#886, #1198).
        match self.handle.route().add(route).replace().execute().await {
            Ok(()) => Ok(()),
            Err(e) => anyhow::bail!("failed to add host route {ip}/32 dev {interface}: {e}"),
        }
    }

    async fn remove_host_route(&self, ip: &str, interface: &str) -> anyhow::Result<()> {
        let addr: Ipv4Addr = ip
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid host-route IP {ip}: {e}"))?;
        let index = self.link_index(interface).await?;

        let mut routes = self.handle.route().get(Self::ipv4_route_filter()).execute();
        while let Some(route) = routes.try_next().await? {
            if is_removable_host_route(&route, addr, index) {
                if let Err(e) = self.handle.route().del(route).execute().await {
                    anyhow::bail!("failed to remove host route {ip}/32 dev {interface}: {e}");
                }
                return Ok(());
            }
        }
        tracing::debug!(ip, interface, "host route not present, nothing to remove");
        Ok(())
    }
}

/// The IPv4 destination of a neighbour message, if it carries one.
fn neigh_destination(msg: &NeighbourMessage) -> Option<Ipv4Addr> {
    msg.attributes.iter().find_map(|a| match a {
        NeighbourAttribute::Destination(NeighbourAddress::Inet(v)) => Some(*v),
        _ => None,
    })
}

/// The `/32` host route [`PolicyRouter::add_host_route`] installs for a device
/// in an isolate-members zone: table `main`, `scope link`, out of `oif`.
///
/// `pref_src` (the device's zone gateway) is load-bearing, not decoration. This
/// `/32` is more specific than the zone's `/24`, so it wins the route lookup —
/// and a route with no `RTA_PREFSRC` drops the `src <gateway>` hint the `/24`
/// carried. Source selection for locally-generated traffic then falls back to
/// the output interface's primary address, which for a multi-zone LAN
/// interface belongs to a *different* zone. The daemon's DNS replies to a
/// member-isolated device went out with that wrong source, postrouting
/// `masquerade` rewrote the address back and reallocated the source port out of
/// netfilter's reserved sub-512 pool, and no client would accept the answer
/// (#1198).
pub(crate) fn build_host_route(addr: Ipv4Addr, oif: u32, pref_src: Ipv4Addr) -> RouteMessage {
    RouteMessageBuilder::<Ipv4Addr>::new()
        .destination_prefix(addr, 32)
        .output_interface(oif)
        .scope(RouteScope::Link)
        .pref_source(pref_src)
        .build()
}

/// True if `route` is a host route that [`PolicyRouter::add_host_route`] itself
/// could have installed for `addr` out of link index `oif` — i.e. one
/// [`PolicyRouter::remove_host_route`] is allowed to delete.
///
/// The table and scope checks are load-bearing, not cosmetic. For every address
/// configured on this box the kernel keeps a route in the `local` table that is
/// ALSO a `/32` with the same destination and output interface — it is what
/// makes packets addressed to us deliver to us. Matching on destination + oif
/// alone therefore matches the kernel's local route, and deleting it makes the
/// box blackhole its own address: it keeps answering ARP, but every packet to
/// it is forwarded rather than delivered, and it replies `ICMP destination host
/// unreachable` from its own IP. Restricting removal to what we add (table
/// `main`, `scope link`) is what keeps a device row that claims our own IP from
/// locking the admin out of the box entirely (issue #886).
pub(crate) fn is_removable_host_route(route: &RouteMessage, addr: Ipv4Addr, oif: u32) -> bool {
    if route.header.destination_prefix_length != 32 {
        return false;
    }
    if route.header.scope != RouteScope::Link {
        return false;
    }
    // `add_host_route` sets no table, so the kernel files our routes in `main`.
    // The kernel's own `local` table (255) — holding the `scope host` `/32`
    // that delivers each of the box's addresses to itself — must never match.
    if route_table(route) != u32::from(RouteHeader::RT_TABLE_MAIN) {
        return false;
    }
    let dest_matches = route
        .attributes
        .iter()
        .any(|a| matches!(a, RouteAttribute::Destination(RouteAddress::Inet(d)) if *d == addr));
    let oif_matches = route
        .attributes
        .iter()
        .any(|a| matches!(a, RouteAttribute::Oif(o) if *o == oif));
    dest_matches && oif_matches
}
