//! No-op [`CertActivator`] for the mock server.
//!
//! The mock terminates plain HTTP only — there is no `:443` `RustlsConfig` to
//! hot-swap. This logs at `debug` level so the call path is visible under
//! `RUST_LOG=debug` but performs no cert reload. The real impl lives in
//! `wardnetd::tls_server::ServingControl`.

use async_trait::async_trait;
use wardnetd_services::CertActivator;

#[derive(Debug, Default, Clone)]
pub struct NoopCertActivator;

#[async_trait]
impl CertActivator for NoopCertActivator {
    async fn activate(
        &self,
        _chain_pem: Vec<u8>,
        _key_pem: Vec<u8>,
        _fqdn: String,
    ) -> anyhow::Result<()> {
        tracing::debug!("NoopCertActivator::activate called (mock)");
        Ok(())
    }

    async fn deactivate(&self) -> anyhow::Result<()> {
        tracing::debug!("NoopCertActivator::deactivate called (mock)");
        Ok(())
    }
}
