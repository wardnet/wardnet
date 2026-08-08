//! `wardnetd uninstall` — the supported way to remove Wardnet from a host.
//!
//! # Why this lives in the daemon rather than in the install script
//!
//! ADR 0013 moved nftables management to pure netlink (`rustables`), so the
//! `nft` CLI is deliberately *not* an install dependency and may not exist on
//! the box at all. A shell uninstaller therefore cannot be relied on to delete
//! `table inet wardnet`. The daemon binary can, because it links the same
//! netlink code that created the table — the uninstaller calls the same
//! abstraction that installed the state.
//!
//! `install.sh` still writes a small `/usr/local/sbin/wardnet-uninstall`
//! wrapper, which execs this subcommand when the binary works. When it does
//! not, the wrapper falls back to a deliberately dumb file-removal path: it
//! still confirms before acting, deletes the table via the `nft` CLI if that
//! happens to be installed, and otherwise exits non-zero saying firewall state
//! may remain rather than claiming a clean uninstall.
//!
//! # Two tiers
//!
//! The default run removes the daemon, units, user, config and kernel state
//! but **keeps** the `SQLite` database and secret store, printing where they
//! are. `--purge` destroys them. Retained data is re-owned to root, because
//! the `wardnet` user is deleted in the same run and an orphaned UID can be
//! silently reassigned to an unrelated service later.
//!
//! # Safety posture
//!
//! Inverted relative to `install.sh`: the installer degrades to
//! non-interactive defaults when there is no tty, because installing is safe.
//! Uninstalling is not, so with no way to reach a terminal and no `--yes` this
//! refuses outright. The confirmation is read from `/dev/tty` rather than
//! stdin, so the piped `curl … | sudo bash -s -- --uninstall` form can still
//! answer it — see [`confirm`].

use std::io::{BufRead, IsTerminal, Write};
use std::path::Path;
use std::sync::Arc;

use clap::Args;
use wardnetd_services::tunnel::interface::TUNNEL_INTERFACE_PREFIX;

use crate::shutdown::{TunnelTeardown, teardown_runtime_state};

pub mod inventory;

#[cfg(test)]
mod tests;

use inventory::{Action, Item, Kind, SERVICE_USER, UNITS};

/// Flags for `wardnetd uninstall`.
#[allow(clippy::struct_excessive_bools)] // four independent CLI flags, not a state machine
#[derive(Args, Debug, Clone, Default)]
pub struct UninstallArgs {
    /// Also delete `/var/lib/wardnet` — the `SQLite` database, `WireGuard`
    /// private keys, backup passphrase and DDNS credentials. Unrecoverable.
    #[arg(long)]
    pub purge: bool,

    /// Print everything that would be removed and exit without touching
    /// anything.
    #[arg(long)]
    pub dry_run: bool,

    /// Skip the confirmation prompt. Required to uninstall non-interactively.
    #[arg(long, short)]
    pub yes: bool,

    /// Mirror `install.sh --container-mode`: skip the sysctl, dhcpcd and
    /// module-load drop-ins the installer never wrote in a container.
    #[arg(long)]
    pub container_mode: bool,
}

/// Outcome of a single removal step, accumulated so the summary can be honest
/// about partial failure rather than claiming success.
struct Report {
    failures: Vec<String>,
}

impl Report {
    fn new() -> Self {
        Self {
            failures: Vec::new(),
        }
    }

    fn record(&mut self, what: &str, result: anyhow::Result<()>) {
        if let Err(e) = result {
            eprintln!("  ! failed to remove {what}: {e}");
            self.failures.push(format!("{what}: {e}"));
        }
    }

    /// Record an already-formatted failure (the caller has printed it).
    fn push_failure(&mut self, failure: String) {
        self.failures.push(failure);
    }

    /// Whether some recorded failure already names this exact interface.
    ///
    /// The typed teardown and the `ip link` sweep both touch the same
    /// interfaces, so without this an interface that resisted both would be
    /// counted twice in the closing summary.
    ///
    /// Matching is on whole identifiers, not substrings: `wg_ward1` occurs
    /// inside `wg_ward10`, and a substring test would treat a still-live
    /// `wg_ward10` as already reported and drop it from the summary — turning
    /// a duplicate-reporting nit into the under-reporting this module exists to
    /// avoid.
    pub(crate) fn mentions_interface(&self, name: &str) -> bool {
        self.failures
            .iter()
            .any(|failure| mentions_identifier(failure, name))
    }
}

/// Run the uninstaller. Returns an error when any step failed, so the process
/// exits non-zero and a caller (or the wrapper script) can tell.
pub async fn run(args: &UninstallArgs) -> anyhow::Result<()> {
    let items = inventory::build_inventory(args.purge, args.container_mode);

    print_warnings(args);

    println!("The following will be touched:\n");
    print!("{}", inventory::render_plan(&items));
    println!();

    // Checked after the plan is printed but before anything else, so
    // `--dry-run` stays usable unprivileged. Without this, an unprivileged run
    // would prompt for confirmation and then fail every single step with a
    // wall of permission errors instead of one clear message.
    if args.dry_run {
        println!("--dry-run: nothing was changed.");
        return Ok(());
    }
    require_root()?;

    if !confirm(args)? {
        println!("Aborted; nothing was changed.");
        return Ok(());
    }

    let mut report = Report::new();

    // Resolved once, before anything is deleted. A symlinked data directory (a
    // database moved to another disk) would otherwise make `--purge` unlink
    // just the symlink while we printed that the keys were destroyed, and make
    // the retained-data `chown` a no-op on the real tree.
    let data_dir = resolved_data_dir();

    // Abort rather than continue past a daemon that would not stop. Removing
    // the binary, units and interfaces out from under a live daemon risks the
    // forced kill that leaves the hardware watchdog armed and reboots this
    // host. Nothing has been changed at this point, so bailing is clean.
    if !stop_service(&mut report).await {
        anyhow::bail!(
            "could not confirm wardnetd.service is stopped; refusing to remove anything \
             while the daemon may still be running (a forced kill would leave the hardware \
             watchdog armed and reboot this host). Investigate with `systemctl status \
             wardnetd`, stop it by hand, then re-run. Nothing has been changed."
        );
    }

    teardown_kernel_state(&mut report).await;
    remove_units(&mut report).await;
    remove_paths(&items, &data_dir, &mut report);

    // Deleting the user is what turns a failed re-own into the orphaned-UID
    // hazard this step exists to prevent, so if the chown did not succeed the
    // account stays. A leftover system user is trivially removable by hand; a
    // recycled UID silently owning the WireGuard keys is not.
    // `protect_retained_data` prints the specific reason when it declines —
    // "the purge left the tree behind" and "the chown failed" are different
    // facts, and reporting one as the other is the kind of small lie this
    // command must not tell.
    let data_is_root_owned = protect_retained_data(args, &data_dir, &mut report);
    if data_is_root_owned {
        remove_user(&mut report).await;
    }
    reload_systemd().await;

    finish(args, &items, &report, data_is_root_owned)
}

/// The data directory with any symlink resolved.
///
/// Operators do relocate the database to another disk by symlinking
/// `/var/lib/wardnet`. Every destructive step has to act on the real tree:
/// `remove_dir_all` on a symlink unlinks only the link, and `chown -R -h` on a
/// symlink operand re-owns the link without traversing into it — both would
/// report success while the database and secret store sat untouched.
///
/// Falls back to the literal path when it does not exist or cannot be resolved.
fn resolved_data_dir() -> std::path::PathBuf {
    std::fs::canonicalize(inventory::DATA_DIR)
        .unwrap_or_else(|_| std::path::PathBuf::from(inventory::DATA_DIR))
}

/// Whether anything exists at `path`, including a symlink whose target does
/// not. `Path::exists` follows links and so answers "no" for a dangling one,
/// which for our purposes is the wrong answer: the link is still there, and
/// whatever it points at may simply be unmounted.
fn path_present(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

/// Whether `haystack` contains `name` as a whole identifier rather than as a
/// substring of a longer one.
///
/// Interface names are `[A-Za-z0-9_]`, so splitting on anything else yields the
/// identifiers in a message and lets `wg_ward1` and `wg_ward10` be told apart.
pub(crate) fn mentions_identifier(haystack: &str, name: &str) -> bool {
    haystack
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .any(|token| token == name)
}

/// Everything past the plan needs `CAP_NET_ADMIN` and write access to `/etc`,
/// `/usr/local` and `/var/lib`. Fail with one clear message rather than a dozen
/// permission errors.
fn require_root() -> anyhow::Result<()> {
    // SAFETY: `geteuid` is always safe — it takes no arguments, reads
    // process-local state and cannot fail.
    let euid = unsafe { libc::geteuid() };
    if euid == 0 {
        return Ok(());
    }
    anyhow::bail!("uninstall must be run as root (try: sudo wardnet-uninstall)")
}

/// Everything the operator needs to know *before* deciding, not after.
fn print_warnings(args: &UninstallArgs) {
    println!("=== Uninstalling Wardnet ===\n");

    println!(
        "This host is your network's DHCP and DNS server. Once Wardnet is gone,\n\
         devices on the LAN will have no addressing and no name resolution until\n\
         you re-enable DHCP on your router. Do that first if you can.\n"
    );

    if !args.container_mode {
        println!(
            "If you installed with --static-ip, removing /etc/dhcpcd.conf.d/wardnet.conf\n\
             means this host reverts to DHCP at next boot and may come back on a\n\
             different address. Note the current one before rebooting.\n"
        );
    }

    if args.purge {
        println!(
            "--purge DESTROYS /var/lib/wardnet: the SQLite database, your WireGuard\n\
             private keys, the backup passphrase and any DDNS credentials. There is\n\
             no way to recover them. Take a backup first if there is any doubt.\n"
        );
    } else {
        println!(
            "Your database and secrets under /var/lib/wardnet will be KEPT and\n\
             re-owned to root. Re-run with --purge to destroy them.\n"
        );
    }
}

/// Ask before doing anything destructive.
///
/// With no way to ask, this refuses rather than assuming yes — the opposite of
/// `install.sh`, which safely degrades to defaults when piped.
///
/// Reading the answer from `/dev/tty` rather than stdin is what makes the
/// documented `curl -sSL … | sudo bash -s -- --uninstall` form work. In that
/// shape the script arrives on stdin, `exec` preserves it, and stdin is a spent
/// pipe — so a stdin-only prompt would make the advertised command fail every
/// time. The controlling terminal is still there; we just have to ask it
/// directly. When there is genuinely no terminal (cron, a CI step), opening
/// `/dev/tty` fails and we refuse.
fn confirm(args: &UninstallArgs) -> anyhow::Result<bool> {
    if args.yes {
        return Ok(true);
    }

    // `--purge` is unrecoverable, so a bare "y" is not enough.
    let (prompt, expected) = if args.purge {
        ("Type PURGE to destroy all data and uninstall: ", "PURGE")
    } else {
        ("Type yes to uninstall: ", "yes")
    };

    let answer = read_from_terminal(prompt)?;
    Ok(answer.trim() == expected)
}

/// Prompt on the controlling terminal and read one line back from it.
fn read_from_terminal(prompt: &str) -> anyhow::Result<String> {
    let no_terminal =
        || anyhow::anyhow!("refusing to uninstall without --yes: no terminal to confirm on");

    // Prefer stdin when it really is a terminal; fall back to /dev/tty for the
    // piped case.
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
        .map_err(|_| no_terminal())?;

    let mut writer = &tty;
    write!(writer, "{prompt}")?;
    writer.flush()?;

    let mut answer = String::new();
    std::io::BufReader::new(&tty).read_line(&mut answer)?;
    Ok(answer)
}

/// Stop the unit cleanly.
///
/// Never a SIGKILL: the daemon holds `/dev/watchdog` and disarms it via the
/// kernel magic-close on clean shutdown. Killing it ungracefully leaves the
/// hardware watchdog armed and reboots the host about fifteen seconds later,
/// in the middle of the uninstall. `systemctl stop` blocks until the unit is
/// down, so by the time this returns the daemon's own shutdown path has
/// already torn most of the kernel state down.
/// Returns `false` when the unit is demonstrably still running, in which case
/// the caller must abort rather than delete anything.
async fn stop_service(report: &mut Report) -> bool {
    println!("Stopping wardnetd (cleanly, so the hardware watchdog is disarmed)...");
    match systemctl(&["stop", "wardnetd.service"]).await {
        Ok(()) => {}
        // Exit 5 is "unit not loaded" — the normal answer on a re-run or a
        // partial install, and this module promises to tolerate both.
        Err(SystemctlError::Status { code: Some(5), .. }) => {
            println!("  wardnetd.service is not loaded; nothing to stop.");
        }
        // No systemd to talk to. Expected in a container or chroot, where
        // `systemctl` either is absent or exits 1 with "System has not been
        // booted with systemd". Treating that as a failure would make an
        // otherwise clean container uninstall report as partial.
        Err(SystemctlError::Spawn(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            println!("  systemctl is not available; skipping service stop.");
        }
        Err(SystemctlError::Status { stderr, .. }) if is_systemd_not_booted(&stderr) => {
            println!("  systemd is not running here; skipping service stop.");
        }
        Err(e) => report.record("stop wardnetd.service", Err(e.into())),
    }

    // A stop that timed out escalates to SIGKILL, which leaves the hardware
    // watchdog armed and reboots this host ~15s later — in the middle of the
    // uninstall. Only proceed on a *confirmed* stop: an unanswerable probe (a
    // D-Bus hiccup, an unexpected systemctl failure) says nothing about whether
    // the daemon is alive, and guessing "down" there is the guess that reboots
    // the box.
    match systemctl(&["is-active", "--quiet", "wardnetd.service"]).await {
        // Exit 0 — still running.
        Ok(()) => false,
        // 3 = inactive, 4 = no such unit. Both mean it is genuinely not up.
        Err(SystemctlError::Status {
            code: Some(3 | 4), ..
        }) => true,
        // No systemd to ask (container, chroot): nothing is supervising the
        // daemon, so there is no stop to confirm.
        Err(SystemctlError::Spawn(e)) if e.kind() == std::io::ErrorKind::NotFound => true,
        Err(SystemctlError::Status { ref stderr, .. }) if is_systemd_not_booted(stderr) => true,
        Err(e) => {
            eprintln!("  ! could not confirm wardnetd.service is stopped: {e}");
            false
        }
    }
}

/// Whether `systemctl` failed because there is no systemd to talk to, rather
/// than because the stop itself went wrong.
fn is_systemd_not_booted(stderr: &str) -> bool {
    stderr.contains("has not been booted with systemd")
        || stderr.contains("Failed to connect to bus")
}

/// Re-run the daemon's own teardown as a belt-and-braces sweep.
///
/// The clean stop above normally handles this, but if the daemon had
/// previously been killed ungracefully — or was not running at all — the
/// nftables table and `wg_ward*` interfaces are still live. Both operations
/// are idempotent, so repeating them costs nothing.
async fn teardown_kernel_state(report: &mut Report) {
    println!("Removing firewall table and wireguard interfaces...");

    let firewall: Arc<dyn wardnetd_services::routing::FirewallManager> =
        Arc::new(crate::firewall_netlink::NetlinkFirewallManager::new());
    let tunnels: Arc<dyn wardnetd_services::tunnel::TunnelInterface> =
        Arc::new(crate::tunnel_interface_wireguard::WireGuardTunnelInterface);
    let inbound: Arc<dyn wardnetd_services::inbound_wg::InboundWgInterface> =
        Arc::new(crate::inbound_wg_interface_wireguard::WireGuardInboundInterface);

    // There is no tracing subscriber on this path, so the returned failures are
    // the only way these reach the operator. Leaving a live firewall table
    // behind while printing "Wardnet has been removed" is the worst outcome
    // this command could produce.
    // Interface-level teardown, not service-level: uninstall has no database
    // to keep in step, and with `--purge` it is about to delete the one there.
    for failure in
        teardown_runtime_state(&firewall, TunnelTeardown::Interface(&tunnels), &inbound).await
    {
        eprintln!("  ! {failure}");
        report.push_failure(failure);
    }

    // `ip link delete` reaps links the WireGuard netlink family cannot see —
    // wrong-type or partially-created leftovers from a crash.
    match wardnet_links().await {
        Ok(names) => {
            for name in &names {
                crate::wireguard_interface::remove_stale_link(name, "wardnet interface").await;
            }
            // `remove_stale_link` is best-effort and logs nothing we can see
            // here, so confirm by re-enumerating. A link we could not delete
            // must reach the summary rather than being reported as removed.
            match wardnet_links().await {
                Ok(remaining) => {
                    for name in remaining {
                        // The typed teardown may already have reported this
                        // interface; counting it again would overstate how much
                        // is left behind.
                        if report.mentions_interface(&name) {
                            continue;
                        }
                        let msg = format!("wireguard interface {name} could not be removed");
                        eprintln!("  ! {msg}");
                        report.push_failure(msg);
                    }
                }
                Err(e) => {
                    let msg = format!("could not confirm wireguard interfaces were removed: {e}");
                    eprintln!("  ! {msg}");
                    report.push_failure(msg);
                }
            }
        }
        Err(e) => {
            let msg = format!("could not enumerate wireguard interfaces: {e}");
            eprintln!("  ! {msg}");
            report.push_failure(msg);
        }
    }
}

/// Every `wg_ward*` link currently on the host, per `ip link`.
///
/// Enumerated rather than guessed from a fixed range, because the case this
/// sweep exists for is precisely the one where the typed teardown found nothing
/// — a failed enumeration, or a link of the wrong type that the `WireGuard`
/// netlink family cannot see. A hardcoded `wg_ward0..N` would leave anything
/// above `N` behind with nothing reporting it.
///
/// Errors propagate rather than collapsing to an empty list: "no wardnet links"
/// and "could not ask" must not look the same to the caller, or a failed sweep
/// would be reported as a clean one.
async fn wardnet_links() -> anyhow::Result<Vec<String>> {
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

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        // `ip -o link show` lines look like `2: eth0: <BROADCAST,...> mtu ...`.
        .filter_map(|line| line.split(": ").nth(1))
        // `ip -o link show` renders VLANs and peers as `name@parent`.
        .map(|name| name.split('@').next().unwrap_or(name).trim().to_owned())
        .filter(|name| name.starts_with(TUNNEL_INTERFACE_PREFIX))
        .collect())
}

async fn remove_units(report: &mut Report) {
    println!("Disabling systemd units...");
    for unit in UNITS {
        // Already-disabled units make this a no-op; a missing unit is fine.
        let _ = systemctl(&["disable", unit]).await;
        report.record(
            &format!("unit file {unit}"),
            remove_path(Path::new(&format!("/etc/systemd/system/{unit}"))),
        );
    }
}

fn remove_paths(items: &[Item], data_dir: &Path, report: &mut Report) {
    println!("Removing files...");
    for item in items {
        if item.action != Action::Remove {
            continue;
        }
        match item.kind {
            Kind::File | Kind::Directory => {
                // `--purge` lists the data directory by its literal path; delete
                // the resolved tree so a symlinked data dir really is destroyed,
                // then drop the symlink itself.
                if item.target == inventory::DATA_DIR {
                    report.record(&item.target, remove_path(data_dir));
                    if data_dir != Path::new(inventory::DATA_DIR) {
                        report.record(
                            inventory::DATA_DIR,
                            remove_path(Path::new(inventory::DATA_DIR)),
                        );
                    }
                    continue;
                }
                report.record(&item.target, remove_path(Path::new(&item.target)));
            }
            // Units are handled by `remove_units`; runtime state and the user
            // account have their own steps.
            Kind::Unit | Kind::User | Kind::Runtime => {}
        }
    }
}

/// Delete a file or directory, treating "already absent" as success.
///
/// Absence is the normal case on a re-run or a partial install, so it must not
/// count as a failure — that is what keeps the summary honest.
fn remove_path(path: &Path) -> anyhow::Result<()> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
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

/// Re-own retained data to root.
///
/// The `wardnet` user is deleted moments later. Left alone, the user's
/// database and private keys would be owned by a numeric UID with no account
/// behind it, which a future `useradd` on the same host can silently hand to
/// an unrelated service.
/// What the retained tree needs before the `wardnet` account can be deleted.
///
/// Split out from the filesystem work so the branch that decides whether the
/// user survives — the one guarding the orphaned-UID hazard — is testable
/// without mocking `chown`, `userdel` and the filesystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RetainedData {
    /// Nothing to protect: purge succeeded, or there was no data directory.
    Gone,
    /// Data is being kept and must be re-owned to root first.
    NeedsChown,
    /// `--purge` was asked for but the tree is still there, so the account has
    /// to stay — freeing the UID now would strand the secret store under it.
    PurgeIncomplete,
}

pub(crate) fn classify_retained_data(purge: bool, data_dir_exists: bool) -> RetainedData {
    match (purge, data_dir_exists) {
        (true, true) => RetainedData::PurgeIncomplete,
        (false, true) => RetainedData::NeedsChown,
        (_, false) => RetainedData::Gone,
    }
}

/// Returns whether the retained tree is now safely owned by root — `true` when
/// there was nothing to protect (purge, or no data directory).
fn protect_retained_data(args: &UninstallArgs, data_dir: &Path, report: &mut Report) -> bool {
    // `symlink_metadata`, not `exists()`: a dangling `/var/lib/wardnet` symlink
    // (unmounted disk, tree moved) satisfies neither `exists()` nor
    // `canonicalize`, so `exists()` would call it `Gone`, free the UID, and
    // announce "now owned by root" over data that is merely out of reach.
    let still_there = path_present(data_dir) || path_present(Path::new(inventory::DATA_DIR));
    match classify_retained_data(args.purge, still_there) {
        RetainedData::Gone => return true,
        RetainedData::PurgeIncomplete => {
            let msg = format!(
                "{} still exists after --purge; keeping the {SERVICE_USER} user so its \
                 data is not left owned by an orphaned UID",
                data_dir.display()
            );
            eprintln!("  ! {msg}");
            report.push_failure(msg);
            return false;
        }
        RetainedData::NeedsChown => {}
    }

    // Re-own the whole tree, not just the files we listed as retained. The
    // directory itself is created `wardnet:wardnet` mode 0750, so leaving it
    // owned by the departing user would let a later account reusing that UID
    // unlink or replace the database and `secrets/` even though the files
    // inside are root-owned. Re-owning the leaves alone would look like it
    // closed the hazard while leaving the door open.
    // Present but unresolvable: `/var/lib/wardnet` is a symlink whose target is
    // gone or unmounted, so `resolved_data_dir` fell back to the literal path.
    // `chown -R -h` on a symlink operand re-owns the link and exits 0 without
    // ever traversing, which would look like success and let the account go —
    // the very hazard this step exists to close.
    if std::fs::metadata(data_dir).is_err() {
        let msg = format!(
            "{} exists but its target cannot be reached (unmounted disk or broken \
             symlink); keeping the {SERVICE_USER} user rather than freeing a UID that \
             may still own the data",
            data_dir.display()
        );
        eprintln!("  ! {msg}");
        report.push_failure(msg);
        return false;
    }

    println!("Re-owning retained data to root...");
    match chown_root_recursive(data_dir) {
        Ok(()) => true,
        Err(e) => {
            report.record(&format!("chown {}", data_dir.display()), Err(e));
            eprintln!(
                "  ! keeping the {SERVICE_USER} user: {} could not be re-owned to root,\n    \
                 and removing the account would leave its data owned by an orphaned UID.",
                data_dir.display()
            );
            false
        }
    }
}

fn chown_root_recursive(target: &Path) -> anyhow::Result<()> {
    // `-h` acts on symlinks themselves and `--` stops the path being read as an
    // option, so nothing under the tree — which the departing, network-facing
    // `wardnet` account could write — can redirect the ownership change. The
    // operand is already symlink-resolved by `resolved_data_dir`, so `-h`
    // cannot turn the whole traversal into a no-op.
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

async fn remove_user(report: &mut Report) {
    println!("Removing the {SERVICE_USER} system user...");
    let output = tokio::process::Command::new("userdel")
        .arg(SERVICE_USER)
        .output()
        .await;

    let result = match output {
        // userdel exit code 6 means "user does not exist" — idempotent success.
        Ok(o) if o.status.success() || o.status.code() == Some(6) => Ok(()),
        Ok(o) => Err(anyhow::anyhow!(
            "userdel exited with {}: {}",
            o.status,
            String::from_utf8_lossy(&o.stderr).trim()
        )),
        Err(e) => Err(e.into()),
    };
    report.record(&format!("user {SERVICE_USER}"), result);
}

async fn reload_systemd() {
    let _ = systemctl(&["daemon-reload"]).await;
}

/// Why a `systemctl` invocation failed. Typed rather than a flat string so
/// callers can distinguish "unit not loaded" (exit 5, benign on a re-run) from
/// a genuine failure.
#[derive(Debug)]
enum SystemctlError {
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

/// Print the closing summary and decide the exit status.
fn finish(
    args: &UninstallArgs,
    items: &[Item],
    report: &Report,
    data_is_root_owned: bool,
) -> anyhow::Result<()> {
    println!();

    let kept: Vec<&Item> = items.iter().filter(|i| i.action == Action::Keep).collect();
    if !kept.is_empty() {
        // Only claim the ownership change when it actually happened — the
        // whole point of the line is to tell the reader their keys are safe.
        if data_is_root_owned {
            println!("Kept (now owned by root):");
        } else {
            println!("Kept (STILL OWNED BY THE wardnet UID — re-own these yourself):");
        }
        for item in &kept {
            println!("  {}", item.target);
        }
        println!(
            "\nThese are the default locations. If this host set a custom database,\n\
             secret-store or log path in /etc/wardnet/wardnet.toml, remove those\n\
             yourself — uninstall does not read the config."
        );
        println!("\nRestore from these with a fresh install, or delete them with:");
        println!("  sudo rm -rf {}\n", inventory::DATA_DIR);
    }

    if !args.container_mode {
        println!(
            "Note: net.ipv4.ip_forward stays 1 on the running kernel until you reboot.\n\
             The drop-in that persisted it has been removed.\n"
        );
    }

    if report.failures.is_empty() {
        println!("Wardnet has been removed.");
        return Ok(());
    }

    // Netdata's posture: say what is still there rather than claiming success.
    println!("Wardnet was removed, but some parts may still be present:");
    for failure in &report.failures {
        println!("  - {failure}");
    }
    anyhow::bail!("{} uninstall step(s) failed", report.failures.len())
}
