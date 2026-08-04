//! Compile-time migration list.
//!
//! Migrations are linearly ordered (sqlx-migrate-style) — there are
//! no branching dependencies or `down` functions. Each migration is
//! a function that brings the system to a desired state and is safe
//! to re-run if it has already been applied. Severity controls what
//! happens on failure:
//!   - `Required` — failure exits the runner nonzero, blocking
//!     wardnetd from starting (`RequiredBy=wardnetd.service`).
//!   - `Optional` — failure is recorded but does not gate startup.
//!
//! The framework lands with an **empty list**. Future PRs (#215, #213,
//! #222) push entries here.

/// What happens when a migration's `up` function returns `Err`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Failure exits the runner nonzero. `wardnetd.service` will not
    /// start until the operator resolves the failure.
    Required,
    /// Failure is recorded but the runner continues and exits 0;
    /// wardnetd starts. Use for cosmetic / non-load-bearing steps.
    Optional,
}

/// One migration. Identified by a stable string id (sortable, unique
/// within `migrations()`) plus a severity and an `up` function. Once
/// a migration has been merged it MUST NEVER change its id, severity,
/// or behaviour — append new migrations instead.
pub struct Migration {
    pub id: &'static str,
    pub severity: Severity,
    pub up: fn() -> anyhow::Result<()>,
}

/// The migration list applied at every boot. Order matters: each
/// migration may rely on the system state established by earlier
/// applied migrations.
#[must_use]
pub fn migrations() -> &'static [Migration] {
    &[
        Migration {
            // Authorises the daemon (running as the unprivileged
            // `wardnet` user) to call logind's reboot / power-off
            // actions, which is what `systemctl reboot|poweroff
            // --no-block` invokes under the hood. See #215.
            //
            // Severity::Optional: the daemon still starts without it.
            // Only Safe Reboot / Safe Shutdown stop working — the rest
            // of the system keeps functioning, so blocking startup
            // would be the wrong trade-off (e.g. on a non-systemd
            // container or a host where polkit is uninstalled).
            id: "0001_polkit_power_rule",
            severity: Severity::Optional,
            up: crate::up::write_polkit_power_rule,
        },
        Migration {
            // Installs the udev rule granting the `wardnet` group
            // access to /dev/watchdog so the daemon can arm the ungated
            // hardware-reboot backstop (issue #214). Without it the open
            // fails EACCES and the backstop silently stays off.
            //
            // Severity::Optional: the soft (sd_notify) watchdog still
            // works without it, and hosts with no watchdog device (or
            // no udev) must still boot the daemon.
            id: "0002_watchdog_udev_rule",
            severity: Severity::Optional,
            up: crate::up::write_watchdog_udev_rule,
        },
        Migration {
            // Refreshes /etc/systemd/system/wardnetd.service from the
            // unit shipped in this release. The auto-updater carries only
            // binaries, so unit-file fixes (e.g. the StartLimit* crash-loop
            // budget that systemd only honours under [Unit]) reach existing
            // installs through this step rather than an install.sh re-run.
            //
            // Run-once, like every migration: this ships the unit as it
            // stood when the migration was added. A later unit change that
            // must reach the fleet needs its own new migration id (append,
            // never edit this one — operators have state referencing it).
            //
            // Severity::Optional: if the rewrite fails the daemon still
            // starts from the on-disk unit; only the newer unit's
            // improvements are missed, which must not gate startup.
            id: "0003_wardnetd_unit_refresh",
            severity: Severity::Optional,
            up: crate::up::refresh_wardnetd_unit,
        },
        Migration {
            // Re-runs the unit refresh for the `ReadWritePaths=/etc/wardnet`
            // grant. `ProtectSystem=strict` mounts /etc read-only, so
            // restoring a backup — which renames /etc/wardnet/wardnet.toml
            // aside and writes the bundle's copy in its place — failed with
            // EROFS on every install that predates this.
            //
            // A new id rather than an edit to 0003: the runner records
            // migrations applied and skips them forever, so a box that
            // already ran 0003 would never pick the change up. Fresh
            // installs run both and the second is a content no-op, because
            // install.sh has already laid down this same unit.
            //
            // Severity::Optional, matching 0003: a failed rewrite leaves the
            // daemon starting from the on-disk unit. Only backup restore
            // stays broken, which must not gate startup.
            id: "0004_wardnetd_unit_config_write",
            severity: Severity::Optional,
            up: crate::up::refresh_wardnetd_unit,
        },
    ]
}
