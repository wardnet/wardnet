//! Production [`SystemPowerOps`] backed by `systemctl`.
//!
//! Both methods invoke `systemctl ... --no-block` so logind queues
//! the action and returns immediately; the actual shutdown sequence
//! runs asynchronously and SIGTERMs wardnetd as part of
//! `shutdown.target`.

use std::sync::Arc;

use async_trait::async_trait;

use wardnetd_services::command::CommandExecutor;
use wardnetd_services::error::AppError;
use wardnetd_services::system::SystemPowerOps;

/// Shells out to `systemctl reboot|poweroff --no-block`. Authorisation
/// to do so as the unprivileged `wardnet` user is granted by the
/// polkit rule installed by the `0001_polkit_power_rule` post-upgrade
/// migration.
#[derive(Debug)]
pub struct SystemctlPowerOps {
    executor: Arc<dyn CommandExecutor>,
}

impl SystemctlPowerOps {
    #[must_use]
    pub fn new(executor: Arc<dyn CommandExecutor>) -> Self {
        Self { executor }
    }

    async fn run_systemctl(&self, verb: &str) -> Result<(), AppError> {
        let output = self
            .executor
            .run("systemctl", &[verb, "--no-block"])
            .await
            .map_err(|e| {
                AppError::Internal(anyhow::anyhow!("failed to spawn systemctl {verb}: {e}"))
            })?;

        if output.success {
            return Ok(());
        }

        // logind refuses with a non-zero exit when the polkit rule is
        // missing or denies the action. Surface the stderr verbatim so
        // the UI can show a useful "did_not_fire" diagnostic.
        Err(AppError::Internal(anyhow::anyhow!(
            "systemctl {verb} --no-block failed: {}",
            output.stderr.trim()
        )))
    }
}

#[async_trait]
impl SystemPowerOps for SystemctlPowerOps {
    async fn reboot(&self) -> Result<(), AppError> {
        self.run_systemctl("reboot").await
    }

    async fn poweroff(&self) -> Result<(), AppError> {
        self.run_systemctl("poweroff").await
    }
}
