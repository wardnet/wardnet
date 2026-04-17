use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use hickory_proto::rr::RecordType;
use wardnet_common::dns::FilterAction;

use crate::dns::filter::{DnsFilter, FilterInputs};

fn localhost_v4() -> IpAddr {
    IpAddr::V4(Ipv4Addr::LOCALHOST)
}

fn build(inputs: FilterInputs) -> DnsFilter {
    DnsFilter::build(inputs)
}

// ---------- Empty filter ----------

#[test]
fn empty_filter_passes_everything() {
    let f = build(FilterInputs::default());
    assert!(f.is_empty());
    assert_eq!(
        f.check("example.com", RecordType::A, localhost_v4()),
        FilterAction::Pass
    );
}

// ---------- Fast-path domain blocking ----------

#[test]
fn exact_domain_blocked() {
    let f = build(FilterInputs {
        blocked_domains: vec!["ads.example.com".into()],
        ..Default::default()
    });
    assert_eq!(
        f.check("ads.example.com", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
}

#[test]
fn subdomain_of_blocked_domain_is_blocked() {
    let f = build(FilterInputs {
        blocked_domains: vec!["example.com".into()],
        ..Default::default()
    });
    assert_eq!(
        f.check("sub.example.com", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
    assert_eq!(
        f.check("deep.sub.example.com", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
}

#[test]
fn unrelated_domain_passes() {
    let f = build(FilterInputs {
        blocked_domains: vec!["ads.example.com".into()],
        ..Default::default()
    });
    assert_eq!(
        f.check("safe.example.org", RecordType::A, localhost_v4()),
        FilterAction::Pass
    );
}

#[test]
fn case_insensitive_matching() {
    let f = build(FilterInputs {
        blocked_domains: vec!["ads.example.com".into()],
        ..Default::default()
    });
    assert_eq!(
        f.check("ADS.EXAMPLE.COM", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
}

#[test]
fn trailing_dot_stripped() {
    let f = build(FilterInputs {
        blocked_domains: vec!["ads.example.com".into()],
        ..Default::default()
    });
    assert_eq!(
        f.check("ads.example.com.", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
}

// ---------- Fast-path allowlist ----------

#[test]
fn allowlist_overrides_blocklist() {
    let f = build(FilterInputs {
        blocked_domains: vec!["example.com".into()],
        allowlist: vec!["safe.example.com".into()],
        ..Default::default()
    });
    // The parent domain is blocked.
    assert_eq!(
        f.check("ads.example.com", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
    // But the explicitly allowed subdomain passes.
    assert_eq!(
        f.check("safe.example.com", RecordType::A, localhost_v4()),
        FilterAction::Pass
    );
    // And children of the allowed domain also pass.
    assert_eq!(
        f.check("cdn.safe.example.com", RecordType::A, localhost_v4()),
        FilterAction::Pass
    );
}

// ---------- Complex rules (custom rules) ----------

#[test]
fn regex_rule_blocks() {
    let f = build(FilterInputs {
        custom_rules: vec![r"/tracker\d+\.example\.com/".into()],
        ..Default::default()
    });
    assert_eq!(
        f.check("tracker1.example.com", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
    assert_eq!(
        f.check("tracker42.example.com", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
    assert_eq!(
        f.check("example.com", RecordType::A, localhost_v4()),
        FilterAction::Pass
    );
}

#[test]
fn wildcard_pattern_blocks() {
    let f = build(FilterInputs {
        custom_rules: vec!["||analytics.*^".into()],
        ..Default::default()
    });
    assert_eq!(
        f.check("analytics.com", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
    assert_eq!(
        f.check("analytics.co.uk", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
    assert_eq!(
        f.check("tracking.com", RecordType::A, localhost_v4()),
        FilterAction::Pass
    );
}

#[test]
fn dnstype_modifier_filters_by_query_type() {
    let f = build(FilterInputs {
        custom_rules: vec!["||analytics.example.com^$dnstype=AAAA".into()],
        ..Default::default()
    });
    // AAAA queries are blocked.
    assert_eq!(
        f.check("analytics.example.com", RecordType::AAAA, localhost_v4()),
        FilterAction::Block
    );
    // A queries pass (modifier doesn't match).
    assert_eq!(
        f.check("analytics.example.com", RecordType::A, localhost_v4()),
        FilterAction::Pass
    );
}

#[test]
fn dnsrewrite_returns_rewrite_action() {
    let f = build(FilterInputs {
        custom_rules: vec!["||internal.corp^$dnsrewrite=192.168.1.50".into()],
        ..Default::default()
    });
    let action = f.check("internal.corp", RecordType::A, localhost_v4());
    assert_eq!(
        action,
        FilterAction::Rewrite {
            ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50))
        }
    );
}

#[test]
fn dnsrewrite_overrides_blocklist() {
    let f = build(FilterInputs {
        blocked_domains: vec!["internal.corp".into()],
        custom_rules: vec!["||internal.corp^$dnsrewrite=10.0.0.1".into()],
        ..Default::default()
    });
    let action = f.check("internal.corp", RecordType::A, localhost_v4());
    assert_eq!(
        action,
        FilterAction::Rewrite {
            ip: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))
        }
    );
}

#[test]
fn client_modifier_restricts_by_ip() {
    let f = build(FilterInputs {
        custom_rules: vec!["||ads.com^$client=192.168.1.0/24".into()],
        ..Default::default()
    });
    // Client in range → blocked.
    assert_eq!(
        f.check(
            "ads.com",
            RecordType::A,
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 42))
        ),
        FilterAction::Block
    );
    // Client outside range → passes.
    assert_eq!(
        f.check(
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
    let f = build(FilterInputs {
        allowlist: vec!["ads.com".into()],
        custom_rules: vec!["||ads.com^$important".into()],
        ..Default::default()
    });
    assert_eq!(
        f.check("ads.com", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
}

#[test]
fn important_exception_overrides_important_block() {
    let f = build(FilterInputs {
        custom_rules: vec![
            "||ads.com^$important".into(),
            "@@||safe.ads.com^$important".into(),
        ],
        ..Default::default()
    });
    // ads.com still blocked.
    assert_eq!(
        f.check("ads.com", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
    // But the important exception for safe.ads.com wins.
    assert_eq!(
        f.check("safe.ads.com", RecordType::A, localhost_v4()),
        FilterAction::Pass
    );
}

#[test]
fn plain_exception_overrides_plain_block() {
    let f = build(FilterInputs {
        custom_rules: vec!["||example.com^".into(), "@@||safe.example.com^".into()],
        ..Default::default()
    });
    assert_eq!(
        f.check("ads.example.com", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
    assert_eq!(
        f.check("safe.example.com", RecordType::A, localhost_v4()),
        FilterAction::Pass
    );
}

// ---------- Mixed inputs ----------

#[test]
fn mixed_blocklist_and_custom_rules() {
    let f = build(FilterInputs {
        blocked_domains: (0..1000).map(|i| format!("ads{i}.example.com")).collect(),
        allowlist: vec!["safe.example.com".into()],
        custom_rules: vec!["||analytics.*^".into(), "@@||allowed-analytics.com^".into()],
    });

    // Blocklist block.
    assert_eq!(
        f.check("ads42.example.com", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
    // Allowlist pass.
    assert_eq!(
        f.check("safe.example.com", RecordType::A, localhost_v4()),
        FilterAction::Pass
    );
    // Wildcard block.
    assert_eq!(
        f.check("analytics.co.uk", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
    // Wildcard exception pass.
    assert_eq!(
        f.check("allowed-analytics.com", RecordType::A, localhost_v4()),
        FilterAction::Pass
    );
    // Unblocked domain.
    assert_eq!(
        f.check("google.com", RecordType::A, localhost_v4()),
        FilterAction::Pass
    );
}

#[test]
fn stats_are_accurate() {
    let f = build(FilterInputs {
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
    let f = build(FilterInputs {
        blocked_domains: vec!["ads.com".into()],
        ..Default::default()
    });
    assert!(!f.is_empty());
}

#[test]
fn normalize_handles_trailing_dot_in_input() {
    let f = build(FilterInputs {
        blocked_domains: vec!["ads.example.com.".into()],
        ..Default::default()
    });
    assert_eq!(
        f.check("ads.example.com", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
}

#[test]
fn invalid_custom_rule_is_skipped() {
    // A rule that fails to parse should be silently skipped.
    let f = build(FilterInputs {
        custom_rules: vec!["notadomain".into(), "||valid.com^".into()],
        ..Default::default()
    });
    // The invalid rule should be silently skipped; the valid one should work.
    assert_eq!(
        f.check("valid.com", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
}

#[test]
fn dnsrewrite_ipv6_returns_rewrite() {
    let f = build(FilterInputs {
        custom_rules: vec!["||ipv6.corp^$dnsrewrite=fe80::1".into()],
        ..Default::default()
    });
    let action = f.check("ipv6.corp", RecordType::AAAA, localhost_v4());
    assert_eq!(
        action,
        FilterAction::Rewrite {
            ip: IpAddr::V6("fe80::1".parse().unwrap())
        }
    );
}

#[test]
fn regex_exception_passes() {
    let f = build(FilterInputs {
        custom_rules: vec![
            r"/tracker\d+\.example\.com/".into(),
            r"@@/tracker1\.example\.com/".into(),
        ],
        ..Default::default()
    });
    // tracker1 is excepted.
    assert_eq!(
        f.check("tracker1.example.com", RecordType::A, localhost_v4()),
        FilterAction::Pass
    );
    // tracker2 is still blocked.
    assert_eq!(
        f.check("tracker2.example.com", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
}

// ---------- IPv6 ----------

#[test]
fn ipv6_client_matching() {
    let f = build(FilterInputs {
        custom_rules: vec!["||ads.com^$client=fd00::/8".into()],
        ..Default::default()
    });
    assert_eq!(
        f.check(
            "ads.com",
            RecordType::A,
            IpAddr::V6(Ipv6Addr::new(0xfd00, 0, 0, 0, 0, 0, 0, 1))
        ),
        FilterAction::Block
    );
    assert_eq!(
        f.check(
            "ads.com",
            RecordType::A,
            IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1))
        ),
        FilterAction::Pass
    );
}
