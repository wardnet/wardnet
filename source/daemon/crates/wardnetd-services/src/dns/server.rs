use std::net::SocketAddr;

use async_trait::async_trait;
use wardnet_common::dns::{DnsConfig, UpstreamLatency};

use crate::dns::authoritative::AuthoritativeView;

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

    /// Replace the in-memory authoritative view (zones + custom records +
    /// forwarding rules). Called by the DNS runner whenever a
    /// `DnsLocalChanged` event arrives.
    async fn update_authoritative_view(&self, view: AuthoritativeView);

    /// Evict every cache entry at or below `domain` — the name and all of
    /// its subdomains — so the next query for any of them is re-resolved
    /// rather than served from a stale cached answer. Local DNS is applied
    /// per subtree, so eviction has to be too.
    async fn invalidate_subtree(&self, domain: &str);

    /// Latest rolling-average latency per configured upstream, produced by the
    /// background prober. One entry per current upstream address (empty until
    /// the first probe). Defaults to empty for implementations without a
    /// prober (mocks, no-op backend).
    fn upstream_latencies(&self) -> Vec<UpstreamLatency> {
        Vec::new()
    }
}
