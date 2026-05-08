use async_trait::async_trait;

/// Result of probing the public side of a tunnel.
#[derive(Debug, Clone)]
pub struct ExitInfo {
    /// Public IP observed at the tunnel exit.
    pub ip: String,
    /// ISO-3166 alpha-2 country code reported by the probe service.
    pub country_code: String,
}

/// Failure modes for [`TunnelExitProbe::probe`].
#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    /// The probe could not establish a connection through the tunnel
    /// interface (DNS, TCP, TLS, or routing failure).
    #[error("probe could not connect: {0}")]
    Connect(String),

    /// The probe completed but the response could not be parsed into an
    /// [`ExitInfo`].
    #[error("probe response could not be parsed: {0}")]
    Parse(String),

    /// The probe exceeded its allotted time budget.
    #[error("probe timed out after {0} ms")]
    Timeout(u64),

    /// The probe is not implemented for this build/platform.
    #[error("probe unsupported on this platform: {0}")]
    Unsupported(String),
}

/// Sends a single HTTP probe through a specific network interface to learn
/// the public IP and country at the tunnel's exit.
///
/// Implementations must bind the outbound socket to the interface so the
/// probe traverses the tunnel and not the default route.
#[async_trait]
pub trait TunnelExitProbe: Send + Sync {
    async fn probe(&self, interface: &str) -> Result<ExitInfo, ProbeError>;
}
