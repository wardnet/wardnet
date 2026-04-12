use std::sync::Arc;

use async_trait::async_trait;

use crate::command::{CommandExecutor, CommandOutput};
use crate::firewall::FirewallManager;

/// Production [`FirewallManager`] implementation backed by the Linux `nft` CLI tool.
///
/// All operations are delegated to a [`CommandExecutor`], making this struct
/// fully testable without requiring real nftables on the host. Requires
/// `CAP_NET_ADMIN` capability and the `nftables` package at runtime.
#[derive(Debug)]
pub struct NftablesFirewallManager {
    executor: Arc<dyn CommandExecutor>,
}

impl NftablesFirewallManager {
    /// Create a new nftables firewall manager using the given command executor.
    pub fn new(executor: Arc<dyn CommandExecutor>) -> Self {
        Self { executor }
    }

    /// Run `nft` with the given arguments and return stdout on success.
    async fn run(&self, args: &[&str]) -> anyhow::Result<String> {
        let output = self
            .executor
            .run("nft", args)
            .await
            .map_err(|e| anyhow::anyhow!("failed to execute nft: {e}"))?;

        check_success(&output, &format!("nft {}", args.join(" ")))?;
        Ok(output.stdout)
    }

    /// Run `nft -f -` with the given script piped via stdin for atomic operations.
    async fn run_stdin(&self, script: &str) -> anyhow::Result<String> {
        let output = self
            .executor
            .run_with_stdin("nft", &["-f", "-"], script)
            .await
            .map_err(|e| anyhow::anyhow!("failed to spawn nft: {e}"))?;

        check_success(&output, "nft -f -")?;
        Ok(output.stdout)
    }
}

/// Check that a command finished successfully, returning an error with stderr on failure.
fn check_success(output: &CommandOutput, description: &str) -> anyhow::Result<()> {
    if !output.success {
        anyhow::bail!("{description} failed: {}", output.stderr.trim());
    }
    Ok(())
}

/// Parse `nft -a list chain` output to find the handle of a rule whose line
/// contains the given comment string.
///
/// Lines look like:
/// ```text
///   oifname "wg_ward0" masquerade comment "\"wardnet:wg_ward0\"" # handle 4
/// ```
///
/// Returns `None` when no matching rule is found.
#[must_use]
pub fn parse_rule_handle(output: &str, comment: &str) -> Option<u64> {
    for line in output.lines() {
        if !line.contains(comment) {
            continue;
        }
        // Lines end with `# handle <N>`
        if let Some(handle_str) = line.rsplit("# handle ").next()
            && let Ok(handle) = handle_str.trim().parse::<u64>()
        {
            return Some(handle);
        }
    }
    None
}

#[async_trait]
impl FirewallManager for NftablesFirewallManager {
    async fn init_wardnet_table(&self) -> anyhow::Result<()> {
        // Use `add` (not `create`) so the command is idempotent — it won't
        // error if the table or chains already exist.
        let script = "\
add table inet wardnet
add chain inet wardnet postrouting { type nat hook postrouting priority 100 ; policy accept ; }
add chain inet wardnet prerouting { type nat hook prerouting priority -100 ; policy accept ; }
";
        self.run_stdin(script).await?;
        tracing::info!("nftables: wardnet table initialised");
        Ok(())
    }

    async fn flush_wardnet_table(&self) -> anyhow::Result<()> {
        self.run(&["flush", "table", "inet", "wardnet"]).await?;
        tracing::info!("nftables: wardnet table flushed");
        Ok(())
    }

    async fn add_masquerade(&self, interface: &str) -> anyhow::Result<()> {
        let comment = format!("\"wardnet:{interface}\"");
        self.run(&[
            "add",
            "rule",
            "inet",
            "wardnet",
            "postrouting",
            "oifname",
            interface,
            "masquerade",
            "comment",
            &comment,
        ])
        .await?;
        tracing::info!(interface, "nftables: masquerade rule added");
        Ok(())
    }

    async fn remove_masquerade(&self, interface: &str) -> anyhow::Result<()> {
        let comment = format!("\"wardnet:{interface}\"");
        let output = self
            .run(&["-a", "list", "chain", "inet", "wardnet", "postrouting"])
            .await?;

        match parse_rule_handle(&output, &comment) {
            Some(h) => {
                let handle_str = h.to_string();
                self.run(&[
                    "delete",
                    "rule",
                    "inet",
                    "wardnet",
                    "postrouting",
                    "handle",
                    &handle_str,
                ])
                .await?;
                tracing::info!(interface, handle = h, "nftables: masquerade rule removed");
            }
            None => {
                tracing::warn!(
                    interface,
                    "nftables: masquerade rule not found, nothing to remove"
                );
            }
        }

        Ok(())
    }

    async fn add_dns_redirect(&self, device_ip: &str, dns_ip: &str) -> anyhow::Result<()> {
        let comment = format!("\"wardnet:dns:{device_ip}\"");
        self.run(&[
            "add",
            "rule",
            "inet",
            "wardnet",
            "prerouting",
            "ip",
            "saddr",
            device_ip,
            "udp",
            "dport",
            "53",
            "dnat",
            "to",
            dns_ip,
            "comment",
            &comment,
        ])
        .await?;
        tracing::info!(device_ip, dns_ip, "nftables: DNS redirect rule added");
        Ok(())
    }

    async fn remove_dns_redirect(&self, device_ip: &str) -> anyhow::Result<()> {
        let comment = format!("\"wardnet:dns:{device_ip}\"");
        let output = self
            .run(&["-a", "list", "chain", "inet", "wardnet", "prerouting"])
            .await?;

        match parse_rule_handle(&output, &comment) {
            Some(h) => {
                let handle_str = h.to_string();
                self.run(&[
                    "delete",
                    "rule",
                    "inet",
                    "wardnet",
                    "prerouting",
                    "handle",
                    &handle_str,
                ])
                .await?;
                tracing::info!(device_ip, handle = h, "nftables: DNS redirect rule removed");
            }
            None => {
                tracing::warn!(
                    device_ip,
                    "nftables: DNS redirect rule not found, nothing to remove"
                );
            }
        }

        Ok(())
    }

    async fn check_tools_available(&self) -> anyhow::Result<()> {
        let version = self.run(&["--version"]).await?;
        tracing::info!(version = version.trim(), "nftables: nft tool available");
        Ok(())
    }

    async fn destroy_wardnet_table(&self) -> anyhow::Result<()> {
        match self.run(&["delete", "table", "inet", "wardnet"]).await {
            Ok(_) => {
                tracing::info!("nftables: wardnet table destroyed");
            }
            Err(e) => {
                // Ignore errors if the table doesn't exist (e.g. first run or already cleaned up).
                tracing::debug!(error = %e, "nftables: ignoring error during table destruction");
            }
        }
        Ok(())
    }
}
