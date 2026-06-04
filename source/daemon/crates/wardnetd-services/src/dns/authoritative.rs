use std::cmp::Reverse;
use std::collections::HashMap;
use std::net::SocketAddr;

use wardnet_common::dns::{ConditionalForwardingRule, CustomDnsRecord, DnsRecordType, DnsZone};

/// In-memory snapshot of enabled local authoritative records and conditional
/// forwarding rules. Populated from the database at startup and rebuilt on
/// every [`WardnetEvent::DnsLocalChanged`] by the DNS runner.
///
/// Held behind an `Arc<ArcSwap<AuthoritativeView>>` in the server so readers
/// get a consistent, lock-free snapshot for the duration of each query.
pub struct AuthoritativeView {
    /// All enabled records for a domain ([`Self::lookup_all`], existence check).
    all_records: HashMap<String, Vec<CustomDnsRecord>>,
    /// Records keyed by (domain, type) for fast typed lookup.
    typed_records: HashMap<(String, DnsRecordType), Vec<CustomDnsRecord>>,
    /// Enabled conditional forwarding rules sorted by domain length descending
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
    /// included when their own `enabled` flag is set.
    #[must_use]
    pub fn build(
        zones: &[DnsZone],
        records: Vec<CustomDnsRecord>,
        rules: Vec<ConditionalForwardingRule>,
    ) -> Self {
        let enabled_zone_ids: std::collections::HashSet<uuid::Uuid> =
            zones.iter().filter(|z| z.enabled).map(|z| z.id).collect();

        let mut all_records: HashMap<String, Vec<CustomDnsRecord>> = HashMap::new();
        let mut typed_records: HashMap<(String, DnsRecordType), Vec<CustomDnsRecord>> =
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
            let key = (domain.clone(), record.record_type);
            typed_records.entry(key).or_default().push(record.clone());
            all_records.entry(domain).or_default().push(record);
        }

        let mut forwarding_rules: Vec<ConditionalForwardingRule> =
            rules.into_iter().filter(|r| r.enabled).collect();
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
    /// has no records of `rtype` (NOERROR empty with AA bit).
    #[must_use]
    pub fn lookup(&self, domain_lower: &str, rtype: DnsRecordType) -> Option<&[CustomDnsRecord]> {
        if !self.all_records.contains_key(domain_lower) {
            return None;
        }
        Some(
            self.typed_records
                .get(&(domain_lower.to_owned(), rtype))
                .map_or(&[], Vec::as_slice),
        )
    }

    /// All enabled records for `domain_lower` regardless of type (ANY queries).
    /// Returns `None` if the domain is unknown.
    #[must_use]
    pub fn lookup_all(&self, domain_lower: &str) -> Option<&[CustomDnsRecord]> {
        self.all_records.get(domain_lower).map(Vec::as_slice)
    }

    /// The first CNAME record for `domain_lower`, if any.
    #[must_use]
    pub fn lookup_cname(&self, domain_lower: &str) -> Option<&CustomDnsRecord> {
        self.typed_records
            .get(&(domain_lower.to_owned(), DnsRecordType::Cname))
            .and_then(|v| v.first())
    }

    /// First-match conditional forwarding rule for `domain_lower`.
    ///
    /// Checks exact match and suffix match (`domain_lower` ends with
    /// `"." + rule.domain`). Because `forwarding_rules` is sorted longest-
    /// first, the first match is always the most specific one.
    #[must_use]
    pub fn match_forwarding_rule(&self, domain_lower: &str) -> Option<&ConditionalForwardingRule> {
        self.forwarding_rules.iter().find(|r| {
            let rule_domain = r.domain.to_ascii_lowercase();
            domain_lower == rule_domain || domain_lower.ends_with(&format!(".{rule_domain}"))
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
