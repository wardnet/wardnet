//! Real `TunnelExitProbe` impl: HTTP probe through a specific interface.
//!
//! Linux uses `SO_BINDTODEVICE` (via `reqwest::ClientBuilder::interface`) to
//! force the outbound socket onto the tunnel interface, so the probe
//! traverses the tunnel rather than the default route. Other platforms
//! return [`ProbeError::Unsupported`] — the production daemon only runs on
//! Linux; macOS/Windows builds reach the mock backend instead.

use std::time::Duration;

use async_trait::async_trait;

use wardnetd_services::tunnel::exit_probe::{ExitInfo, ProbeError, TunnelExitProbe};

/// 1.5 s probe budget — chosen so the daemon endpoint stays under its 5 s
/// overall timeout once the 3.5 s handshake budget is spent.
const PROBE_TIMEOUT: Duration = Duration::from_millis(1500);

/// HTTP probe for tunnel exit IP / country, configurable via
/// `tunnel.test_probe_url`.
///
/// The default endpoint is Cloudflare's `https://1.1.1.1/cdn-cgi/trace`,
/// which returns a `key=value` document including `ip=` and `loc=` (an
/// ISO-3166 alpha-2 code). Operators that want a different probe must
/// pick one with the same response shape.
pub struct ReqwestTunnelExitProbe {
    probe_url: String,
}

impl ReqwestTunnelExitProbe {
    #[must_use]
    pub fn new(probe_url: String) -> Self {
        Self { probe_url }
    }
}

#[async_trait]
impl TunnelExitProbe for ReqwestTunnelExitProbe {
    #[cfg(target_os = "linux")]
    async fn probe(&self, interface: &str) -> Result<ExitInfo, ProbeError> {
        let client = crate::reqwest_client::interface_bound_builder(Some(interface))
            .timeout(PROBE_TIMEOUT)
            .build()
            .map_err(|e| ProbeError::Connect(format!("client build failed: {e}")))?;

        let resp = client.get(&self.probe_url).send().await.map_err(|e| {
            if e.is_timeout() {
                ProbeError::Timeout(u64::try_from(PROBE_TIMEOUT.as_millis()).unwrap_or(u64::MAX))
            } else {
                ProbeError::Connect(e.to_string())
            }
        })?;

        if !resp.status().is_success() {
            return Err(ProbeError::Connect(format!(
                "probe returned HTTP {}",
                resp.status()
            )));
        }

        let body = resp
            .text()
            .await
            .map_err(|e| ProbeError::Connect(format!("read body failed: {e}")))?;

        parse_trace_body(&body)
    }

    #[cfg(not(target_os = "linux"))]
    async fn probe(&self, _interface: &str) -> Result<ExitInfo, ProbeError> {
        Err(ProbeError::Unsupported(
            "SO_BINDTODEVICE is Linux-only; tunnel test endpoint requires Linux".to_owned(),
        ))
    }
}

/// Parse Cloudflare's `cdn-cgi/trace` response into [`ExitInfo`].
///
/// The body is a sequence of `key=value` lines; `ip=` and `loc=` are
/// the two we need. Missing fields fail with [`ProbeError::Parse`] so
/// the user sees a precise error rather than a generic "probe failed".
pub(crate) fn parse_trace_body(body: &str) -> Result<ExitInfo, ProbeError> {
    let mut ip: Option<String> = None;
    let mut country: Option<String> = None;

    for line in body.lines() {
        if let Some(rest) = line.strip_prefix("ip=") {
            ip = Some(rest.trim().to_owned());
        } else if let Some(rest) = line.strip_prefix("loc=") {
            country = Some(rest.trim().to_owned());
        }
    }

    let ip = ip.ok_or_else(|| ProbeError::Parse("response missing 'ip=' field".to_owned()))?;
    let country_code =
        country.ok_or_else(|| ProbeError::Parse("response missing 'loc=' field".to_owned()))?;

    if ip.is_empty() {
        return Err(ProbeError::Parse("'ip=' was empty".to_owned()));
    }
    if country_code.is_empty() {
        return Err(ProbeError::Parse("'loc=' was empty".to_owned()));
    }

    Ok(ExitInfo { ip, country_code })
}
