//! DNS filter engine.
//!
//! Takes blocklist domains, allowlist domains, and custom `AdGuard`-syntax rules
//! and answers queries with a [`FilterAction`] (Pass / Block / Rewrite).
//!
//! ## Two-tier evaluation
//!
//! - **Fast path**: plain domain blocks and allows live in `HashSet<String>`;
//!   subdomain match via parent walk. This handles the bulk of blocklist
//!   entries (hundreds of thousands).
//! - **Slow path**: any rule with modifiers, wildcards, or regex goes into
//!   `Vec<ParsedRule>` evaluated linearly.
//!
//! Evaluation order mirrors `AdGuard` precedence:
//!
//! 1. `$dnsrewrite` (highest — returns immediately)
//! 2. `$important` exception (`@@...$important`)
//! 3. `$important` block
//! 4. Plain exception (`@@...`, or allowlist fast-path)
//! 5. Plain block (rule or blocklist fast-path)
//! 6. Pass

use std::collections::HashSet;
use std::net::IpAddr;

use hickory_proto::rr::RecordType;
use wardnet_types::dns::FilterAction;

use super::filter_parser::{self, ParsedRule, RuleModifiers};

/// Inputs required to build a [`DnsFilter`]. Assembled by the service layer
/// from repository state.
#[derive(Debug, Clone, Default)]
pub struct FilterInputs {
    /// Deduplicated, lowercased domains from all enabled blocklists.
    pub blocked_domains: Vec<String>,
    /// Domains from the allowlist.
    pub allowlist: Vec<String>,
    /// Raw rule text lines from enabled custom filter rules.
    pub custom_rules: Vec<String>,
}

/// Aggregate counts, exposed via `/api/dns/status` in later stages.
#[derive(Debug, Clone, Copy, Default)]
pub struct FilterStats {
    pub blocked_count: usize,
    pub allowed_count: usize,
    pub complex_count: usize,
}

/// The filter engine.
///
/// Built from [`FilterInputs`]. Swapped atomically behind an `Arc<RwLock<…>>`
/// on reload (see `DnsRunner`).
#[derive(Debug, Default)]
pub struct DnsFilter {
    blocked_domains: HashSet<String>,
    allowed_domains: HashSet<String>,
    complex: Vec<ParsedRule>,
}

impl DnsFilter {
    /// Build a filter from the given inputs.
    ///
    /// Invalid custom rules are logged at `warn` and skipped — a single bad
    /// rule must never take down filtering for the rest.
    #[must_use]
    pub fn build(inputs: FilterInputs) -> Self {
        let mut blocked_domains: HashSet<String> = inputs
            .blocked_domains
            .into_iter()
            .map(|d| normalize(&d))
            .collect();
        let mut allowed_domains: HashSet<String> = inputs
            .allowlist
            .into_iter()
            .map(|d| normalize(&d))
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

    /// Build an empty filter — used at bootstrap before the first rebuild.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Whether the filter has no rules at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.blocked_domains.is_empty()
            && self.allowed_domains.is_empty()
            && self.complex.is_empty()
    }

    /// Return aggregate counts.
    #[must_use]
    pub fn stats(&self) -> FilterStats {
        FilterStats {
            blocked_count: self.blocked_domains.len(),
            allowed_count: self.allowed_domains.len(),
            complex_count: self.complex.len(),
        }
    }

    /// Evaluate the filter for a single query.
    #[must_use]
    pub fn check(&self, domain: &str, qtype: RecordType, client: IpAddr) -> FilterAction {
        let d = normalize(domain);
        let mut best: Option<MatchKind> = None;

        for rule in &self.complex {
            if !rule_matches(rule, &d) {
                continue;
            }
            if !modifiers_apply(rule.modifiers(), qtype, client) {
                continue;
            }

            // $dnsrewrite wins immediately.
            if let Some(ip) = rule.modifiers().rewrite_to {
                return FilterAction::Rewrite { ip };
            }

            let kind = classify(rule);
            upgrade(&mut best, kind);
        }

        // Fast-path matches: always "plain" (no modifiers), so they map to
        // MatchKind::Allow / MatchKind::Block.
        if matches_subdomain(&d, &self.allowed_domains) {
            upgrade(&mut best, MatchKind::Allow);
        }
        if matches_subdomain(&d, &self.blocked_domains) {
            upgrade(&mut best, MatchKind::Block);
        }

        match best {
            Some(k) if k.blocks() => FilterAction::Block,
            _ => FilterAction::Pass,
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MatchKind {
    Block,
    Allow,
    ImportantBlock,
    ImportantAllow,
}

impl MatchKind {
    fn rank(self) -> u8 {
        match self {
            Self::Block => 1,
            Self::Allow => 2,
            Self::ImportantBlock => 3,
            Self::ImportantAllow => 4,
        }
    }

    fn blocks(self) -> bool {
        matches!(self, Self::Block | Self::ImportantBlock)
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

/// Returns true when `domain` is `rule_domain` itself or a subdomain of it.
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

/// Walk a domain and its parents, checking each against the set.
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

fn normalize(s: &str) -> String {
    s.trim_end_matches('.').to_ascii_lowercase()
}
