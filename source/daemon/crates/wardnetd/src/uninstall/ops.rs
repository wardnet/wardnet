//! The side effects `wardnetd uninstall` performs, behind one trait.
//!
//! Everything here shells out, touches the filesystem, or talks to the kernel.
//! None of it is unit-testable, and all of it is a thin pass-through with no
//! decisions in it — the decisions live in [`super`], which drives this trait
//! and is tested against a recording double. That split is what lets the
//! interesting behaviour (abort when the daemon will not stop, keep the account
//! when the re-own fails, report honestly on partial failure) be covered
//! without mocking `systemctl`, `chown` and `userdel` at the process level.
//!
//! Same instinct as `FirewallManager` and `WatchdogOps`: the risky, untestable
//! edge is a trait, and the logic above it is ordinary code.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;

use crate::shutdown::{TunnelTeardown, teardown_runtime_state};
use crate::uninstall::inventory::UNITS;

/// Why a `systemctl` invocation failed.
///
/// Typed rather than a flat string so callers can tell "unit not loaded"
/// (exit 5, benign on a re-run) and "inactive" (exit 3) from a real failure.
#[derive(Debug)]
pub enum SystemctlError {
    Spawn(std::io::Error),
    Status {
        args: String,
        code: Option<i32>,
        stderr: String,
    },
}

impl std::fmt::Display for SystemctlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn(e) => write!(f, "failed to run systemctl: {e}"),
            Self::Status { args, code, stderr } => {
                let code = code.map_or_else(|| "signal".to_owned(), |c| c.to_string());
                write!(f, "systemctl {args} exited with {code}: {stderr}")
            }
        }
    }
}

impl std::error::Error for SystemctlError {}

/// Every side effect the uninstaller performs.
#[async_trait]
pub trait UninstallOps: Send + Sync {
    /// Resolve symlinks; `None` when the path cannot be resolved.
    fn canonicalize(&self, path: &Path) -> Option<PathBuf>;

    /// Whether anything exists at `path`, **including a dangling symlink**.
    fn path_present(&self, path: &Path) -> bool;

    /// Whether `path` resolves to something reachable (follows symlinks).
    fn is_reachable(&self, path: &Path) -> bool;

    /// Delete a file or directory. Absence is success.
    fn remove_path(&self, path: &Path) -> anyhow::Result<()>;

    /// `chown -R -h root:root -- <path>`.
    fn chown_root_recursive(&self, path: &Path) -> anyhow::Result<()>;

    async fn stop_unit(&self, unit: &str) -> Result<(), SystemctlError>;
    async fn unit_is_active(&self, unit: &str) -> Result<(), SystemctlError>;
    async fn disable_unit(&self, unit: &str);
    async fn daemon_reload(&self);

    /// Remove the system user. Absence is success.
    async fn delete_user(&self, user: &str) -> anyhow::Result<()>;

    /// Delete the nftables table and `WireGuard` interfaces; returns failures.
    async fn teardown_runtime_state(&self) -> Vec<String>;

    /// Every `wg_ward*` link currently on the host.
    async fn wardnet_links(&self) -> anyhow::Result<Vec<String>>;

    /// Best-effort `ip link delete`, for links the `WireGuard` family cannot see.
    async fn delete_link(&self, name: &str);

    fn is_root(&self) -> bool;

    /// Prompt on the controlling terminal and read one line back.
    fn prompt(&self, prompt: &str) -> anyhow::Result<String>;
}

/// The real implementation.
pub struct LinuxUninstallOps;

#[async_trait]
impl UninstallOps for LinuxUninstallOps {
    fn canonicalize(&self, path: &Path) -> Option<PathBuf> {
        std::fs::canonicalize(path).ok()
    }

    fn path_present(&self, path: &Path) -> bool {
        std::fs::symlink_metadata(path).is_ok()
    }

    fn is_reachable(&self, path: &Path) -> bool {
        std::fs::metadata(path).is_ok()
    }

    fn remove_path(&self, path: &Path) -> anyhow::Result<()> {
        let metadata = match std::fs::symlink_metadata(path) {
            Ok(m) => m,
            // Absence is the normal case on a re-run or a partial install.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };

        if metadata.is_dir() {
            std::fs::remove_dir_all(path)?;
        } else {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    fn chown_root_recursive(&self, target: &Path) -> anyhow::Result<()> {
        // `-h` acts on symlinks themselves and `--` stops the path being read
        // as an option, so nothing under the tree — which the departing,
        // network-facing `wardnet` account could write — can redirect the
        // ownership change. The operand is symlink-resolved by the caller, so
        // `-h` cannot turn the whole traversal into a no-op.
        let status = std::process::Command::new("chown")
            .args([
                std::ffi::OsStr::new("-R"),
                std::ffi::OsStr::new("-h"),
                std::ffi::OsStr::new("root:root"),
                std::ffi::OsStr::new("--"),
                target.as_os_str(),
            ])
            .status()?;
        if status.success() {
            Ok(())
        } else {
            anyhow::bail!("chown exited with {status}")
        }
    }

    async fn stop_unit(&self, unit: &str) -> Result<(), SystemctlError> {
        systemctl(&["stop", unit]).await
    }

    async fn unit_is_active(&self, unit: &str) -> Result<(), SystemctlError> {
        systemctl(&["is-active", "--quiet", unit]).await
    }

    async fn disable_unit(&self, unit: &str) {
        // Already-disabled or missing units make this a no-op.
        let _ = systemctl(&["disable", unit]).await;
    }

    async fn daemon_reload(&self) {
        let _ = systemctl(&["daemon-reload"]).await;
    }

    async fn delete_user(&self, user: &str) -> anyhow::Result<()> {
        let output = tokio::process::Command::new("userdel")
            .arg(user)
            .output()
            .await?;

        // Exit 6 is "user does not exist" — idempotent success.
        if output.status.success() || output.status.code() == Some(6) {
            return Ok(());
        }
        anyhow::bail!(
            "userdel exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )
    }

    async fn teardown_runtime_state(&self) -> Vec<String> {
        let firewall: Arc<dyn wardnetd_services::routing::FirewallManager> =
            Arc::new(crate::firewall_netlink::NetlinkFirewallManager::new());
        let tunnels: Arc<dyn wardnetd_services::tunnel::TunnelInterface> =
            Arc::new(crate::tunnel_interface_wireguard::WireGuardTunnelInterface);
        let inbound: Arc<dyn wardnetd_services::inbound_wg::InboundWgInterface> =
            Arc::new(crate::inbound_wg_interface_wireguard::WireGuardInboundInterface);

        // Interface-level teardown, not service-level: uninstall has no
        // database to keep in step, and with `--purge` it is about to delete
        // the one there.
        teardown_runtime_state(&firewall, TunnelTeardown::Interface(&tunnels), &inbound).await
    }

    async fn wardnet_links(&self) -> anyhow::Result<Vec<String>> {
        let output = tokio::process::Command::new("ip")
            .args(["-o", "link", "show"])
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("failed to run `ip link show`: {e}"))?;

        if !output.status.success() {
            anyhow::bail!(
                "`ip link show` exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        Ok(parse_link_names(&String::from_utf8_lossy(&output.stdout)))
    }

    async fn delete_link(&self, name: &str) {
        crate::wireguard_interface::remove_stale_link(name, "wardnet interface").await;
    }

    fn is_root(&self) -> bool {
        // SAFETY: `geteuid` takes no arguments, reads process-local state and
        // cannot fail.
        unsafe { libc::geteuid() == 0 }
    }

    fn prompt(&self, prompt: &str) -> anyhow::Result<String> {
        read_from_terminal(prompt)
    }
}

/// Pull `wg_ward*` names out of `ip -o link show` output.
///
/// Lines look like `2: eth0: <BROADCAST,...> mtu ...`, and VLANs/peers render
/// as `name@parent`. Split out so the parsing — the one part of this module
/// with a way to be wrong — is testable without a kernel.
#[must_use]
pub fn parse_link_names(stdout: &str) -> Vec<String> {
    use wardnetd_services::tunnel::interface::TUNNEL_INTERFACE_PREFIX;

    stdout
        .lines()
        .filter_map(|line| line.split(": ").nth(1))
        .map(|name| name.split('@').next().unwrap_or(name).trim().to_owned())
        .filter(|name| name.starts_with(TUNNEL_INTERFACE_PREFIX))
        .collect()
}

/// The units the installer places, in the order it places them.
#[must_use]
pub fn installed_units() -> [&'static str; 3] {
    UNITS
}

async fn systemctl(args: &[&str]) -> Result<(), SystemctlError> {
    let output = tokio::process::Command::new("systemctl")
        .args(args)
        .output()
        .await
        .map_err(SystemctlError::Spawn)?;

    if output.status.success() {
        return Ok(());
    }
    Err(SystemctlError::Status {
        args: args.join(" "),
        code: output.status.code(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    })
}

/// Prompt on the controlling terminal and read one line back from it.
///
/// Reading `/dev/tty` rather than stdin is what makes the documented
/// `curl -sSL … | sudo bash -s -- --uninstall` form work: there the script
/// arrives on stdin, `exec` preserves it, and stdin is a spent pipe, so a
/// stdin-only prompt would make the advertised command fail every time. When
/// there is genuinely no terminal (cron, CI), opening `/dev/tty` fails and the
/// caller refuses.
fn read_from_terminal(prompt: &str) -> anyhow::Result<String> {
    use std::io::{BufRead, IsTerminal, Write};

    if std::io::stdin().is_terminal() {
        print!("{prompt}");
        std::io::stdout().flush()?;
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer)?;
        return Ok(answer);
    }

    let tty = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
        .map_err(|_| {
            anyhow::anyhow!("refusing to uninstall without --yes: no terminal to confirm on")
        })?;

    let mut writer = &tty;
    write!(writer, "{prompt}")?;
    writer.flush()?;

    let mut answer = String::new();
    std::io::BufReader::new(&tty).read_line(&mut answer)?;
    Ok(answer)
}
