use std::net::Ipv4Addr;
use std::sync::Arc;

use async_trait::async_trait;
use futures::TryStreamExt;
use rtnetlink::packet_route::AddressFamily;
use rtnetlink::packet_route::route::{RouteAttribute, RouteScope};
use rtnetlink::packet_route::rule::{RuleAction, RuleAttribute, RuleMessage};
use rtnetlink::{Handle, RouteMessageBuilder};

use crate::command::CommandExecutor;
use crate::policy_router::PolicyRouter;

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
        if let Ok(()) = tokio::fs::write("/proc/sys/net/ipv4/ip_forward", "1").await {
            Ok(())
        } else {
            self.executor
                .run("sysctl", &["-w", "net.ipv4.ip_forward=1"])
                .await
                .map_err(|e| anyhow::anyhow!("failed to enable IP forwarding: {e}"))?;
            Ok(())
        }
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
                    tracing::warn!(table, error = %e, "failed to delete route from table");
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

    async fn flush_conntrack(&self, src_ip: &str) -> anyhow::Result<()> {
        // Conntrack flush via CLI — no mature pure-Rust netlink crate for
        // NFNL_SUBSYS_CTNETLINK. Filed as future work.
        let output = match self.executor.run("conntrack", &["-D", "-s", src_ip]).await {
            Ok(o) => o,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tracing::warn!(
                    "`conntrack` command not found; install conntrack to enable \
                     conntrack flushing on routing changes"
                );
                return Ok(());
            }
            Err(e) => return Err(anyhow::anyhow!("failed to execute `conntrack`: {e}")),
        };

        let deleted = output
            .stderr
            .split_whitespace()
            .collect::<Vec<_>>()
            .windows(2)
            .find_map(|w| (w[1] == "flow").then(|| w[0].parse::<u32>().ok()).flatten())
            .unwrap_or(0);

        if output.success || output.stderr.contains("flow entries have been deleted") {
            tracing::info!(src_ip, deleted, "flushed conntrack entries for source IP");
            return Ok(());
        }

        anyhow::bail!(
            "`conntrack -D -s {}` failed: {}",
            src_ip,
            output.stderr.trim()
        )
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
}
