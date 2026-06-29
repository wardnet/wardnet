//! The built-in **region catalog**, latency-based selection, and the global
//! **tenants** endpoint.
//!
//! wardnet-cloud is split into a global `tenants` service (one deployment) and
//! per-region `ddns` services. The daemon must already know a region's address to
//! reach it, so the catalog ships in the daemon: a **region slug** mapped to that
//! region's `ddns` control endpoint. At registration the daemon probes every
//! known region's health endpoint and registers against the lowest-latency one,
//! passing that slug as the network's `region`.
//!
//! Two endpoints per region matter:
//! * **control** (`https://ddns.svc.<region>…:443`) — TLS-terminated by the edge
//!   ingress for the `ddns.svc` SNI; the daemon dials the FQDN directly so SNI is
//!   the hostname automatically.
//! * **health** (`http://ddns.svc.<region>…:81/v1/health`) — the edge ingress
//!   exposes health on plain-HTTP `:81`, Host-demuxed by service FQDN.
//!
//! The `tenants` endpoint is **global**, so it is a single constant rather than a
//! per-region catalog entry.
//!
//! Today only `use1` exists, but the probe-and-pick mechanism is built in from
//! the start so adding a region is a one-line catalog change.

use std::time::{Duration, Instant};

/// The global **tenants** service base URL (enroll / token / availability /
/// networks). One deployment, region-independent.
///
/// FIXME(infra): confirm the daemon-facing tenants FQDN. This is the tenants
/// ingress public vanity (`account.{BASE_DOMAIN}`); infra still marks it FIXME.
pub const TENANTS_BASE_URL: &str = "https://account.wardnet.network";

/// One entry in the built-in region catalog.
#[derive(Debug, Clone)]
pub struct RegionEndpoint {
    /// Short region slug, e.g. `use1`. Selects the region and is passed to
    /// `POST /v1/networks` as `region`.
    pub slug: String,
    /// `ddns` control base URL for this region (`https://ddns.svc.<region>…`).
    pub ddns_base_url: String,
    /// `ddns` health-probe URL for region selection
    /// (`http://ddns.svc.<region>…:81/v1/health`, plain HTTP).
    pub health_url: String,
}

impl RegionEndpoint {
    fn new(slug: &str, ddns_base_url: &str, health_url: &str) -> Self {
        Self {
            slug: slug.to_owned(),
            ddns_base_url: ddns_base_url.to_owned(),
            health_url: health_url.to_owned(),
        }
    }
}

/// The built-in catalog. Extend this to add regions.
///
/// FIXME(infra): confirm the derived `.svc` FQDN format/segment order against
/// inforge's record derivation (env `prd`, region slug `use1`). Kept as data here
/// so confirming it is a one-line change.
#[must_use]
pub fn default_catalog() -> Vec<RegionEndpoint> {
    vec![RegionEndpoint::new(
        "use1",
        "https://ddns.svc.prd.use1.wardnet.network",
        "http://ddns.svc.prd.use1.wardnet.network:81/v1/health",
    )]
}

/// A region chosen by [`select_best`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedRegion {
    pub slug: String,
    /// The region's `ddns` control base URL (steady-state report-IP / ACME).
    pub ddns_base_url: String,
}

/// Probe every entry's health endpoint and return the lowest-latency reachable
/// region.
///
/// Probes run **concurrently** (one task per region) so the wall time is the
/// slowest single probe, not their sum — and each region's measured RTT reflects
/// its own latency. Health is probed on the plain-HTTP `:81` endpoint; the
/// returned [`SelectedRegion`] carries the `:443` control base the daemon then
/// uses. Factored to take an explicit catalog so tests can drive it against
/// wiremock servers.
pub(crate) async fn select_best(
    client: &reqwest::Client,
    entries: &[RegionEndpoint],
) -> anyhow::Result<SelectedRegion> {
    let mut probes = tokio::task::JoinSet::new();
    for entry in entries {
        let client = client.clone();
        let entry = entry.clone();
        probes.spawn(async move {
            let start = Instant::now();
            let healthy = match client.get(&entry.health_url).send().await {
                Ok(response) => response.status().is_success(),
                Err(error) => {
                    tracing::debug!(slug = %entry.slug, %error, "region health probe failed");
                    false
                }
            };
            (entry, healthy, start.elapsed())
        });
    }

    let mut best: Option<(Duration, SelectedRegion)> = None;
    while let Some(joined) = probes.join_next().await {
        let (entry, healthy, elapsed) = match joined {
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
            best = Some((
                elapsed,
                SelectedRegion {
                    slug: entry.slug,
                    ddns_base_url: entry.ddns_base_url,
                },
            ));
        }
    }

    best.map(|(_, region)| region)
        .ok_or_else(|| anyhow::anyhow!("no wardnet ddns region is reachable"))
}
