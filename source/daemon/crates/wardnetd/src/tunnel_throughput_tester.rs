//! Real `ThroughputTester` impl: runs several concurrent HTTP downloads,
//! discards an initial warm-up window, and sums bytes read over a fixed
//! measure window afterward.
//!
//! A single-shot, single-connection download is prone to two skews: its
//! timing includes DNS/TCP/TLS handshake and TCP slow-start (understating
//! throughput, worse at higher RTT — e.g. through a tunnel), and a single
//! TCP flow is capped by its bandwidth-delay product (also worse at higher
//! RTT). Running several streams concurrently and discarding a warm-up
//! prefix avoids both.
//!
//! Linux uses `SO_BINDTODEVICE` (via `reqwest::ClientBuilder::interface`) to
//! force each stream's outbound socket onto the tunnel interface for the
//! tunnel leg; the direct leg builds an unbound client so it egresses the
//! default (WAN) route. Other platforms return [`ThroughputError::Unsupported`]
//! — the production daemon only runs on Linux; macOS/Windows builds reach the
//! mock backend instead.

use std::time::{Duration, Instant};

use async_trait::async_trait;

use wardnetd_services::tunnel::throughput_tester::{
    ThroughputError, ThroughputMeasurement, ThroughputTester,
};

/// Extra grace added on top of `warmup + measure` to form each stream's
/// safety-net timeout — covers connect/TLS setup and scheduling jitter
/// without masking a legitimately slow but working link. Kept separate
/// from `warmup`/`measure` (which are operator-configurable) so raising
/// those config values can never make the safety net fire before the
/// stream's own deadline logic gets to run.
const CONNECT_GRACE: Duration = Duration::from_secs(15);

/// Outcome of a single stream's download attempt.
pub(crate) struct StreamOutcome {
    /// Bytes read while inside the `[warmup, warmup + measure)` window.
    pub(crate) bytes_in_window: u64,
    /// Whether this stream failed (timed out, connection error, non-success
    /// status). Failed streams are excluded from the aggregate — see
    /// [`aggregate_throughput`].
    pub(crate) failed: bool,
}

/// Downloads `download_url` concurrently across several streams and measures
/// sustained throughput. The URL, stream count, warm-up, and measure window
/// are fixed at construction from the `tunnel.speed_test_*` config.
pub struct HttpThroughputTester {
    download_url: String,
    parallel_streams: u32,
    warmup: Duration,
    measure: Duration,
}

impl HttpThroughputTester {
    #[must_use]
    pub fn new(
        download_url: String,
        parallel_streams: u32,
        warmup: Duration,
        measure: Duration,
    ) -> Self {
        Self {
            download_url,
            parallel_streams,
            warmup,
            measure,
        }
    }
}

#[async_trait]
impl ThroughputTester for HttpThroughputTester {
    #[cfg(target_os = "linux")]
    async fn download(
        &self,
        interface: Option<&str>,
    ) -> Result<ThroughputMeasurement, ThroughputError> {
        if self.parallel_streams == 0 {
            return Err(ThroughputError::Download(
                "speed_test_parallel_streams must be at least 1".to_owned(),
            ));
        }

        // One client per leg, cloned into each stream: `reqwest::Client` is
        // `Arc`-backed internally, so cloning is cheap and doesn't repeat
        // DNS-resolver/TLS-config setup per stream — each clone still opens
        // its own independent connection per concurrent request.
        let client = crate::reqwest_client::interface_bound_builder(interface)
            .build()
            .map_err(|e| ThroughputError::Download(format!("client build failed: {e}")))?;
        let stream_timeout = self.warmup + self.measure + CONNECT_GRACE;

        let start = Instant::now();
        let streams =
            (0..self.parallel_streams).map(|_| self.run_stream(&client, start, stream_timeout));
        let outcomes = futures::future::join_all(streams).await;
        aggregate_throughput(&outcomes, self.measure)
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

#[cfg(target_os = "linux")]
impl HttpThroughputTester {
    /// Runs a single stream: reads the body via `client` and tallies bytes
    /// read during `[warmup, warmup + measure)` measured from `start`. Stops
    /// reading once the measure window ends, regardless of how much of the
    /// payload remains. Wrapped in `stream_timeout` so a stalled connection
    /// can't hang the whole speed test.
    async fn run_stream(
        &self,
        client: &reqwest::Client,
        start: Instant,
        stream_timeout: Duration,
    ) -> StreamOutcome {
        use futures::StreamExt;

        let attempt = async {
            let resp = client
                .get(&self.download_url)
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !resp.status().is_success() {
                return Err(format!("download returned HTTP {}", resp.status()));
            }

            let deadline = start + self.warmup + self.measure;
            let mut body = resp.bytes_stream();
            let mut bytes_in_window: u64 = 0;
            loop {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    break;
                }
                tokio::select! {
                    chunk = body.next() => {
                        match chunk {
                            Some(Ok(bytes)) => {
                                let now = Instant::now();
                                if now >= start + self.warmup && now < deadline {
                                    bytes_in_window += bytes.len() as u64;
                                }
                            }
                            Some(Err(e)) => return Err(e.to_string()),
                            None => break,
                        }
                    }
                    () = tokio::time::sleep(remaining) => break,
                }
            }
            Ok(bytes_in_window)
        };

        match tokio::time::timeout(stream_timeout, attempt).await {
            Ok(Ok(bytes_in_window)) => StreamOutcome {
                bytes_in_window,
                failed: false,
            },
            Ok(Err(_)) | Err(_) => StreamOutcome {
                bytes_in_window: 0,
                failed: true,
            },
        }
    }
}

/// Aggregates per-stream outcomes into a single throughput measurement.
/// Sums bytes read during the measure window by streams that didn't fail.
/// Errors if every stream failed, if `measure` is zero (nothing to divide
/// by), or if every non-failed stream delivered zero bytes in the window
/// (indistinguishable from a stalled test, not a real measurement) — the
/// last two mirror the "empty body" / "zero elapsed time" guards the
/// previous single-shot implementation had. Pure and deterministic — no I/O
/// or timing — so it's unit-tested directly rather than through real HTTP
/// downloads.
pub(crate) fn aggregate_throughput(
    outcomes: &[StreamOutcome],
    measure: Duration,
) -> Result<ThroughputMeasurement, ThroughputError> {
    if measure.is_zero() {
        return Err(ThroughputError::Download(
            "speed_test_measure_ms must be greater than zero".to_owned(),
        ));
    }
    if outcomes.iter().all(|o| o.failed) {
        return Err(ThroughputError::Download(
            "all parallel download streams failed".to_owned(),
        ));
    }

    let total_bytes: u64 = outcomes
        .iter()
        .filter(|o| !o.failed)
        .map(|o| o.bytes_in_window)
        .sum();
    if total_bytes == 0 {
        return Err(ThroughputError::Download(
            "no bytes were read during the measure window".to_owned(),
        ));
    }

    let secs = measure.as_secs_f64();
    #[allow(clippy::cast_precision_loss)]
    let mbps = (total_bytes as f64 * 8.0) / 1_000_000.0 / secs;
    Ok(ThroughputMeasurement { mbps })
}
