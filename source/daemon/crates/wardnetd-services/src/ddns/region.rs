//! The built-in **region catalog**, latency-based selection, and the global
//! **gateway** endpoint.
//!
//! wardnet-cloud sits behind per-scope north-south **gateways** (cloud
//! ADR-0014/ADR-0015 / inforge ADR-0032/ADR-0034): one public edge per scope that
//! daemons HTTPS into, with the target service selected from the `X-Mesh-Target`
//! header the gateway derives per service — **not** a path prefix; paths are
//! prefix-free `/v1/…`. The global scope's gateway fronts `tenants`; each region's
//! gateway fronts that region's `ddns` + `tunneller`. The daemon must already
//! know a region's address to reach it, so the catalog ships in the daemon: a
//! **region slug** mapped to that region's gateway. At registration the daemon
//! probes every known region's health endpoint and registers against the
//! lowest-latency one, passing that slug as the network's `region`.
//!
//! Two endpoints per region matter:
//! * **control** (`https://api.<region-slug>…:443`) — the regional gateway; TLS
//!   is a normal public cert, requests route to the service by `X-Mesh-Target`.
//! * **health** (`http://ddns.svc.prd.<region-slug>…:81/readyz`) — the region's
//!   `ddns` service readiness probe on the plain-HTTP `:81` health listener
//!   (cloud ADR-0027). The `:81` listener serves **per-service** vhosts
//!   (`<service>.svc.prd.<region>…`) only — the gateway host answers 404 there,
//!   and `ddns` is the service registration is about to talk to, so its
//!   readiness IS the "region reachable" signal.
//!
//! The global gateway (`tenants`) is scope-wide, so it is a single constant
//! rather than a per-region catalog entry.
//!
//! Today only `euc` (eu-central) is deployed, but the probe-and-pick mechanism
//! is built in from the start so adding a region is a one-line catalog change.

use std::time::{Duration, Instant};

/// The **global gateway** base URL — the north-south edge fronting the global
/// `tenants` service (enroll / token / availability / networks under prefix-free
/// `/v1/…`, cloud ADR-0015). One deployment, region-independent.
///
/// Confirmed against wardnet-infrastructure `resources/prd/`: the global scope
/// drops the region label (ADR-0032's `api.<slug>.<base>` shape), so billing /
/// identity / tenants all answer here.
pub const GLOBAL_GATEWAY_URL: &str = "https://api.wardnet.network";

/// One entry in the built-in region catalog.
#[derive(Debug, Clone)]
pub struct RegionEndpoint {
    /// Short region slug, e.g. `euc`. Selects the region and is passed to
    /// `POST /v1/networks` as `region`.
    pub slug: String,
    /// The region's **gateway** base URL (`https://api.<region-slug>…`) — fronts
    /// the regional `ddns` and `tunneller` services (routing on `X-Mesh-Target`,
    /// cloud ADR-0015).
    pub gateway_base_url: String,
    /// Health-probe URL for region selection — the region's `ddns` service
    /// readiness probe (`http://ddns.svc.prd.<region-slug>…:81/readyz`, plain
    /// HTTP; cloud ADR-0027). The `:81` health listener routes by per-service
    /// vhost, not by the gateway host.
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
/// The slug is the **wire value**: it is sent verbatim as `region` on
/// `POST /v1/networks`, where the global `tenants` service checks it against its
/// `KNOWN_REGIONS` set and 400s on a miss. It is also the gateway's hostname
/// label (ADR-0032's `api.<slug>.<base>`). Both come from
/// wardnet-infrastructure `resources/prd/regions.yaml`, which is the single
/// authority: region `eu-central` carries `slug: euc`, and the deployed
/// `KNOWN_REGIONS` is `euc`. `eu-central` is the human name — never the slug.
#[must_use]
pub fn default_catalog() -> Vec<RegionEndpoint> {
    vec![RegionEndpoint::new(
        "euc",
        "https://api.euc.wardnet.network",
        "http://ddns.svc.prd.euc.wardnet.network:81/readyz",
    )]
}

/// A region chosen by [`select_best`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedRegion {
    pub slug: String,
    /// The region's gateway base URL (steady-state report-IP / ACME via
    /// prefix-free `/v1/…`, cloud ADR-0015).
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
