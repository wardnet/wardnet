//! Real `ThroughputTester` impl: times a fixed HTTP download to derive
//! throughput, optionally bound to a tunnel interface.
//!
//! Linux uses `SO_BINDTODEVICE` (via `reqwest::ClientBuilder::interface`) to
//! force the outbound socket onto the tunnel interface for the tunnel leg;
//! the direct leg builds an unbound client so it egresses the default (WAN)
//! route. Other platforms return [`ThroughputError::Unsupported`] — the
//! production daemon only runs on Linux; macOS/Windows builds reach the mock
//! backend instead.

use std::time::{Duration, Instant};

use async_trait::async_trait;

use wardnetd_services::tunnel::throughput_tester::{
    ThroughputError, ThroughputMeasurement, ThroughputTester,
};

/// 30 s download budget — the acceptance target is "completes in under 30 s
/// on a 100 Mbps tunnel", and a 10 MB payload at 100 Mbps takes under a
/// second, so this only trips on a stalled or very slow link.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30);

/// Downloads `download_url` and measures throughput. The URL is fixed at
/// construction from `tunnel.speed_test_url` (default: Cloudflare's `__down`
/// endpoint requesting 10 MB).
pub struct HttpThroughputTester {
    download_url: String,
}

impl HttpThroughputTester {
    #[must_use]
    pub fn new(download_url: String) -> Self {
        Self { download_url }
    }
}

#[async_trait]
impl ThroughputTester for HttpThroughputTester {
    #[cfg(target_os = "linux")]
    async fn download(
        &self,
        interface: Option<&str>,
    ) -> Result<ThroughputMeasurement, ThroughputError> {
        let mut builder = reqwest::Client::builder().timeout(DOWNLOAD_TIMEOUT);
        // `Some` binds the socket to the tunnel (tunnel leg); `None` leaves it
        // unbound so it egresses the default WAN route (direct baseline).
        if let Some(iface) = interface {
            builder = builder.interface(iface);
        }
        let client = builder
            .build()
            .map_err(|e| ThroughputError::Download(format!("client build failed: {e}")))?;

        let started = Instant::now();
        let resp = client.get(&self.download_url).send().await.map_err(|e| {
            if e.is_timeout() {
                ThroughputError::Timeout(
                    u64::try_from(DOWNLOAD_TIMEOUT.as_millis()).unwrap_or(u64::MAX),
                )
            } else {
                ThroughputError::Download(e.to_string())
            }
        })?;

        if !resp.status().is_success() {
            return Err(ThroughputError::Download(format!(
                "download returned HTTP {}",
                resp.status()
            )));
        }

        let body = resp.bytes().await.map_err(|e| {
            if e.is_timeout() {
                ThroughputError::Timeout(
                    u64::try_from(DOWNLOAD_TIMEOUT.as_millis()).unwrap_or(u64::MAX),
                )
            } else {
                ThroughputError::Download(format!("read body failed: {e}"))
            }
        })?;

        let elapsed = started.elapsed().as_secs_f64();
        let bytes = body.len();
        if bytes == 0 {
            return Err(ThroughputError::Download(
                "download returned an empty body".to_owned(),
            ));
        }
        if elapsed <= 0.0 {
            return Err(ThroughputError::Download(
                "download completed in zero time".to_owned(),
            ));
        }

        // Megabits per second: bytes → bits (×8) → megabits (÷1e6) ÷ seconds.
        #[allow(clippy::cast_precision_loss)]
        let mbps = (bytes as f64 * 8.0) / 1_000_000.0 / elapsed;
        Ok(ThroughputMeasurement { mbps })
    }

    #[cfg(not(target_os = "linux"))]
    async fn download(
        &self,
        _interface: Option<&str>,
    ) -> Result<ThroughputMeasurement, ThroughputError> {
        Err(ThroughputError::Unsupported(
            "SO_BINDTODEVICE is Linux-only; speed test requires Linux".to_owned(),
        ))
    }
}
