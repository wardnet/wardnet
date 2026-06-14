//! Public **WAN** IP discovery.
//!
//! DDNS must publish the home's WAN IP, *not* a tunnel exit IP. Tunnel routing
//! is installed by `RoutingService` as **source-based** `ip rule`s keyed on each
//! device's source IP (`PolicyRouter::add_ip_rule(src_ip, table)`) — there is no
//! host-wide default route through a tunnel. The daemon's own sockets bind the
//! Pi's host IP (not a device IP), so they match no per-device rule and fall
//! through to the main table → the WAN default route. A plain HTTPS GET to an
//! IP-echo service therefore returns the WAN address. (Contrast
//! [`TunnelExitProbe`](crate::tunnel), which must use `SO_BINDTODEVICE` precisely
//! because default daemon egress is *not* through a tunnel.)
//!
//! Endpoints are tried in order; the first that returns a parseable, globally
//! routable IPv4 wins. A non-global answer (a proxy or captive portal echoing a
//! private address) is rejected rather than published.

use std::net::Ipv4Addr;

use wardnet_common::net::is_reserved_ipv4;

/// IP-echo endpoints, tried in order. Each returns the caller's IP as plain text.
pub(crate) const ECHO_ENDPOINTS: &[&str] = &["https://api.ipify.org", "https://icanhazip.com"];

/// Discover the home WAN IPv4 address via the built-in echo endpoints.
pub async fn discover_public_ip(client: &reqwest::Client) -> anyhow::Result<Ipv4Addr> {
    discover_from(client, ECHO_ENDPOINTS).await
}

/// Try each endpoint in order; return the first globally routable IPv4.
///
/// Factored out so tests can pass wiremock URLs; [`discover_public_ip`] passes
/// [`ECHO_ENDPOINTS`].
pub(crate) async fn discover_from(
    client: &reqwest::Client,
    endpoints: &[&str],
) -> anyhow::Result<Ipv4Addr> {
    let mut last_error = anyhow::anyhow!("no public-IP echo endpoints configured");
    for endpoint in endpoints {
        match fetch_ip(client, endpoint).await {
            Ok(ip) => return Ok(ip),
            Err(error) => {
                tracing::debug!(%endpoint, %error, "public-IP echo endpoint failed");
                last_error = error;
            }
        }
    }
    Err(last_error)
}

/// Fetch and validate the IP from a single echo endpoint.
async fn fetch_ip(client: &reqwest::Client, endpoint: &str) -> anyhow::Result<Ipv4Addr> {
    let text = client
        .get(endpoint)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    let ip: Ipv4Addr = text.trim().parse().map_err(|_| {
        anyhow::anyhow!("echo endpoint returned a non-IPv4 body: {:?}", text.trim())
    })?;
    if is_reserved_ipv4(ip) {
        anyhow::bail!("echo endpoint returned a non-global address: {ip}");
    }
    Ok(ip)
}
