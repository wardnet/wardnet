use std::net::SocketAddr;

use async_trait::async_trait;

use crate::error::AppError;

// ---------------------------------------------------------------------------
// DhcpSocket trait
// ---------------------------------------------------------------------------

/// Abstraction over UDP socket operations for DHCP packet I/O.
///
/// Allows injecting a mock socket in tests instead of binding a real
/// UDP port.
#[async_trait]
pub trait DhcpSocket: Send + Sync {
    /// Receive a DHCP packet, returning the number of bytes read and
    /// the source address.
    async fn recv_from(&self, buf: &mut [u8]) -> std::io::Result<(usize, SocketAddr)>;

    /// Send a DHCP response to the given destination.
    async fn send_to(&self, buf: &[u8], target: SocketAddr) -> std::io::Result<usize>;
}

// ---------------------------------------------------------------------------
// DhcpServer trait
// ---------------------------------------------------------------------------

/// Abstraction over the raw DHCP packet handling server.
///
/// Allows testing with a noop or mock implementation.
#[async_trait]
pub trait DhcpServer: Send + Sync {
    /// Start listening for DHCP packets on UDP port 67.
    async fn start(&self) -> Result<(), AppError>;

    /// Stop the running server.
    async fn stop(&self) -> Result<(), AppError>;

    /// Whether the server is currently running.
    fn is_running(&self) -> bool;
}
