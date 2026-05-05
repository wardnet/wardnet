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
    &[Migration {
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
    }]
}
