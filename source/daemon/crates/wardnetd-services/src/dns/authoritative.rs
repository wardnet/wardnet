use std::cmp::Reverse;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use wardnet_common::dns::{ConditionalForwardingRule, CustomDnsRecord, DnsRecordType, DnsZone};

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
}

impl AuthoritativeView {
    /// An empty view — no local records, no forwarding rules.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            all_records: HashMap::new(),
            typed_records: HashMap::new(),
            forwarding_rules: Vec::new(),
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
