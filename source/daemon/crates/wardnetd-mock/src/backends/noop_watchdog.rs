//! No-op [`WatchdogOps`] implementation for the mock server.
//!
//! Reports the device as unavailable and turns every pet/disarm into a logged
//! no-op, so the dev daemon never opens or arms a real `/dev/watchdog` (which
//! could reboot the developer's machine). The hardware watchdog runner then
//! parks idle — exactly as on a board without a watchdog.

use async_trait::async_trait;
use wardnetd_services::system::WatchdogOps;

/// No-op [`WatchdogOps`] backend for the mock daemon. Reports the device as
/// unavailable; every `pet`/`disarm` is a logged no-op so the dev daemon never
/// arms a real `/dev/watchdog`.
#[derive(Debug, Default, Clone)]
pub struct NoopWatchdog;

#[async_trait]
impl WatchdogOps for NoopWatchdog {
    async fn pet(&self) {
        tracing::debug!("mock watchdog pet() called (no-op)");
    }

    async fn disarm(&self) {
        tracing::debug!("mock watchdog disarm() called (no-op)");
    }

    fn is_available(&self) -> bool {
        false
    }
}
