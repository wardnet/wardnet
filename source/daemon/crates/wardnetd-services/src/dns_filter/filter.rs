//! Per-source DNS filter.
//!
//! A [`DnsFilter`] owns the parsed rules contributed by ONE filter source
//! (a blocklist, an allowlist, or a custom-rules set scoped to a profile).
//! It returns a [`MatchOutcome`] from [`DnsFilter::check`] which a caller
//! aggregates across the rest of a profile's filters.
//!
//! This is the rebuild atom: when a single blocklist is re-downloaded, the
//! runner swaps just that one filter via `ArcSwap`, and every profile holding
//! an `Arc<ArcSwap<DnsFilter>>` to it transparently sees the new state.

use std::borrow::Cow;
use std::collections::HashSet;
use std::net::IpAddr;

use hickory_proto::rr::RecordType;
use wardnet_common::dns::FilterAction;

use crate::dns::filter_parser::{self, ParsedRule, RuleModifiers};

/// Inputs required to build a [`DnsFilter`].
#[derive(Debug, Clone, Default)]
pub struct DnsFilterInputs {
    /// Deduplicated, lowercased domains from one blocklist's enabled rows.
    pub blocked_domains: Vec<String>,
    /// Allowlist domains contributed by one source.
    pub allowlist: Vec<String>,
    /// Raw rule text from one custom-rules source.
    pub custom_rules: Vec<String>,
}

/// Aggregate counts; useful for `/api/dns/status`.
#[derive(Debug, Clone, Copy, Default)]
pub struct FilterStats {
    pub blocked_count: usize,
    pub allowed_count: usize,
    pub complex_count: usize,
}

/// Outcome of evaluating a single [`DnsFilter`] against a query. The
/// containing profile aggregates these across filters using the natural
/// [`MatchKind`] precedence: `ImportantAllow` beats `ImportantBlock`
/// beats `Allow` beats `Block`. `Rewrite` short-circuits and is returned
/// to the service directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchOutcome {
    None,
    Match(MatchKind),
    Rewrite { ip: IpAddr },
}

/// Precedence-ordered match kind. Larger rank wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchKind {
    Block,
    Allow,
    ImportantBlock,
    ImportantAllow,
}

impl MatchKind {
    #[must_use]
    pub fn rank(self) -> u8 {
        match self {
            Self::Block => 1,
            Self::Allow => 2,
            Self::ImportantBlock => 3,
            Self::ImportantAllow => 4,
        }
    }

    #[must_use]
    pub fn blocks(self) -> bool {
        matches!(self, Self::Block | Self::ImportantBlock)
    }
}

/// One filter source's compiled rules. Cheap to swap; expensive to build.
#[derive(Debug, Default)]
pub struct DnsFilter {
    blocked_domains: HashSet<String>,
    allowed_domains: HashSet<String>,
    complex: Vec<ParsedRule>,
}

impl DnsFilter {
    #[must_use]
    pub fn build(inputs: DnsFilterInputs) -> Self {
        let mut blocked_domains: HashSet<String> = inputs
            .blocked_domains
            .into_iter()
            .map(|d| normalize_owned(&d))
            .collect();
        let mut allowed_domains: HashSet<String> = inputs
            .allowlist
            .into_iter()
            .map(|d| normalize_owned(&d))
            .collect();
        let mut complex = Vec::new();

        for line in inputs.custom_rules {
            match filter_parser::parse_line(&line) {
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!(error = %e, rule = %line, "skipping invalid custom rule");
                }
                Ok(Some(rule)) => match rule {
                    ParsedRule::DomainBlock {
                        ref domain,
                        ref modifiers,
                        allow,
                    } if modifiers.is_empty() => {
                        if allow {
                            allowed_domains.insert(domain.clone());
                        } else {
                            blocked_domains.insert(domain.clone());
                        }
                    }
                    other => complex.push(other),
                },
            }
        }

        Self {
            blocked_domains,
            allowed_domains,
            complex,
        }
    }

    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.blocked_domains.is_empty()
            && self.allowed_domains.is_empty()
            && self.complex.is_empty()
    }

    #[must_use]
    pub fn stats(&self) -> FilterStats {
        FilterStats {
            blocked_count: self.blocked_domains.len(),
            allowed_count: self.allowed_domains.len(),
            complex_count: self.complex.len(),
        }
    }

    /// Evaluate this source against a query and return the strongest
    /// outcome it contributes.
    #[must_use]
    pub fn check(&self, domain: &str, qtype: RecordType, client: IpAddr) -> MatchOutcome {
        let d = normalize(domain);
        let mut best: Option<MatchKind> = None;

        for rule in &self.complex {
            if !rule_matches(rule, &d) {
                continue;
            }
            if !modifiers_apply(rule.modifiers(), qtype, client) {
                continue;
            }

            if let Some(ip) = rule.modifiers().rewrite_to {
                return MatchOutcome::Rewrite { ip };
            }

            upgrade(&mut best, classify(rule));
        }

        if matches_subdomain(&d, &self.allowed_domains) {
            upgrade(&mut best, MatchKind::Allow);
        }
        if matches_subdomain(&d, &self.blocked_domains) {
            upgrade(&mut best, MatchKind::Block);
        }

        best.map_or(MatchOutcome::None, MatchOutcome::Match)
    }

    /// Convenience for tests/single-source legacy callers — translate
    /// [`MatchOutcome`] back into a [`FilterAction`].
    #[must_use]
    pub fn check_action(&self, domain: &str, qtype: RecordType, client: IpAddr) -> FilterAction {
        match self.check(domain, qtype, client) {
            MatchOutcome::Rewrite { ip } => FilterAction::Rewrite { ip },
            MatchOutcome::Match(kind) if kind.blocks() => FilterAction::Block,
            _ => FilterAction::Pass,
        }
    }
}

fn classify(rule: &ParsedRule) -> MatchKind {
    let important = rule.modifiers().important;
    let allow = rule.is_allow();
    match (important, allow) {
        (true, true) => MatchKind::ImportantAllow,
        (true, false) => MatchKind::ImportantBlock,
        (false, true) => MatchKind::Allow,
        (false, false) => MatchKind::Block,
    }
}

fn upgrade(cur: &mut Option<MatchKind>, new: MatchKind) {
    let replace = cur.is_none_or(|c| new.rank() > c.rank());
    if replace {
        *cur = Some(new);
    }
}

fn rule_matches(rule: &ParsedRule, domain: &str) -> bool {
    match rule {
        ParsedRule::DomainBlock { domain: d, .. } => is_subdomain_of(domain, d),
        ParsedRule::Pattern { regex, .. } | ParsedRule::Regex { regex, .. } => {
            regex.is_match(domain)
        }
    }
}

fn modifiers_apply(mods: &RuleModifiers, qtype: RecordType, client: IpAddr) -> bool {
    if let Some(types) = &mods.dns_types
        && !types.contains(&qtype)
    {
        return false;
    }
    if let Some(nets) = &mods.clients
        && !nets.iter().any(|n| n.contains(client))
    {
        return false;
    }
    true
}

fn is_subdomain_of(domain: &str, rule_domain: &str) -> bool {
    if domain == rule_domain {
        return true;
    }
    if domain.len() <= rule_domain.len() {
        return false;
    }
    let prefix_len = domain.len() - rule_domain.len() - 1;
    domain.ends_with(rule_domain) && domain.as_bytes()[prefix_len] == b'.'
}

fn matches_subdomain(domain: &str, set: &HashSet<String>) -> bool {
    if set.contains(domain) {
        return true;
    }
    let mut rest = domain;
    while let Some(idx) = rest.find('.') {
        let parent = &rest[idx + 1..];
        if set.contains(parent) {
            return true;
        }
        rest = parent;
    }
    false
}

/// Canonical form of a domain: lowercase, no trailing dot.
///
/// Borrows when the input is already canonical — the common case on the
/// query hot path, where the pipeline hands down a name it has already
/// lowercased and dot-trimmed, so a query crossing several filter sources
/// no longer allocates a fresh copy per source. Only a name that actually
/// needs case folding pays for an owned `String`.
///
/// Callers that must own the result (a map key, a cache key) should use
/// [`normalize_owned`] instead: `normalize(s).into_owned()` scans to decide
/// whether it can borrow and *then* copies, two passes over the string, which
/// is pure overhead when the answer is always "own it".
pub(crate) fn normalize(s: &str) -> Cow<'_, str> {
    let trimmed = s.trim_end_matches('.');
    // Scan once to the first byte that needs folding. None → the input is
    // already canonical and we hand it back borrowed. Some(i) → copy the
    // bytes and lowercase only from `i` on; the prefix is already lowercase,
    // so it needs no second pass. ASCII bytes are always char boundaries,
    // so `i` is a valid split point.
    match trimmed.bytes().position(|b| b.is_ascii_uppercase()) {
        None => Cow::Borrowed(trimmed),
        Some(i) => {
            let mut owned = trimmed.to_owned();
            owned[i..].make_ascii_lowercase();
            Cow::Owned(owned)
        }
    }
}

/// Owned canonical form, in a single fused allocate-and-lowercase pass. For
/// callers that always need to own the result — building the filter's domain
/// sets, the cache's key — where borrowing can't help and the [`normalize`]
/// Cow's scan-then-copy would only add a redundant pass. Produces the exact
/// same string as `normalize(s).into_owned()`.
pub(crate) fn normalize_owned(s: &str) -> String {
    s.trim_end_matches('.').to_ascii_lowercase()
}
