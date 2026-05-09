//! No-op [`GarpOps`] for the mock server.
//!
//! The mock has no LAN interface to broadcast on — this just logs at
//! `debug` level so dev launches don't get spammed but the call path
//! still shows up in `RUST_LOG=debug` output. The real impl lives in
//! `wardnetd::garp_pnet::PnetGarpOps`.

use async_trait::async_trait;
use wardnetd_services::garp::GarpOps;

#[derive(Debug, Default, Clone)]
pub struct NoopGarpOps;

#[async_trait]
impl GarpOps for NoopGarpOps {
    async fn broadcast_farewell(&self) -> anyhow::Result<()> {
        tracing::debug!("NoopGarpOps::broadcast_farewell called (mock)");
        Ok(())
    }

    async fn broadcast_claim(&self) -> anyhow::Result<()> {
        tracing::debug!("NoopGarpOps::broadcast_claim called (mock)");
        Ok(())
    }
}
