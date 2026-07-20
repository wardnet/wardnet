use std::cmp::Reverse;
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use hickory_proto::rr::{Name, RData, Record, rdata::SOA};
use wardnet_common::dns::{ConditionalForwardingRule, CustomDnsRecord, DnsRecordType, DnsZone};

/// Negative-cache TTL (seconds) used for the synthetic SOA MINIMUM field and
/// the SOA record's own TTL on authoritative NXDOMAIN / NODATA answers.
const NEGATIVE_TTL: u32 = 300;

/// An enabled authoritative zone the gateway answers for directly. Holds the
/// lowercased zone name (used as a suffix to claim the namespace) and the SOA
/// serial derived from the zone's `updated_at`, so negative answers under the
/// zone can carry a synthetic SOA for RFC 2308 negative caching.
#[derive(Debug, Clone)]
pub struct ZoneAuthority {
    /// Lowercased, trailing-dot-trimmed zone name (e.g. `lan`, `home`).
    pub name: String,
    /// SOA serial — the zone's `updated_at` as epoch seconds.
    pub serial: u32,
    /// Pre-built synthetic SOA for this zone's negative answers, or `None`
    /// if the zone name can't form a valid DNS name. Built once when the
    /// view is constructed so the per-query path clones it instead of
    /// re-allocating three `format!` strings and re-parsing three DNS names.
    pub soa: Option<Record>,
}

/// In-memory snapshot of enabled local authoritative records and conditional
/// forwarding rules. Populated from the database at startup and rebuilt on
/// every [`WardnetEvent::DnsLocalChanged`] by the DNS runner.
///
/// Held behind an `Arc<ArcSwap<AuthoritativeView>>` in the server so readers
/// get a consistent, lock-free snapshot for the duration of each query.
pub struct AuthoritativeView {
    /// All enabled records for a domain ([`Self::lookup_all`], existence check, CNAME search).
    /// Values are `Arc`-wrapped so `typed_records` can share the same heap allocations.
    all_records: HashMap<String, Vec<Arc<CustomDnsRecord>>>,
    /// Records keyed by domain (outer) then record type (inner). The outer key accepts
    /// `&str` directly — no `to_owned()` allocation on the per-query hot path.
    typed_records: HashMap<String, HashMap<DnsRecordType, Vec<Arc<CustomDnsRecord>>>>,
    /// Wildcard records (`*.<suffix>`), sorted longest-suffix-first so the
    /// first match is the most specific one. Consulted only after an exact
    /// miss — an explicit record always beats a wildcard.
    wildcards: Vec<WildcardEntry>,
    /// Enabled forwarding rules with domains pre-lowercased, sorted longest-first
    /// so the first suffix match is always the most specific one.
    forwarding_rules: Vec<ConditionalForwardingRule>,
    /// Enabled authoritative zones, sorted longest-name-first so the first
    /// suffix match is the most specific zone claiming the namespace.
    zone_authorities: Vec<ZoneAuthority>,
    /// Reverse (PTR) map: private IPv4 address → the forward A records that
    /// point at it. Built by inverting every enabled, non-wildcard A record
    /// whose value is a private/internal address (RFC 1918, link-local, or
    /// RFC 6598 CGN). The DHCP `.lan` integration already writes those
    /// forward records and fires `DnsLocalChanged`, so this map rides the
    /// same rebuild with no extra data source. Consulted only for
    /// `in-addr.arpa` PTR queries; the record's own TTL is reused for the
    /// PTR answer.
    reverse_ptr: HashMap<Ipv4Addr, Vec<Arc<CustomDnsRecord>>>,
}

/// The compiled form of one wildcard domain (`*.<suffix>`): every enabled
/// record sharing that suffix, split the same way as the exact-match maps.
/// A name matches when it sits one **or more** labels below `suffix` — the
/// left-most-wildcard behaviour (issue #912's `<token>.<fqdn>` hostnames are
/// always exactly one label below). The bare suffix itself never matches.
struct WildcardEntry {
    /// Lowercased suffix (the domain with the `*.` stripped).
    suffix: String,
    all: Vec<Arc<CustomDnsRecord>>,
    typed: HashMap<DnsRecordType, Vec<Arc<CustomDnsRecord>>>,
}

impl AuthoritativeView {
    /// An empty view — no local records, no forwarding rules.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            all_records: HashMap::new(),
            typed_records: HashMap::new(),
            wildcards: Vec::new(),
            forwarding_rules: Vec::new(),
            zone_authorities: Vec::new(),
            reverse_ptr: HashMap::new(),
        }
    }

    /// Build a view from raw repository data.
    ///
    /// Only records that are both `enabled` and whose zone (if any) is also
    /// `enabled` are included. Records with `zone_id IS NULL` are always
    /// included when their own `enabled` flag is set. Forwarding-rule domains
    /// are pre-lowercased here so per-query matching requires no allocations.
    #[must_use]
    pub fn build(
        zones: &[DnsZone],
        records: Vec<CustomDnsRecord>,
        rules: Vec<ConditionalForwardingRule>,
    ) -> Self {
        let enabled_zone_ids: std::collections::HashSet<uuid::Uuid> =
            zones.iter().filter(|z| z.enabled).map(|z| z.id).collect();

        // Enabled zones double as authoritative suffixes: the gateway owns the
        // whole namespace, so unknown names under them are answered NXDOMAIN
        // rather than forwarded upstream. Sorted longest-first for most-specific
        // suffix matching.
        let mut zone_authorities: Vec<ZoneAuthority> = zones
            .iter()
            .filter(|z| z.enabled)
            .map(|z| {
                let name = z.name.trim_end_matches('.').to_ascii_lowercase();
                // SOA serial = the zone's `updated_at` epoch seconds. The
                // fallback only triggers for clocks before 1970 or after 2106
                // (outside u32 range) — surface it rather than swallow it.
                let serial = u32::try_from(z.updated_at.timestamp()).unwrap_or_else(|_| {
                    tracing::warn!(
                        zone = %name,
                        "zone {name} updated_at outside u32 range; using fallback SOA serial 1"
                    );
                    1
                });
                // Pre-build the synthetic SOA once. If the (DB-validated) zone
                // name still can't form a valid DNS name, negative answers for
                // the zone simply omit the SOA.
                let soa = match build_soa(&name, serial) {
                    Ok(record) => Some(record),
                    Err(e) => {
                        tracing::warn!(
                            zone = %name,
                            error = %e,
                            "failed to build synthetic SOA for zone {name}: {e}; negative answers will omit it"
                        );
                        None
                    }
                };
                ZoneAuthority { name, serial, soa }
            })
            .collect();
        zone_authorities.sort_by_key(|z| Reverse(z.name.len()));

        let mut all_records: HashMap<String, Vec<Arc<CustomDnsRecord>>> = HashMap::new();
        let mut typed_records: HashMap<String, HashMap<DnsRecordType, Vec<Arc<CustomDnsRecord>>>> =
            HashMap::new();
        let mut wildcard_map: HashMap<String, WildcardEntry> = HashMap::new();
        let mut reverse_ptr: HashMap<Ipv4Addr, Vec<Arc<CustomDnsRecord>>> = HashMap::new();

        for record in records {
            if !record.enabled {
                continue;
            }
            if record
                .zone_id
                .is_some_and(|z| !enabled_zone_ids.contains(&z))
            {
                continue;
            }
            let domain = record.domain.trim_end_matches('.').to_ascii_lowercase();
            let rtype = record.record_type;
            let record = Arc::new(record);

            // Wildcard record — compiled into the suffix-matched list rather
            // than the exact-match maps (issue #912).
            if let Some(suffix) = domain.strip_prefix("*.") {
                let entry =
                    wildcard_map
                        .entry(suffix.to_owned())
                        .or_insert_with(|| WildcardEntry {
                            suffix: suffix.to_owned(),
                            all: Vec::new(),
                            typed: HashMap::new(),
                        });
                entry
                    .typed
                    .entry(rtype)
                    .or_default()
                    .push(Arc::clone(&record));
                entry.all.push(record);
                continue;
            }

            // Reverse (PTR) index: an A record for a private address doubles as
            // the answer to that address's `in-addr.arpa` lookup. Only private
            // targets are indexed — public IPs keep resolving reverse upstream —
            // so the map mirrors exactly the addresses the pipeline answers for.
            if rtype == DnsRecordType::A
                && let Ok(v4) = record.value.parse::<Ipv4Addr>()
                && private_reverse_zone(v4).is_some()
            {
                reverse_ptr.entry(v4).or_default().push(Arc::clone(&record));
            }

            // Shared Arc: one heap allocation, two map entries.
            typed_records
                .entry(domain.clone())
                .or_default()
                .entry(rtype)
                .or_default()
                .push(Arc::clone(&record));
            all_records.entry(domain).or_default().push(record);
        }

        let mut wildcards: Vec<WildcardEntry> = wildcard_map.into_values().collect();
        wildcards.sort_by_key(|w| Reverse(w.suffix.len()));

        let mut forwarding_rules: Vec<ConditionalForwardingRule> = rules
            .into_iter()
            .filter(|r| r.enabled)
            .map(|r| ConditionalForwardingRule {
                domain: r.domain.to_ascii_lowercase(),
                ..r
            })
            .collect();
        forwarding_rules.sort_by_key(|r| Reverse(r.domain.len()));

        Self {
            all_records,
            typed_records,
            wildcards,
            forwarding_rules,
            zone_authorities,
            reverse_ptr,
        }
    }

    /// The most specific wildcard entry covering `domain_lower`, if any. A
    /// match requires at least one label left of the suffix — the bare
    /// suffix (apex) never matches its own wildcard.
    fn match_wildcard(&self, domain_lower: &str) -> Option<&WildcardEntry> {
        self.wildcards.iter().find(|w| {
            domain_lower
                .strip_suffix(w.suffix.as_str())
                .is_some_and(|prefix| prefix.len() > 1 && prefix.ends_with('.'))
        })
    }

    /// Look up records for `domain_lower` matching `rtype`.
    ///
    /// Returns `None` if the domain is unknown entirely (fall through to
    /// cache + upstream). Returns `Some(&[])` if the domain is known but
    /// has no records of `rtype` (NOERROR empty with AA bit). No heap
    /// allocation — the outer map accepts `&str` directly.
    ///
    /// **Wildcard fallback (issue #912):** an exact miss falls back to the
    /// most specific covering wildcard, but *only for a type the wildcard
    /// actually carries* — a type-miss under a wildcard returns `None` (fall
    /// through to forwarding/upstream), **not** `Some(&[])`. Unlike an
    /// explicit record, which authoritatively owns its exact name for every
    /// type (an A record NODATA-answers an AAAA query for that one name), a
    /// wildcard covers an entire subtree, so synthesising NODATA for the
    /// types it doesn't carry would blackhole AAAA/MX/TXT/… for every name
    /// under the suffix even when a conditional-forwarding rule or a public
    /// zone serves them. Answering only the carried types keeps the
    /// daemon-owned `*.<fqdn>` A record (the sole v1 use) working while
    /// leaving everything else to resolve normally.
    #[must_use]
    pub fn lookup(
        &self,
        domain_lower: &str,
        rtype: DnsRecordType,
    ) -> Option<&[Arc<CustomDnsRecord>]> {
        if let Some(type_map) = self.typed_records.get(domain_lower) {
            return Some(type_map.get(&rtype).map_or(&[], Vec::as_slice));
        }
        let wildcard = self.match_wildcard(domain_lower)?;
        // Some only when the wildcard carries this type; a type-miss falls
        // through (None) rather than claiming authoritative NODATA.
        wildcard.typed.get(&rtype).map(Vec::as_slice)
    }

    /// All enabled records for `domain_lower` regardless of type (ANY queries).
    /// Returns `None` if the domain is unknown. A wildcard-covered name
    /// resolves to the wildcard's records; unlike [`Self::lookup`] there is
    /// no per-type shadowing question for an ANY query (it wants everything
    /// the name has), so a covered name always yields the wildcard's set.
    #[must_use]
    pub fn lookup_all(&self, domain_lower: &str) -> Option<&[Arc<CustomDnsRecord>]> {
        if let Some(records) = self.all_records.get(domain_lower) {
            return Some(records.as_slice());
        }
        self.match_wildcard(domain_lower).map(|w| w.all.as_slice())
    }

    /// The first CNAME record for `domain_lower`, if any. Falls back to a
    /// covering wildcard's CNAME (issue #912) so the pipeline's A/AAAA
    /// CNAME-chase sees wildcard CNAMEs the same way [`Self::lookup`] does.
    #[must_use]
    pub fn lookup_cname(&self, domain_lower: &str) -> Option<&Arc<CustomDnsRecord>> {
        if let Some(type_map) = self.typed_records.get(domain_lower) {
            return type_map.get(&DnsRecordType::Cname).and_then(|v| v.first());
        }
        self.match_wildcard(domain_lower)?
            .typed
            .get(&DnsRecordType::Cname)
            .and_then(|v| v.first())
    }

    /// First-match conditional forwarding rule for `domain_lower`.
    ///
    /// Performs exact match and suffix match with no heap allocations: rule
    /// domains are pre-lowercased in [`Self::build`], and the suffix check
    /// uses `str::strip_suffix` rather than `format!`. Because
    /// `forwarding_rules` is sorted longest-first, the first match is always
    /// the most specific one.
    #[must_use]
    pub fn match_forwarding_rule(&self, domain_lower: &str) -> Option<&ConditionalForwardingRule> {
        self.forwarding_rules.iter().find(|r| {
            // Exact match or valid subdomain suffix (must be preceded by '.').
            domain_lower == r.domain.as_str()
                || domain_lower
                    .strip_suffix(r.domain.as_str())
                    .is_some_and(|prefix| prefix.ends_with('.'))
        })
    }

    /// The most specific enabled authoritative zone that `domain_lower` falls
    /// under — an exact match (`domain == zone`) or a `.zone` suffix. Returns
    /// `None` if the name is not inside any enabled zone. `zone_authorities` is
    /// sorted longest-first, so the first match is the most specific zone.
    #[must_use]
    pub fn authoritative_zone(&self, domain_lower: &str) -> Option<&ZoneAuthority> {
        self.zone_authorities.iter().find(|z| {
            domain_lower == z.name.as_str()
                || domain_lower
                    .strip_suffix(z.name.as_str())
                    .is_some_and(|prefix| prefix.ends_with('.'))
        })
    }

    /// The forward A records naming `ip` (built from the reverse index), for
    /// answering an `in-addr.arpa` PTR query. `None` when no enabled record
    /// points at the address — the pipeline then answers an authoritative
    /// NXDOMAIN for a private address, so the reverse lookup never leaks
    /// upstream. Each record carries the TTL and hostname to echo back.
    #[must_use]
    pub fn reverse_lookup(&self, ip: Ipv4Addr) -> Option<&[Arc<CustomDnsRecord>]> {
        self.reverse_ptr.get(&ip).map(Vec::as_slice)
    }
}

/// Parse an `in-addr.arpa` PTR query name into the IPv4 address it asks about.
///
/// `name_lower` must be lowercased and trailing-dot-trimmed (the pipeline
/// normalizes it that way before matching). Returns `None` for anything that
/// isn't exactly four numeric octet labels followed by `in-addr.arpa` — a
/// zone-apex query (`168.192.in-addr.arpa`), an RFC 2317 classless
/// delegation label, or an `ip6.arpa` name all fall through to normal
/// forwarding rather than being answered locally.
#[must_use]
pub fn parse_ptr_ipv4(name_lower: &str) -> Option<Ipv4Addr> {
    let labels = name_lower.strip_suffix(".in-addr.arpa")?;
    let mut parts = labels.split('.');
    let a = parts.next()?.parse::<u8>().ok()?;
    let b = parts.next()?.parse::<u8>().ok()?;
    let c = parts.next()?.parse::<u8>().ok()?;
    let d = parts.next()?.parse::<u8>().ok()?;
    if parts.next().is_some() {
        return None; // more than four labels — not a single-host PTR
    }
    // `in-addr.arpa` lists the octets least-significant first, so reverse them.
    Some(Ipv4Addr::new(d, c, b, a))
}

/// The RFC 6303 private-use reverse zone apex that `ip` falls under, or `None`
/// if the address is not in a private/internal block.
///
/// These are the reverse zones a recursive resolver should answer for locally
/// instead of leaking upstream (RFC 6303 §4): RFC 1918 (10/8, 172.16/12,
/// 192.168/16), link-local (169.254/16), and the RFC 6598 shared CGN space
/// (100.64/10). The returned name is the zone apex used as the SOA owner on
/// authoritative NXDOMAIN answers, so downstream resolvers can negatively
/// cache the miss (RFC 2308) and stop re-asking.
#[must_use]
pub fn private_reverse_zone(ip: Ipv4Addr) -> Option<String> {
    match ip.octets() {
        [10, ..] => Some("10.in-addr.arpa".to_owned()),
        [172, b, ..] if (16..=31).contains(&b) => Some(format!("{b}.172.in-addr.arpa")),
        [192, 168, ..] => Some("168.192.in-addr.arpa".to_owned()),
        [169, 254, ..] => Some("254.169.in-addr.arpa".to_owned()),
        [100, b, ..] if (64..=127).contains(&b) => Some(format!("{b}.100.in-addr.arpa")),
        _ => None,
    }
}

/// Build a synthetic SOA record for an authoritative zone, placed in the
/// authority section of NXDOMAIN / NODATA answers so downstream resolvers can
/// negatively cache per RFC 2308. The SOA MINIMUM field (and the record TTL)
/// is [`NEGATIVE_TTL`]. Returns an error only if the zone name can't form a
/// valid DNS name.
pub fn build_soa(zone: &str, serial: u32) -> anyhow::Result<Record> {
    let apex = Name::from_utf8(format!("{zone}."))?;
    let mname = Name::from_utf8(format!("ns.{zone}."))?;
    let rname = Name::from_utf8(format!("hostmaster.{zone}."))?;
    // refresh 1h, retry 10m, expire 7d, minimum = negative TTL.
    let soa = SOA::new(mname, rname, serial, 3600, 600, 604_800, NEGATIVE_TTL);
    Ok(Record::from_rdata(apex, NEGATIVE_TTL, RData::SOA(soa)))
}

/// Parse a forwarding rule upstream string into a `SocketAddr`.
///
/// If the string already contains a colon (IPv4+port or bracketed IPv6),
/// parse directly; otherwise append `:53`.
pub fn parse_conditional_upstream(s: &str) -> anyhow::Result<SocketAddr> {
    let with_port = if s.starts_with('[') {
        // Bracketed IPv6 — already has port or needs :53.
        if s.parse::<SocketAddr>().is_ok() {
            s.to_owned()
        } else {
            format!("{s}:53")
        }
    } else if s.contains(':') {
        // Plain IPv6 literal (no port) or IPv4+port.
        // Attempt direct parse; if it fails assume bare IPv6 and bracket it.
        if s.parse::<SocketAddr>().is_ok() {
            s.to_owned()
        } else {
            format!("[{s}]:53")
        }
    } else {
        // Plain IPv4 or hostname — append default port.
        format!("{s}:53")
    };
    with_port
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid upstream address {s:?}: {e}"))
}
