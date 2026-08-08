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
//! refuses outright.
//!
//! # Structure
//!
//! Every side effect goes through [`UninstallOps`]; this module only decides.
//! That is what makes the branches worth testing — abort when the daemon will
//! not stop, keep the account when the re-own fails, report honestly on
//! partial failure — reachable from tests with a recording double.

use std::path::{Path, PathBuf};

use clap::Args;

pub mod inventory;
pub mod ops;

#[cfg(test)]
mod tests;

use inventory::{Action, Item, Kind, SERVICE_USER, UNITS};
use ops::{LinuxUninstallOps, SystemctlError, UninstallOps};

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

/// Outcome of the removal steps, accumulated so the summary can be honest
/// about partial failure rather than claiming success.
#[derive(Default)]
pub(crate) struct Report {
    failures: Vec<String>,
}

impl Report {
    pub(crate) fn new() -> Self {
        Self::default()
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
    /// a duplicate-reporting nit into the under-reporting this module exists
    /// to avoid.
    pub(crate) fn mentions_interface(&self, name: &str) -> bool {
        self.failures
            .iter()
            .any(|failure| mentions_identifier(failure, name))
    }
}

/// Run the uninstaller against the real host.
pub async fn run(args: &UninstallArgs) -> anyhow::Result<()> {
    run_with(args, &LinuxUninstallOps).await
}

/// Run the uninstaller against `ops`.
///
/// Returns an error when any step failed, so the process exits non-zero and a
/// caller (or the wrapper script) can tell.
pub(crate) async fn run_with(args: &UninstallArgs, ops: &dyn UninstallOps) -> anyhow::Result<()> {
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
    if !ops.is_root() {
        anyhow::bail!("uninstall must be run as root (try: sudo wardnet-uninstall)");
    }

    if !confirm(args, ops)? {
        println!("Aborted; nothing was changed.");
        return Ok(());
    }

    let mut report = Report::new();

    // Resolved once, before anything is deleted. A symlinked data directory (a
    // database moved to another disk) would otherwise make `--purge` unlink
    // just the symlink while we printed that the keys were destroyed, and make
    // the retained-data `chown` a no-op on the real tree.
    let data_dir = resolved_data_dir(ops);

    // Abort rather than continue past a daemon that would not stop. Removing
    // the binary, units and interfaces out from under a live daemon risks the
    // forced kill that leaves the hardware watchdog armed and reboots this
    // host. Nothing has been changed at this point, so bailing is clean.
    if !stop_service(ops).await {
        anyhow::bail!(
            "could not confirm wardnetd.service is stopped; refusing to remove anything \
             while the daemon may still be running (a forced kill would leave the hardware \
             watchdog armed and reboot this host). Investigate with `systemctl status \
             wardnetd`, stop it by hand, then re-run. Nothing has been changed."
        );
    }

    teardown_kernel_state(ops, &mut report).await;
    remove_units(ops, &mut report).await;
    remove_paths(ops, &items, &data_dir, &mut report);

    // Deleting the user is what turns a failed re-own into the orphaned-UID
    // hazard this step exists to prevent, so if the chown did not succeed the
    // account stays. A leftover system user is trivially removable by hand; a
    // recycled UID silently owning the WireGuard keys is not.
    //
    // `protect_retained_data` prints the specific reason when it declines —
    // "the purge left the tree behind" and "the chown failed" are different
    // facts, and reporting one as the other is the kind of small lie this
    // command must not tell.
    let data_is_root_owned = protect_retained_data(ops, args, &data_dir, &mut report);
    if data_is_root_owned {
        remove_user(ops, &mut report).await;
    }
    ops.daemon_reload().await;

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
/// Falls back to the literal path when it cannot be resolved.
fn resolved_data_dir(ops: &dyn UninstallOps) -> PathBuf {
    ops.canonicalize(Path::new(inventory::DATA_DIR))
        .unwrap_or_else(|| PathBuf::from(inventory::DATA_DIR))
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

/// The exact word a tier requires before it will proceed.
///
/// `--purge` is unrecoverable, so a bare "y" is not enough.
pub(crate) fn confirmation_word(purge: bool) -> &'static str {
    if purge { "PURGE" } else { "yes" }
}

/// Ask before doing anything destructive.
fn confirm(args: &UninstallArgs, ops: &dyn UninstallOps) -> anyhow::Result<bool> {
    if args.yes {
        return Ok(true);
    }

    let expected = confirmation_word(args.purge);
    let prompt = if args.purge {
        "Type PURGE to destroy all data and uninstall: "
    } else {
        "Type yes to uninstall: "
    };

    Ok(ops.prompt(prompt)?.trim() == expected)
}

/// What a post-stop liveness probe told us.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Liveness {
    /// Confirmed down — safe to proceed.
    Stopped,
    /// Confirmed still up, or unanswerable. Either way, do not touch anything.
    NotConfirmed,
}

/// Interpret `systemctl is-active`.
///
/// Only a *confirmed* stop lets the uninstall proceed: an unanswerable probe (a
/// D-Bus hiccup, an unexpected systemctl failure) says nothing about whether
/// the daemon is alive, and guessing "down" there is the guess that reboots the
/// box via the still-armed hardware watchdog.
pub(crate) fn classify_liveness(probe: &Result<(), SystemctlError>) -> Liveness {
    match probe {
        // 3 = inactive, 4 = no such unit. Both mean it is genuinely not up.
        Err(SystemctlError::Status {
            code: Some(3 | 4), ..
        }) => Liveness::Stopped,
        // No systemd to ask (container, chroot): nothing is supervising the
        // daemon, so there is no stop to confirm.
        Err(SystemctlError::Spawn(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            Liveness::Stopped
        }
        Err(SystemctlError::Status { stderr, .. }) if is_systemd_not_booted(stderr) => {
            Liveness::Stopped
        }
        // `Ok(())` is exit 0 — still running. Any other failure tells us
        // nothing about liveness, and the two collapse deliberately: both mean
        // "not confirmed down", which is the only safe reading.
        Ok(()) | Err(_) => Liveness::NotConfirmed,
    }
}

/// Stop the unit cleanly.
///
/// Never a SIGKILL: the daemon holds `/dev/watchdog` and disarms it via the
/// kernel magic-close on clean shutdown. Killing it ungracefully leaves the
/// hardware watchdog armed and reboots the host about fifteen seconds later,
/// in the middle of the uninstall. `systemctl stop` blocks until the unit is
/// down, so by the time this returns the daemon's own shutdown path has
/// already torn most of the kernel state down.
///
/// Returns whether the unit is *confirmed* stopped.
async fn stop_service(ops: &dyn UninstallOps) -> bool {
    println!("Stopping wardnetd (cleanly, so the hardware watchdog is disarmed)...");
    match ops.stop_unit("wardnetd.service").await {
        Ok(()) => {}
        // Exit 5 is "unit not loaded" — the normal answer on a re-run or a
        // partial install, and this module promises to tolerate both.
        Err(SystemctlError::Status { code: Some(5), .. }) => {
            println!("  wardnetd.service is not loaded; nothing to stop.");
        }
        // No systemd to talk to: expected in a container or chroot.
        Err(SystemctlError::Spawn(ref e)) if e.kind() == std::io::ErrorKind::NotFound => {
            println!("  systemctl is not available; skipping service stop.");
        }
        Err(SystemctlError::Status { ref stderr, .. }) if is_systemd_not_booted(stderr) => {
            println!("  systemd is not running here; skipping service stop.");
        }
        Err(e) => eprintln!("  ! failed to stop wardnetd.service: {e}"),
    }

    let liveness = classify_liveness(&ops.unit_is_active("wardnetd.service").await);
    liveness == Liveness::Stopped
}

/// Whether `systemctl` failed because there is no systemd to talk to, rather
/// than because the operation itself went wrong.
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
async fn teardown_kernel_state(ops: &dyn UninstallOps, report: &mut Report) {
    println!("Removing firewall table and wireguard interfaces...");

    // There is no tracing subscriber on this path, so the returned failures
    // are the only way these reach the operator. Leaving a live firewall table
    // behind while printing "Wardnet has been removed" is the worst outcome
    // this command could produce.
    for failure in ops.teardown_runtime_state().await {
        eprintln!("  ! {failure}");
        report.push_failure(failure);
    }

    // `ip link delete` reaps links the WireGuard netlink family cannot see —
    // wrong-type or partially-created leftovers from a crash.
    match ops.wardnet_links().await {
        Ok(names) => {
            for name in &names {
                ops.delete_link(name).await;
            }
            // The delete is best-effort and reports nothing we can see here,
            // so confirm by re-enumerating. A link we could not delete must
            // reach the summary rather than being reported as removed.
            match ops.wardnet_links().await {
                Ok(remaining) => {
                    for name in remaining {
                        // The typed teardown may already have reported this
                        // interface; counting it again would overstate how
                        // much is left behind.
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

async fn remove_units(ops: &dyn UninstallOps, report: &mut Report) {
    println!("Disabling systemd units...");
    for unit in UNITS {
        ops.disable_unit(unit).await;
        report.record(
            &format!("unit file {unit}"),
            ops.remove_path(Path::new(&format!("/etc/systemd/system/{unit}"))),
        );
    }
}

fn remove_paths(ops: &dyn UninstallOps, items: &[Item], data_dir: &Path, report: &mut Report) {
    println!("Removing files...");
    for item in items {
        if item.action != Action::Remove {
            continue;
        }
        match item.kind {
            Kind::File | Kind::Directory => {
                // `--purge` lists the data directory by its literal path;
                // delete the resolved tree so a symlinked data dir really is
                // destroyed, then drop the symlink itself.
                if item.target == inventory::DATA_DIR {
                    report.record(&item.target, ops.remove_path(data_dir));
                    if data_dir != Path::new(inventory::DATA_DIR) {
                        report.record(
                            inventory::DATA_DIR,
                            ops.remove_path(Path::new(inventory::DATA_DIR)),
                        );
                    }
                    continue;
                }
                report.record(&item.target, ops.remove_path(Path::new(&item.target)));
            }
            // Units are handled by `remove_units`; runtime state and the user
            // account have their own steps.
            Kind::Unit | Kind::User | Kind::Runtime => {}
        }
    }
}

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

/// Re-own retained data to root.
///
/// The `wardnet` user is deleted moments later. Left alone, the user's
/// database and private keys would be owned by a numeric UID with no account
/// behind it, which a future `useradd` on the same host can silently hand to
/// an unrelated service.
///
/// Returns whether the retained tree is now safely owned by root — `true` when
/// there was nothing to protect.
fn protect_retained_data(
    ops: &dyn UninstallOps,
    args: &UninstallArgs,
    data_dir: &Path,
    report: &mut Report,
) -> bool {
    // `path_present` uses `symlink_metadata`, not `exists()`: a dangling
    // `/var/lib/wardnet` symlink (unmounted disk, tree moved) satisfies
    // neither `exists()` nor `canonicalize`, so `exists()` would call it
    // `Gone`, free the UID, and announce "now owned by root" over data that is
    // merely out of reach.
    let still_there =
        ops.path_present(data_dir) || ops.path_present(Path::new(inventory::DATA_DIR));
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

    // Present but unresolvable: `/var/lib/wardnet` is a symlink whose target
    // is gone or unmounted, so `resolved_data_dir` fell back to the literal
    // path. `chown -R -h` on a symlink operand re-owns the link and exits 0
    // without ever traversing, which would look like success and let the
    // account go — the very hazard this step exists to close.
    if !ops.is_reachable(data_dir) {
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

    // Re-own the whole tree, not just the files we listed as retained. The
    // directory itself is created `wardnet:wardnet` mode 0750, so leaving it
    // owned by the departing user would let a later account reusing that UID
    // unlink or replace the database and `secrets/` even though the files
    // inside are root-owned. Re-owning the leaves alone would look like it
    // closed the hazard while leaving the door open.
    println!("Re-owning retained data to root...");
    match ops.chown_root_recursive(data_dir) {
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

async fn remove_user(ops: &dyn UninstallOps, report: &mut Report) {
    println!("Removing the {SERVICE_USER} system user...");
    let result = ops.delete_user(SERVICE_USER).await;
    report.record(&format!("user {SERVICE_USER}"), result);
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
