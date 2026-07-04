//! The built-in **region catalog**, latency-based selection, and the global
//! **gateway** endpoint.
//!
//! wardnet-cloud sits behind per-scope north-south **gateways** (cloud ADR-0014 /
//! inforge ADR-0032): one public edge per scope that daemons HTTPS into, with the
//! target service selected by the **first path segment** (`/tenants/…`, `/ddns/…`,
//! `/tunneller/…`). The global scope's gateway fronts `tenants`; each region's
//! gateway fronts that region's `ddns` + `tunneller`. The daemon must already
//! know a region's address to reach it, so the catalog ships in the daemon: a
//! **region slug** mapped to that region's gateway. At registration the daemon
//! probes every known region's health endpoint and registers against the
//! lowest-latency one, passing that slug as the network's `region`.
//!
//! Two endpoints per region matter:
//! * **control** (`https://api.<region-slug>…:443`) — the regional gateway; TLS
//!   is a normal public cert, requests are path-routed to the service.
//! * **health** (`http://api.<region-slug>…:81/ddns/v1/health`) — the gateway
//!   host exposes health on plain-HTTP `:81`.
//!
//! The global gateway (`tenants`) is scope-wide, so it is a single constant
//! rather than a per-region catalog entry.
//!
//! Today only `use1` exists, but the probe-and-pick mechanism is built in from
//! the start so adding a region is a one-line catalog change.

use std::time::{Duration, Instant};

/// The **global gateway** base URL — the north-south edge fronting the global
/// `tenants` service (enroll / token / availability / networks under
/// `/tenants/v1/…`). One deployment, region-independent.
///
/// FIXME(infra): confirm the daemon-facing gateway FQDN once the gateway
/// manifest lands in wardnet-infrastructure; ADR-0032's shape is
/// `api.<slug>.<base>` with the global scope dropping the slug.
pub const GLOBAL_GATEWAY_URL: &str = "https://api.wardnet.network";

/// One entry in the built-in region catalog.
#[derive(Debug, Clone)]
pub struct RegionEndpoint {
    /// Short region slug, e.g. `use1`. Selects the region and is passed to
    /// `POST /tenants/v1/networks` as `region`.
    pub slug: String,
    /// The region's **gateway** base URL (`https://api.<region-slug>…`) — fronts
    /// the regional `ddns` and `tunneller` services by path prefix.
    pub gateway_base_url: String,
    /// Health-probe URL for region selection
    /// (`http://api.<region-slug>…:81/ddns/v1/health`, plain HTTP).
    pub health_url: String,
}

impl RegionEndpoint {
    fn new(slug: &str, gateway_base_url: &str, health_url: &str) -> Self {
        Self {
            slug: slug.to_owned(),
            gateway_base_url: gateway_base_url.to_owned(),
            health_url: health_url.to_owned(),
        }
    }
}

/// The built-in catalog. Extend this to add regions.
///
/// FIXME(infra): confirm the regional gateway FQDN against the gateway manifest
/// once it lands in wardnet-infrastructure (ADR-0032 shape `api.<slug>.<base>`,
/// region slug `use1`). Kept as data here so confirming it is a one-line change.
#[must_use]
pub fn default_catalog() -> Vec<RegionEndpoint> {
    vec![RegionEndpoint::new(
        "use1",
        "https://api.use1.wardnet.network",
        "http://api.use1.wardnet.network:81/ddns/v1/health",
    )]
}

/// A region chosen by [`select_best`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedRegion {
    pub slug: String,
    /// The region's gateway base URL (steady-state report-IP / ACME via
    /// `/ddns/v1/…`).
    pub gateway_base_url: String,
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
                    tracing::debug!(slug = %entry.slug, %error, "region health probe failed for {}: {error}", entry.slug);
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
                    gateway_base_url: entry.gateway_base_url,
                },
            ));
        }
    }

    best.map(|(_, region)| region)
        .ok_or_else(|| anyhow::anyhow!("no wardnet ddns region is reachable"))
}
