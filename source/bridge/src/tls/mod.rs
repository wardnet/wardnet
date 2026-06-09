//! Bridge-owned TLS termination glue.
//!
//! The bridge terminates TLS for its **own** FQDN on `:8443` (everything else is
//! passed through to tenant tunnels — see [`crate::sni`]). This module owns the
//! hot-swappable serving certificate ([`CertResolver`]), the boot-time
//! placeholder, and the background ACME renewal loop ([`runner`]).

pub mod runner;
pub mod serving;

pub use runner::TlsRenewalRunner;
pub use serving::{CertResolver, PLACEHOLDER_VERSION};

/// Install `aws-lc-rs` as the process-default rustls crypto provider.
///
/// rustls 0.23 cannot pick a provider automatically when more than one is linked,
/// so we install it explicitly once at startup. Idempotent: a second call is a
/// no-op (the `Result` is ignored).
pub fn install_crypto_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}
