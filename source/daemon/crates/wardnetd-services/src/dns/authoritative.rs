use std::cmp::Reverse;
use std::collections::HashMap;
use std::net::SocketAddr;
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
    /// Enabled forwarding rules with domains pre-lowercased, sorted longest-first
    /// so the first suffix match is always the most specific one.
    forwarding_rules: Vec<ConditionalForwardingRule>,
    /// Enabled authoritative zones, sorted longest-name-first so the first
    /// suffix match is the most specific zone claiming the namespace.
    zone_authorities: Vec<ZoneAuthority>,
}

impl AuthoritativeView {
    /// An empty view — no local records, no forwarding rules.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            all_records: HashMap::new(),
            typed_records: HashMap::new(),
            forwarding_rules: Vec::new(),
            zone_authorities: Vec::new(),
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
            // Shared Arc: one heap allocation, two map entries.
            typed_records
                .entry(domain.clone())
                .or_default()
                .entry(rtype)
                .or_default()
                .push(Arc::clone(&record));
            all_records.entry(domain).or_default().push(record);
        }

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
            forwarding_rules,
            zone_authorities,
        }
    }

    /// Look up records for `domain_lower` matching `rtype`.
    ///
    /// Returns `None` if the domain is unknown entirely (fall through to
    /// cache + upstream). Returns `Some(&[])` if the domain is known but
    /// has no records of `rtype` (NOERROR empty with AA bit). No heap
    /// allocation — the outer map accepts `&str` directly.
    #[must_use]
    pub fn lookup(
        &self,
        domain_lower: &str,
        rtype: DnsRecordType,
    ) -> Option<&[Arc<CustomDnsRecord>]> {
        let type_map = self.typed_records.get(domain_lower)?;
        Some(type_map.get(&rtype).map_or(&[], Vec::as_slice))
    }

    /// All enabled records for `domain_lower` regardless of type (ANY queries).
    /// Returns `None` if the domain is unknown.
    #[must_use]
    pub fn lookup_all(&self, domain_lower: &str) -> Option<&[Arc<CustomDnsRecord>]> {
        self.all_records.get(domain_lower).map(Vec::as_slice)
    }

    /// The first CNAME record for `domain_lower`, if any.
    #[must_use]
    pub fn lookup_cname(&self, domain_lower: &str) -> Option<&Arc<CustomDnsRecord>> {
        self.typed_records
            .get(domain_lower)?
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
