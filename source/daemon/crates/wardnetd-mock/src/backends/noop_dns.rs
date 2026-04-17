//! No-op [`DnsServer`] implementation for the mock server.

use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use wardnet_common::dns::DnsConfig;
use wardnetd_services::dns::server::DnsServer;

/// A DNS server that never binds a UDP socket or resolves a query.
///
/// Tracks its logical running state and a synthetic zero-cache so the API
/// surface behaves predictably, without any real DNS resolution happening.
#[derive(Debug, Default)]
pub struct NoopDnsServer {
    running: AtomicBool,
}

#[async_trait]
impl DnsServer for NoopDnsServer {
    async fn start(&self) -> anyhow::Result<()> {
        self.running.store(true, Ordering::SeqCst);
        tracing::debug!("mock DNS server start");
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        self.running.store(false, Ordering::SeqCst);
        tracing::debug!("mock DNS server stop");
        Ok(())
    }

    fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    async fn flush_cache(&self) -> u64 {
        tracing::debug!("mock DNS flush_cache (returning 0)");
        0
    }

    async fn cache_size(&self) -> u64 {
        0
    }

    async fn cache_hit_rate(&self) -> f64 {
        0.0
    }

    async fn update_config(&self, _config: DnsConfig) {
        tracing::debug!("mock DNS update_config");
    }
}
