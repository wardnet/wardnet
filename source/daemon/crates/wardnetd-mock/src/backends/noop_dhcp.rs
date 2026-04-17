//! No-op [`DhcpServer`] implementation for the mock server.

use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use wardnetd_services::dhcp::server::DhcpServer;
use wardnetd_services::error::AppError;

/// A DHCP server that never binds a UDP socket or processes packets.
///
/// Tracks its logical running state so API endpoints that toggle the server
/// behave consistently in the UI, but performs no real DHCP exchange.
#[derive(Debug, Default)]
pub struct NoopDhcpServer {
    running: AtomicBool,
}

#[async_trait]
impl DhcpServer for NoopDhcpServer {
    async fn start(&self) -> Result<(), AppError> {
        self.running.store(true, Ordering::SeqCst);
        tracing::debug!("mock DHCP server start");
        Ok(())
    }

    async fn stop(&self) -> Result<(), AppError> {
        self.running.store(false, Ordering::SeqCst);
        tracing::debug!("mock DHCP server stop");
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}
