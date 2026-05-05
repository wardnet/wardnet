//! No-op [`SystemPowerOps`] implementation for the mock server.
//!
//! Calling `reboot()` or `poweroff()` here logs a `WARN` and returns
//! `Ok(())`. The dev daemon is **not** terminated — that would force
//! the operator to relaunch the mock every time they exercise the
//! Safe Reboot / Safe Shutdown UI.

use async_trait::async_trait;
use wardnetd_services::error::AppError;
use wardnetd_services::system::SystemPowerOps;

#[derive(Debug, Default, Clone)]
pub struct NoopSystemPowerOps;

#[async_trait]
impl SystemPowerOps for NoopSystemPowerOps {
    async fn reboot(&self) -> Result<(), AppError> {
        tracing::warn!("mock power_ops reboot() called (no-op; dev daemon stays up)");
        Ok(())
    }

    async fn poweroff(&self) -> Result<(), AppError> {
        tracing::warn!("mock power_ops poweroff() called (no-op; dev daemon stays up)");
        Ok(())
    }
}
