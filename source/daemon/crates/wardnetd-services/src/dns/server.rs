use std::net::SocketAddr;

use async_trait::async_trait;
use wardnet_common::dns::DnsConfig;

// ---------------------------------------------------------------------------
// DnsSocket trait
// ---------------------------------------------------------------------------

/// Abstraction over UDP socket operations for DNS packet I/O.
#[async_trait]
pub trait DnsSocket: Send + Sync {
    async fn recv_from(&self, buf: &mut [u8]) -> std::io::Result<(usize, SocketAddr)>;
    async fn send_to(&self, buf: &[u8], target: SocketAddr) -> std::io::Result<usize>;
}

// ---------------------------------------------------------------------------
// DnsServer trait
// ---------------------------------------------------------------------------

/// Abstraction over the DNS server.
#[async_trait]
pub trait DnsServer: Send + Sync {
    /// Start listening for DNS queries on UDP port 53.
    async fn start(&self) -> anyhow::Result<()>;

    /// Stop the running server.
    async fn stop(&self) -> anyhow::Result<()>;

    /// Whether the server is currently running.
    fn is_running(&self) -> bool;

    /// Flush the DNS cache. Returns number of entries cleared.
    async fn flush_cache(&self) -> u64;

    /// Current cache size.
    async fn cache_size(&self) -> u64;

    /// Cache hit rate (0.0 to 1.0).
    async fn cache_hit_rate(&self) -> f64;

    /// Update the DNS configuration at runtime.
    async fn update_config(&self, config: DnsConfig);
}
