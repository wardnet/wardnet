//! Classification of upstream DNS responses for caching.
//!
//! This is the pure decision the forwarders in `wardnetd`'s query
//! pipeline apply to every datagram they receive back from an upstream
//! resolver, before relaying or caching it. It lives here, next to
//! [`DnsCache`](super::cache::DnsCache), because "how long is this
//! response cacheable" is a cache concern — the pipeline only orchestrates
//! the socket I/O around it.

use hickory_proto::op::{Message, ResponseCode};

/// Classify a parsed upstream response for caching: positive answers use the
/// answers' minimum TTL; negative answers (NXDOMAIN, or NOERROR with zero
/// answers) use the RFC 2308 negative TTL — min(SOA record TTL, SOA MINIMUM)
/// from the authority section, or 0 (uncacheable) when the zone forbids
/// negative caching or carries no SOA. Returns `(is_negative, raw_ttl)`.
#[must_use]
pub fn classify_response(parsed: &Message) -> (bool, u32) {
    use hickory_proto::rr::RData;

    let negative = parsed.metadata.response_code == ResponseCode::NXDomain
        || (parsed.metadata.response_code == ResponseCode::NoError && parsed.answers.is_empty());
    if negative {
        let ttl = parsed
            .authorities
            .iter()
            .find_map(|r| match &r.data {
                RData::SOA(soa) => Some(r.ttl.min(soa.minimum)),
                _ => None,
            })
            .unwrap_or(0);
        return (true, ttl);
    }
    let min_ttl = parsed.answers.iter().map(|r| r.ttl).min().unwrap_or(0);
    (false, min_ttl)
}

/// Parse arbitrary upstream-response bytes and run [`classify_response`] over
/// them — the sync core of the response-handling path in the `wardnetd`
/// forwarders, minus the socket I/O.
///
/// This is the trust boundary where wardnet decodes bytes served by an
/// upstream resolver (or an off-path attacker who beats it): `Message`
/// wire-parsing followed by our own walk over the answer/authority sections
/// and SOA-TTL arithmetic. Exposed for the `dns_response` fuzz harness and
/// gated on the `fuzz-internals` cargo feature so production builds never
/// pick up a public entry-point by accident.
///
/// Returns `None` when the bytes don't parse as a DNS message (the
/// relay-raw-and-skip-caching branch); otherwise `Some((is_negative,
/// raw_ttl))`, exactly what the forwarders compute before a cache insert.
///
/// See `source/daemon/fuzz/fuzz_targets/dns_response.rs`.
#[cfg(feature = "fuzz-internals")]
#[must_use]
pub fn parse_and_classify_response_for_fuzz(bytes: &[u8]) -> Option<(bool, u32)> {
    use hickory_proto::serialize::binary::BinDecodable;

    let parsed = Message::from_bytes(bytes).ok()?;
    Some(classify_response(&parsed))
}
