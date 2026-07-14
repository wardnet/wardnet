//! Minimal **DoH-JSON** client for the external resolution check (C10).
//!
//! The [resolution check](super::DdnsService::resolution_check) confirms that the
//! *public* internet resolves the canonical FQDN to the IP the daemon published —
//! a diagnostic that must deliberately **bypass the daemon's own split-horizon
//! override**. It does that by querying public resolvers **addressed by IP** over
//! HTTPS (`https://1.1.1.1/dns-query`, `https://8.8.8.8/dns-query`): the resolver
//! host is a literal IP, so its name never passes through the local stub, and the
//! resolvers' TLS certificates carry those IPs as SANs, so ordinary certificate
//! validation still applies.
//!
//! The query uses the de-facto **`application/dns-json`** convenience API (Google
//! defined it; Cloudflare serves the identical schema), so there is no DNS
//! wire-format encoding to do — a single GET and a small JSON parse. Both pinned
//! resolvers support it; the list is fixed precisely so we can rely on it (an
//! arbitrary resolver might speak only RFC 8484 wire). See `CONTEXT.md`,
//! "Resolution check".

use std::net::Ipv4Addr;

use serde::Deserialize;

/// Public DoH-JSON resolvers, queried **by IP** in order until one answers.
/// Cloudflare + Google for redundancy; both serve the same JSON schema but at
/// different paths — Cloudflare answers JSON on `/dns-query` (per its `accept`
/// header), while Google serves JSON **only** on `/resolve` (`/dns-query` is
/// RFC 8484 wire format and rejects a JSON request with HTTP 400).
pub(crate) const DOH_RESOLVERS: &[&str] = &["https://1.1.1.1/dns-query", "https://8.8.8.8/resolve"];

/// DNS `A` record type number, used to pick A answers out of a CNAME chain.
const DNS_TYPE_A: u16 = 1;
/// `Status` values that are a *definitive* answer (vs. a server failure we should
/// retry against the other resolver): `0` = NOERROR, `3` = NXDOMAIN.
const STATUS_NOERROR: u32 = 0;
const STATUS_NXDOMAIN: u32 = 3;

#[derive(Deserialize)]
struct DohResponse {
    #[serde(rename = "Status")]
    status: u32,
    #[serde(default, rename = "Answer")]
    answer: Vec<DohAnswer>,
}

#[derive(Deserialize)]
struct DohAnswer {
    #[serde(rename = "type")]
    record_type: u16,
    data: String,
}

/// Resolve the `A` records for `fqdn` through the DoH-JSON endpoint at
/// `resolver_url` (a full `https://<ip>/dns-query` URL).
///
/// Returns the parsed A addresses — **empty on NXDOMAIN or a name with no A
/// record**, which the caller maps to the `Pending` verdict (the normal state in
/// the propagation window right after registration). A transport/parse failure,
/// or a non-definitive `Status` (e.g. SERVFAIL), is an `Err` so the caller can
/// fall through to the next resolver.
pub(crate) async fn resolve_a(
    client: &reqwest::Client,
    resolver_url: &str,
    fqdn: &str,
) -> anyhow::Result<Vec<Ipv4Addr>> {
    let response = client
        .get(resolver_url)
        .header(reqwest::header::ACCEPT, "application/dns-json")
        .query(&[("name", fqdn), ("type", "A")])
        .send()
        .await?
        .error_for_status()?;
    let parsed: DohResponse = response.json().await?;

    if parsed.status != STATUS_NOERROR && parsed.status != STATUS_NXDOMAIN {
        anyhow::bail!(
            "DoH resolver {resolver_url} returned non-definitive Status {}",
            parsed.status
        );
    }

    Ok(parsed
        .answer
        .into_iter()
        .filter(|a| a.record_type == DNS_TYPE_A)
        .filter_map(|a| a.data.parse::<Ipv4Addr>().ok())
        .collect())
}
