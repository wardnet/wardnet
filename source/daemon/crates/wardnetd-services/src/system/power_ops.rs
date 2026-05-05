//! Host-power operations: reboot and poweroff.
//!
//! Behind a trait so the production binary can shell out to
//! `systemctl reboot --no-block` / `systemctl poweroff --no-block`
//! while the mock daemon and unit tests can plug in a no-op stub
//! that does not actually reboot the developer's laptop.

use async_trait::async_trait;

use crate::error::AppError;

/// Reboot or power off the host. Implementations are wired onto
/// [`crate::Backends`] so the service layer never reaches for a
/// concrete `systemctl` shell-out.
#[async_trait]
pub trait SystemPowerOps: Send + Sync {
    /// Reboot the host. Implementations must return promptly —
    /// `systemctl reboot --no-block` returns once logind has accepted
    /// the request, before the actual shutdown sequence begins.
    async fn reboot(&self) -> Result<(), AppError>;

    /// Power off the host. Same semantics as [`Self::reboot`].
    async fn poweroff(&self) -> Result<(), AppError>;
}
