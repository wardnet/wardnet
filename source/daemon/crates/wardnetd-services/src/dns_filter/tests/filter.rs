//! Per-source `DnsFilter` tests. Ported from the pre-#221 `dns/tests/filter.rs`
//! file the migration deleted; assertions use `check_action()` (the legacy
//! `FilterAction` shim) so the matrix is identical to the old behaviour.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use hickory_proto::rr::RecordType;
use wardnet_common::dns::FilterAction;

use crate::dns_filter::filter::{DnsFilter, DnsFilterInputs, MatchKind, MatchOutcome};

fn localhost_v4() -> IpAddr {
    IpAddr::V4(Ipv4Addr::LOCALHOST)
}

fn build(inputs: DnsFilterInputs) -> DnsFilter {
    DnsFilter::build(inputs)
}

// ---------- Empty filter ----------

#[test]
fn empty_filter_passes_everything() {
    let f = build(DnsFilterInputs::default());
    assert!(f.is_empty());
    assert_eq!(
        f.check_action("example.com", RecordType::A, localhost_v4()),
        FilterAction::Pass
    );
    // Default-constructed filter behaves the same as empty inputs.
    let g = DnsFilter::empty();
    assert!(g.is_empty());
    assert_eq!(
        g.check("example.com", RecordType::A, localhost_v4()),
        MatchOutcome::None
    );
}

// ---------- Fast-path domain blocking ----------

#[test]
fn exact_domain_blocked() {
    let f = build(DnsFilterInputs {
        blocked_domains: vec!["ads.example.com".into()],
        ..Default::default()
    });
    assert_eq!(
        f.check_action("ads.example.com", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
}

#[test]
fn subdomain_of_blocked_domain_is_blocked() {
    let f = build(DnsFilterInputs {
        blocked_domains: vec!["example.com".into()],
        ..Default::default()
    });
    assert_eq!(
        f.check_action("sub.example.com", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
    assert_eq!(
        f.check_action("deep.sub.example.com", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
}

#[test]
fn unrelated_domain_passes() {
    let f = build(DnsFilterInputs {
        blocked_domains: vec!["ads.example.com".into()],
        ..Default::default()
    });
    assert_eq!(
        f.check_action("safe.example.org", RecordType::A, localhost_v4()),
        FilterAction::Pass
    );
}

#[test]
fn case_insensitive_matching() {
    let f = build(DnsFilterInputs {
        blocked_domains: vec!["ads.example.com".into()],
        ..Default::default()
    });
    assert_eq!(
        f.check_action("ADS.EXAMPLE.COM", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
}

#[test]
fn trailing_dot_stripped() {
    let f = build(DnsFilterInputs {
        blocked_domains: vec!["ads.example.com".into()],
        ..Default::default()
    });
    assert_eq!(
        f.check_action("ads.example.com.", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
}

// ---------- Allowlist precedence ----------

#[test]
fn allowlist_overrides_blocklist() {
    let f = build(DnsFilterInputs {
        blocked_domains: vec!["example.com".into()],
        allowlist: vec!["safe.example.com".into()],
        ..Default::default()
    });
    // The parent domain is blocked.
    assert_eq!(
        f.check_action("ads.example.com", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
    // But the explicitly allowed subdomain passes.
    assert_eq!(
        f.check_action("safe.example.com", RecordType::A, localhost_v4()),
        FilterAction::Pass
    );
    // And children of the allowed domain also pass.
    assert_eq!(
        f.check_action("cdn.safe.example.com", RecordType::A, localhost_v4()),
        FilterAction::Pass
    );
}

// ---------- Complex rules (custom rules) ----------

#[test]
fn regex_rule_blocks() {
    let f = build(DnsFilterInputs {
        custom_rules: vec![r"/tracker\d+\.example\.com/".into()],
        ..Default::default()
    });
    assert_eq!(
        f.check_action("tracker1.example.com", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
    assert_eq!(
        f.check_action("tracker42.example.com", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
    assert_eq!(
        f.check_action("example.com", RecordType::A, localhost_v4()),
        FilterAction::Pass
    );
}

#[test]
fn wildcard_pattern_blocks() {
    let f = build(DnsFilterInputs {
        custom_rules: vec!["||analytics.*^".into()],
        ..Default::default()
    });
    assert_eq!(
        f.check_action("analytics.com", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
    assert_eq!(
        f.check_action("analytics.co.uk", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
    assert_eq!(
        f.check_action("tracking.com", RecordType::A, localhost_v4()),
        FilterAction::Pass
    );
}

#[test]
fn dnstype_modifier_filters_by_query_type() {
    let f = build(DnsFilterInputs {
        custom_rules: vec!["||analytics.example.com^$dnstype=AAAA".into()],
        ..Default::default()
    });
    assert_eq!(
        f.check_action("analytics.example.com", RecordType::AAAA, localhost_v4()),
        FilterAction::Block
    );
    assert_eq!(
        f.check_action("analytics.example.com", RecordType::A, localhost_v4()),
        FilterAction::Pass
    );
}

#[test]
fn dnsrewrite_returns_rewrite_action() {
    let f = build(DnsFilterInputs {
        custom_rules: vec!["||internal.corp^$dnsrewrite=192.168.1.50".into()],
        ..Default::default()
    });
    let action = f.check_action("internal.corp", RecordType::A, localhost_v4());
    assert_eq!(
        action,
        FilterAction::Rewrite {
            ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50))
        }
    );
}

#[test]
fn dnsrewrite_overrides_blocklist() {
    let f = build(DnsFilterInputs {
        blocked_domains: vec!["internal.corp".into()],
        custom_rules: vec!["||internal.corp^$dnsrewrite=10.0.0.1".into()],
        ..Default::default()
    });
    let action = f.check_action("internal.corp", RecordType::A, localhost_v4());
    assert_eq!(
        action,
        FilterAction::Rewrite {
            ip: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))
        }
    );
}

#[test]
fn client_modifier_restricts_by_ip() {
    let f = build(DnsFilterInputs {
        custom_rules: vec!["||ads.com^$client=192.168.1.0/24".into()],
        ..Default::default()
    });
    assert_eq!(
        f.check_action(
            "ads.com",
            RecordType::A,
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 42))
        ),
        FilterAction::Block
    );
    assert_eq!(
        f.check_action(
            "ads.com",
            RecordType::A,
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))
        ),
        FilterAction::Pass
    );
}

// ---------- $important precedence ----------

#[test]
fn important_block_overrides_plain_allowlist() {
    let f = build(DnsFilterInputs {
        allowlist: vec!["ads.com".into()],
        custom_rules: vec!["||ads.com^$important".into()],
        ..Default::default()
    });
    assert_eq!(
        f.check_action("ads.com", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
}

#[test]
fn important_exception_overrides_important_block() {
    let f = build(DnsFilterInputs {
        custom_rules: vec![
            "||ads.com^$important".into(),
            "@@||safe.ads.com^$important".into(),
        ],
        ..Default::default()
    });
    assert_eq!(
        f.check_action("ads.com", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
    assert_eq!(
        f.check_action("safe.ads.com", RecordType::A, localhost_v4()),
        FilterAction::Pass
    );
}

#[test]
fn plain_exception_overrides_plain_block() {
    let f = build(DnsFilterInputs {
        custom_rules: vec!["||example.com^".into(), "@@||safe.example.com^".into()],
        ..Default::default()
    });
    assert_eq!(
        f.check_action("ads.example.com", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
    assert_eq!(
        f.check_action("safe.example.com", RecordType::A, localhost_v4()),
        FilterAction::Pass
    );
}

// ---------- Mixed inputs ----------

#[test]
fn mixed_blocklist_and_custom_rules() {
    let f = build(DnsFilterInputs {
        blocked_domains: (0..1000).map(|i| format!("ads{i}.example.com")).collect(),
        allowlist: vec!["safe.example.com".into()],
        custom_rules: vec!["||analytics.*^".into(), "@@||allowed-analytics.com^".into()],
    });

    assert_eq!(
        f.check_action("ads42.example.com", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
    assert_eq!(
        f.check_action("safe.example.com", RecordType::A, localhost_v4()),
        FilterAction::Pass
    );
    assert_eq!(
        f.check_action("analytics.co.uk", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
    assert_eq!(
        f.check_action("allowed-analytics.com", RecordType::A, localhost_v4()),
        FilterAction::Pass
    );
    assert_eq!(
        f.check_action("google.com", RecordType::A, localhost_v4()),
        FilterAction::Pass
    );
}

#[test]
fn stats_are_accurate() {
    let f = build(DnsFilterInputs {
        blocked_domains: vec!["a.com".into(), "b.com".into()],
        allowlist: vec!["c.com".into()],
        custom_rules: vec!["||x.*^$important".into()],
    });
    let s = f.stats();
    assert_eq!(s.blocked_count, 2);
    assert_eq!(s.allowed_count, 1);
    assert_eq!(s.complex_count, 1);
}

// ---------- Minor gap coverage ----------

#[test]
fn non_empty_filter_is_not_empty() {
    let f = build(DnsFilterInputs {
        blocked_domains: vec!["ads.com".into()],
        ..Default::default()
    });
    assert!(!f.is_empty());
}

#[test]
fn normalize_handles_trailing_dot_in_input() {
    let f = build(DnsFilterInputs {
        blocked_domains: vec!["ads.example.com.".into()],
        ..Default::default()
    });
    assert_eq!(
        f.check_action("ads.example.com", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
}

#[test]
fn invalid_custom_rule_is_skipped() {
    // A rule that fails to parse should be silently skipped.
    let f = build(DnsFilterInputs {
        custom_rules: vec!["notadomain".into(), "||valid.com^".into()],
        ..Default::default()
    });
    assert_eq!(
        f.check_action("valid.com", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
}

#[test]
fn dnsrewrite_ipv6_returns_rewrite() {
    let f = build(DnsFilterInputs {
        custom_rules: vec!["||ipv6.corp^$dnsrewrite=fe80::1".into()],
        ..Default::default()
    });
    let action = f.check_action("ipv6.corp", RecordType::AAAA, localhost_v4());
    assert_eq!(
        action,
        FilterAction::Rewrite {
            ip: IpAddr::V6("fe80::1".parse().unwrap())
        }
    );
}

#[test]
fn regex_exception_passes() {
    let f = build(DnsFilterInputs {
        custom_rules: vec![
            r"/tracker\d+\.example\.com/".into(),
            r"@@/tracker1\.example\.com/".into(),
        ],
        ..Default::default()
    });
    assert_eq!(
        f.check_action("tracker1.example.com", RecordType::A, localhost_v4()),
        FilterAction::Pass
    );
    assert_eq!(
        f.check_action("tracker2.example.com", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
}

// ---------- IPv6 ----------

#[test]
fn ipv6_client_matching() {
    let f = build(DnsFilterInputs {
        custom_rules: vec!["||ads.com^$client=fd00::/8".into()],
        ..Default::default()
    });
    assert_eq!(
        f.check_action(
            "ads.com",
            RecordType::A,
            IpAddr::V6(Ipv6Addr::new(0xfd00, 0, 0, 0, 0, 0, 0, 1))
        ),
        FilterAction::Block
    );
    assert_eq!(
        f.check_action(
            "ads.com",
            RecordType::A,
            IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1))
        ),
        FilterAction::Pass
    );
}

// ---------- MatchKind precedence ranks ----------

#[test]
fn match_kind_rank_ordering() {
    assert!(MatchKind::ImportantAllow.rank() > MatchKind::ImportantBlock.rank());
    assert!(MatchKind::ImportantBlock.rank() > MatchKind::Allow.rank());
    assert!(MatchKind::Allow.rank() > MatchKind::Block.rank());
}

#[test]
fn match_kind_blocks_classification() {
    assert!(MatchKind::Block.blocks());
    assert!(MatchKind::ImportantBlock.blocks());
    assert!(!MatchKind::Allow.blocks());
    assert!(!MatchKind::ImportantAllow.blocks());
}
