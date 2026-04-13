use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;

use crate::command::CommandExecutor;
use crate::policy_router::PolicyRouter;

/// Production implementation of [`PolicyRouter`] using the Linux `ip` and `sysctl` CLI tools.
///
/// All operations are executed via a [`CommandExecutor`], which enables testing without
/// shelling out to real commands. Requires `CAP_NET_ADMIN` capability and the `iproute2`
/// package on the host.
#[derive(Debug)]
pub struct IproutePolicyRouter {
    /// The command executor used to run `ip` and `sysctl` commands.
    executor: Arc<dyn CommandExecutor>,
}

impl IproutePolicyRouter {
    /// Create a new `IproutePolicyRouter` backed by the given command executor.
    pub fn new(executor: Arc<dyn CommandExecutor>) -> Self {
        Self { executor }
    }

    /// Run a command via the executor and return its stdout.
    ///
    /// Returns an error if the command exits with a non-zero status, including
    /// stderr in the error message.
    async fn run(&self, program: &str, args: &[&str]) -> anyhow::Result<String> {
        let output = self
            .executor
            .run(program, args)
            .await
            .map_err(|e| anyhow::anyhow!("failed to execute `{program}`: {e}"))?;

        if !output.success {
            anyhow::bail!("`{program} {}` failed: {}", args.join(" "), output.stderr);
        }

        Ok(output.stdout)
    }
}

/// Entry from `ip -json rule list`.
#[derive(Debug, Deserialize)]
pub struct IpRuleEntry {
    /// Source prefix (e.g. "192.168.1.100").
    #[serde(default)]
    pub src: Option<String>,
    /// Routing table number or name.
    #[serde(default)]
    pub table: Option<String>,
}

/// Parse the JSON output of `ip -json rule list` and return Wardnet-managed rules.
///
/// Wardnet-managed rules are those with a source IP and a numeric routing table >= 100.
/// Returns tuples of (`source_ip`, `table_number`).
pub fn parse_wardnet_rules(json: &str) -> anyhow::Result<Vec<(String, u32)>> {
    let entries: Vec<IpRuleEntry> = serde_json::from_str(json)
        .map_err(|e| anyhow::anyhow!("failed to parse `ip rule list` JSON: {e}"))?;

    let mut rules = Vec::new();
    for entry in entries {
        if let (Some(src), Some(table_str)) = (entry.src, entry.table)
            && let Ok(table) = table_str.parse::<u32>()
            && table >= 100
        {
            rules.push((src, table));
        }
    }
    Ok(rules)
}

#[async_trait]
impl PolicyRouter for IproutePolicyRouter {
    async fn enable_ip_forwarding(&self) -> anyhow::Result<()> {
        // Write directly to /proc/sys instead of using sysctl, which requires
        // CAP_SYS_ADMIN. Writing to procfs works with CAP_NET_ADMIN.
        match tokio::fs::write("/proc/sys/net/ipv4/ip_forward", "1").await {
            Ok(()) => Ok(()),
            Err(_) => {
                // Fall back to sysctl for environments where procfs isn't writable
                // (e.g. containers, or testing on macOS).
                self.run("sysctl", &["-w", "net.ipv4.ip_forward=1"]).await?;
                Ok(())
            }
        }
    }

    async fn add_route_table(&self, interface: &str, table: u32) -> anyhow::Result<()> {
        let table_str = table.to_string();
        let output = self
            .executor
            .run(
                "ip",
                &[
                    "route", "add", "default", "dev", interface, "table", &table_str,
                ],
            )
            .await
            .map_err(|e| anyhow::anyhow!("failed to execute `ip`: {e}"))?;

        if !output.success {
            // Ignore "File exists" — the route is already present (idempotent).
            if output.stderr.contains("File exists") {
                tracing::debug!(interface, table, "route already exists, skipping");
                return Ok(());
            }
            anyhow::bail!(
                "`ip route add default dev {} table {}` failed: {}",
                interface,
                table,
                output.stderr
            );
        }
        Ok(())
    }

    async fn remove_route_table(&self, table: u32) -> anyhow::Result<()> {
        self.run(
            "ip",
            &["route", "del", "default", "table", &table.to_string()],
        )
        .await?;
        Ok(())
    }

    async fn has_route_table(&self, table: u32) -> anyhow::Result<bool> {
        let output = self
            .run("ip", &["route", "show", "table", &table.to_string()])
            .await?;
        Ok(!output.trim().is_empty())
    }

    async fn add_ip_rule(&self, src_ip: &str, table: u32) -> anyhow::Result<()> {
        self.run(
            "ip",
            &["rule", "add", "from", src_ip, "lookup", &table.to_string()],
        )
        .await?;
        Ok(())
    }

    async fn remove_ip_rule(&self, src_ip: &str, table: u32) -> anyhow::Result<()> {
        self.run(
            "ip",
            &["rule", "del", "from", src_ip, "lookup", &table.to_string()],
        )
        .await?;
        Ok(())
    }

    async fn list_wardnet_rules(&self) -> anyhow::Result<Vec<(String, u32)>> {
        let output = self.run("ip", &["-json", "rule", "list"]).await?;
        parse_wardnet_rules(&output)
    }

    async fn flush_conntrack(&self, src_ip: &str) -> anyhow::Result<()> {
        // `conntrack -D -s <ip>` deletes tracked flows whose original source
        // matches. conntrack prints each deleted entry to stdout and a summary
        // "N flow entries have been deleted." to stderr. When N > 0 it exits 0;
        // when N = 0 ("0 flow entries have been deleted.") it exits non-zero.
        // Both are benign here.
        let output = match self.executor.run("conntrack", &["-D", "-s", src_ip]).await {
            Ok(o) => o,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tracing::warn!(
                    "`conntrack` command not found; install conntrack to enable \
                     conntrack flushing on routing changes (without it, existing \
                     flows stay pinned to their previous route)"
                );
                return Ok(());
            }
            Err(e) => return Err(anyhow::anyhow!("failed to execute `conntrack`: {e}")),
        };

        // Count deleted flows from the stderr summary. The line looks like:
        //   `conntrack v1.4.7 (conntrack-tools): 7 flow entries have been deleted.`
        // Find the integer immediately preceding "flow entries".
        let deleted = output
            .stderr
            .split_whitespace()
            .collect::<Vec<_>>()
            .windows(2)
            .find_map(|w| (w[1] == "flow").then(|| w[0].parse::<u32>().ok()).flatten())
            .unwrap_or(0);

        if output.success || output.stderr.contains("flow entries have been deleted") {
            tracing::info!(
                src_ip,
                deleted,
                "flushed conntrack entries for source IP"
            );
            return Ok(());
        }

        anyhow::bail!(
            "`conntrack -D -s {}` failed: {}",
            src_ip,
            output.stderr.trim()
        )
    }

    async fn flush_route_cache(&self) -> anyhow::Result<()> {
        self.run("ip", &["route", "flush", "cache"]).await?;
        tracing::info!("flushed kernel route cache");
        Ok(())
    }

    async fn check_tools_available(&self) -> anyhow::Result<()> {
        self.run("ip", &["-V"])
            .await
            .map_err(|_| anyhow::anyhow!("`ip` command not found; install iproute2"))?;
        self.run("sysctl", &["--version"])
            .await
            .map_err(|_| anyhow::anyhow!("`sysctl` command not found; install procps"))?;
        Ok(())
    }
}
