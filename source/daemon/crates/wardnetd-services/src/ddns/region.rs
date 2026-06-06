//! The built-in **region catalog** and latency-based selection.
//!
//! Each wardnet bridge is region-specific, so the daemon must already know a
//! region's address to reach it — the bridge cannot supply the list. The
//! catalog therefore ships in the daemon: a short **region slug** mapped to a
//! **bridge endpoint** URL. At registration the daemon probes every known
//! region's `GET /v1/health` and registers against the lowest-latency one.
//!
//! Today only `use1` exists, but the probe-and-pick mechanism is built in from
//! the start so adding a region is a one-line catalog change.

use std::time::{Duration, Instant};

/// One entry in the built-in region catalog.
#[derive(Debug, Clone, Copy)]
pub struct RegionEndpoint {
    /// Short region slug, e.g. `use1`. Selects which bridge endpoint to use; it
    /// is **not** the FQDN region label (the bridge owns that).
    pub slug: &'static str,
    /// Bridge endpoint base URL for this region.
    pub base_url: &'static str,
}

/// The built-in catalog. Extend this to add regions.
pub const REGION_CATALOG: &[RegionEndpoint] = &[RegionEndpoint {
    slug: "use1",
    base_url: "https://bridge.prod.use1.wardnet.network",
}];

/// A region chosen by [`select_region`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedRegion {
    pub slug: String,
    pub base_url: String,
}

/// Probe every catalog region and return the lowest-latency reachable one.
///
/// Returns an error if no region's bridge answers a healthy `/v1/health`.
pub async fn select_region(client: &reqwest::Client) -> anyhow::Result<SelectedRegion> {
    let entries: Vec<(String, String)> = REGION_CATALOG
        .iter()
        .map(|e| (e.slug.to_owned(), e.base_url.to_owned()))
        .collect();
    select_best(client, &entries).await
}

/// Probe-and-pick over an explicit `(slug, base_url)` list.
///
/// Probes run **concurrently** (one task per region) so the wall time is the
/// slowest single probe, not their sum — and each region's measured RTT
/// reflects its own latency rather than being inflated by earlier probes.
/// Factored out so tests can drive it against wiremock bridges with artificial
/// `/v1/health` delays; [`select_region`] passes [`REGION_CATALOG`].
pub(crate) async fn select_best(
    client: &reqwest::Client,
    entries: &[(String, String)],
) -> anyhow::Result<SelectedRegion> {
    let mut probes = tokio::task::JoinSet::new();
    for (slug, base_url) in entries {
        let client = client.clone();
        let slug = slug.clone();
        let base_url = base_url.clone();
        probes.spawn(async move {
            let start = Instant::now();
            let healthy = match client.get(format!("{base_url}/v1/health")).send().await {
                Ok(response) => response.status().is_success(),
                Err(error) => {
                    tracing::debug!(%slug, %error, "region health probe failed");
                    false
                }
            };
            (slug, base_url, healthy, start.elapsed())
        });
    }

    let mut best: Option<(Duration, SelectedRegion)> = None;
    while let Some(joined) = probes.join_next().await {
        let (slug, base_url, healthy, elapsed) = match joined {
            Ok(probe) => probe,
            Err(error) => {
                tracing::debug!(%error, "region health probe task failed");
                continue;
            }
        };
        if !healthy {
            continue;
        }
        if best.as_ref().is_none_or(|(rtt, _)| elapsed < *rtt) {
            best = Some((elapsed, SelectedRegion { slug, base_url }));
        }
    }

    best.map(|(_, region)| region)
        .ok_or_else(|| anyhow::anyhow!("no wardnet bridge region is reachable"))
}
