//! The list of everything `install.sh` created, and what each tier does with
//! it.
//!
//! Kept pure — building the inventory touches no filesystem — so the two tiers
//! and the container/bare-metal split can be unit-tested, and so `--dry-run`
//! prints exactly the list the real run will work from. Nothing here checks
//! whether a path exists; the executor tolerates absence, which is what makes
//! uninstall safe to re-run and safe on a partial install.

use std::fmt;

/// What kind of thing an inventory entry refers to. Drives both how it is
/// removed and how it is labelled in the printed plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A single file.
    File,
    /// A directory, removed recursively.
    Directory,
    /// A systemd unit: disabled, then its unit file removed.
    Unit,
    /// A system user account.
    User,
    /// Kernel state (nftables table, `WireGuard` interfaces, conntrack).
    Runtime,
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::File => "file",
            Self::Directory => "dir",
            Self::Unit => "unit",
            Self::User => "user",
            Self::Runtime => "runtime",
        };
        f.write_str(label)
    }
}

/// What the current tier does with an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Delete it.
    Remove,
    /// Leave it on disk. Retained data is re-owned by root, because the
    /// `wardnet` user is deleted in the same run.
    Keep,
}

/// One thing the uninstaller will touch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub kind: Kind,
    pub target: String,
    pub action: Action,
    /// Why this entry is interesting, shown in the printed plan. Only set
    /// where the reason is not obvious from the path.
    pub note: Option<String>,
}

impl Item {
    fn new(kind: Kind, target: &str, action: Action, note: Option<&str>) -> Self {
        Self {
            kind,
            target: target.to_owned(),
            action,
            note: note.map(ToOwned::to_owned),
        }
    }
}

/// The systemd units `install.sh` installs. Deliberately the same three, in
/// the same order — `wardnetd-dev.service` is a dev-only unit the installer
/// never places on a real host, so uninstall must not touch it.
pub const UNITS: [&str; 3] = [
    "wardnetd.service",
    "wardnetd-rollback.service",
    "wardnet-postupgrade.service",
];

/// The system user `install.sh` creates.
pub const SERVICE_USER: &str = "wardnet";

/// The data directory. Retained by default, destroyed by `--purge`.
pub const DATA_DIR: &str = "/var/lib/wardnet";

/// Paths under [`DATA_DIR`] that hold user data worth keeping: the `SQLite`
/// database (plus its WAL sidecars) and the secret store, which contains the
/// `WireGuard` private keys, the backup passphrase and the DDNS credentials.
pub const RETAINED_DATA: [&str; 4] = [
    "/var/lib/wardnet/wardnet.db",
    "/var/lib/wardnet/wardnet.db-wal",
    "/var/lib/wardnet/wardnet.db-shm",
    "/var/lib/wardnet/secrets",
];

/// Build the full list of things to touch for this run.
///
/// `container_mode` mirrors `install.sh --container-mode`: in a container the
/// installer never writes the sysctl drop-in, the dhcpcd drop-in or the
/// watchdog module-load file, because the container's networking owns
/// forwarding and `/proc/sys` is read-only. Listing them anyway would be
/// dishonest in `--dry-run` output.
#[must_use]
pub fn build_inventory(purge: bool, container_mode: bool) -> Vec<Item> {
    let mut items = Vec::new();
    // Runtime state first: it is the part of the removal a reader most needs
    // to see in a dry run, and the part no other tool would do for them.
    items.extend(runtime_items());
    items.extend(unit_items());
    items.extend(installed_file_items());
    items.extend(data_items(purge));
    if !container_mode {
        items.extend(host_dropin_items());
    }
    items.push(Item::new(
        Kind::User,
        SERVICE_USER,
        Action::Remove,
        Some("system user"),
    ));
    items
}

/// Kernel state the daemon created (issue #864).
fn runtime_items() -> Vec<Item> {
    vec![
        Item::new(
            Kind::Runtime,
            "nftables table `inet wardnet`",
            Action::Remove,
            Some("only our named table; never a full ruleset flush"),
        ),
        Item::new(
            Kind::Runtime,
            "wireguard interfaces `wg_ward*`",
            Action::Remove,
            Some("outbound tunnels and the inbound remote-access server"),
        ),
    ]
}

fn unit_items() -> Vec<Item> {
    UNITS
        .iter()
        .map(|unit| {
            Item::new(
                Kind::Unit,
                &format!("/etc/systemd/system/{unit}"),
                Action::Remove,
                None,
            )
        })
        .collect()
}

/// Binaries, config and logs — everything outside the data directory.
fn installed_file_items() -> Vec<Item> {
    vec![
        Item::new(Kind::File, "/usr/local/bin/wardnetd", Action::Remove, None),
        Item::new(
            Kind::File,
            "/usr/local/bin/wardnetd.old",
            Action::Remove,
            Some("previous binary kept for rollback"),
        ),
        Item::new(
            Kind::File,
            "/usr/local/sbin/wardnet-uninstall",
            Action::Remove,
            Some("this uninstaller's escape-hatch wrapper"),
        ),
        Item::new(
            Kind::Directory,
            "/usr/local/libexec/wardnet",
            Action::Remove,
            None,
        ),
        Item::new(
            Kind::Directory,
            "/etc/wardnet",
            Action::Remove,
            Some("configuration"),
        ),
        Item::new(Kind::Directory, "/var/log/wardnet", Action::Remove, None),
        Item::new(
            Kind::Directory,
            "/var/lib/wardnet-postupgrade",
            Action::Remove,
            Some("root-owned, outside the wardnet tree"),
        ),
    ]
}

/// The data directory, split by tier.
fn data_items(purge: bool) -> Vec<Item> {
    if purge {
        return vec![Item::new(
            Kind::Directory,
            DATA_DIR,
            Action::Remove,
            Some("DESTROYS the database, wireguard keys and secret store"),
        )];
    }

    // Scratch subdirectories go in both tiers; only genuine user data survives
    // the default run.
    let mut items = vec![
        Item::new(
            Kind::Directory,
            "/var/lib/wardnet/updates",
            Action::Remove,
            Some("downloaded release artefacts"),
        ),
        Item::new(
            Kind::Directory,
            "/var/lib/wardnet/postupgrade",
            Action::Remove,
            Some("staged post-upgrade migration binary"),
        ),
    ];
    items.extend(RETAINED_DATA.iter().copied().map(|path| {
        // `secrets` is a directory; the database and its WAL sidecars are
        // files. Labelling them all `file` would misdescribe the plan.
        let kind = if path.ends_with("/secrets") {
            Kind::Directory
        } else {
            Kind::File
        };
        Item::new(
            kind,
            path,
            Action::Keep,
            Some("re-owned to root; delete with --purge"),
        )
    }));
    items
}

/// Drop-ins the installer only writes on a bare-metal host.
fn host_dropin_items() -> Vec<Item> {
    vec![
        Item::new(
            Kind::File,
            "/etc/sysctl.d/99-wardnet.conf",
            Action::Remove,
            Some("stops net.ipv4.ip_forward persisting; live value survives until reboot"),
        ),
        Item::new(
            Kind::File,
            "/etc/dhcpcd.conf.d/wardnet.conf",
            Action::Remove,
            Some("static IP drop-in; the host reverts to DHCP at next boot"),
        ),
        Item::new(
            Kind::File,
            "/etc/modules-load.d/wardnet-watchdog.conf",
            Action::Remove,
            Some("hardware watchdog module load"),
        ),
    ]
}

/// Render the inventory as the plan printed before acting (and the whole
/// output of `--dry-run`).
#[must_use]
pub fn render_plan(items: &[Item]) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    for item in items {
        let verb = match item.action {
            Action::Remove => "remove",
            Action::Keep => "KEEP  ",
        };
        let _ = write!(out, "  {verb}  [{:<7}] {}", item.kind, item.target);
        if let Some(note) = &item.note {
            let _ = write!(out, "\n              ({note})");
        }
        out.push('\n');
    }
    out
}
