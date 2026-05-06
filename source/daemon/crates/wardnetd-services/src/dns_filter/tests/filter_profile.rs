//! `RuntimeDnsFilterProfile::check` aggregates outcomes across the per-source
//! filters that compose the profile. Tests pin the precedence rules: rewrite
//! short-circuits, important allow beats important block, and an empty
//! profile decides nothing.

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use arc_swap::ArcSwap;
use hickory_proto::rr::RecordType;
use uuid::Uuid;

use crate::dns_filter::filter::{DnsFilter, DnsFilterInputs, MatchKind};
use crate::dns_filter::filter_profile::{ProfileMatch, RuntimeDnsFilterProfile};

fn localhost_v4() -> IpAddr {
    IpAddr::V4(Ipv4Addr::LOCALHOST)
}

fn arc_filter(inputs: DnsFilterInputs) -> Arc<ArcSwap<DnsFilter>> {
    Arc::new(ArcSwap::from_pointee(DnsFilter::build(inputs)))
}

fn profile(filters: Vec<Arc<ArcSwap<DnsFilter>>>) -> RuntimeDnsFilterProfile {
    RuntimeDnsFilterProfile::new(Uuid::new_v4(), "test".into(), filters)
}

#[test]
fn empty_profile_returns_none() {
    let p = profile(vec![]);
    assert_eq!(
        p.check("example.com", RecordType::A, localhost_v4()),
        ProfileMatch::None
    );
}

#[test]
fn single_filter_block_decides() {
    let p = profile(vec![arc_filter(DnsFilterInputs {
        blocked_domains: vec!["ads.com".into()],
        ..Default::default()
    })]);
    assert_eq!(
        p.check("ads.com", RecordType::A, localhost_v4()),
        ProfileMatch::Decided(MatchKind::Block)
    );
}

#[test]
fn allow_in_one_filter_overrides_block_in_another() {
    // Profile composes a blocklist + an allowlist; allow beats block.
    let p = profile(vec![
        arc_filter(DnsFilterInputs {
            blocked_domains: vec!["example.com".into()],
            ..Default::default()
        }),
        arc_filter(DnsFilterInputs {
            allowlist: vec!["safe.example.com".into()],
            ..Default::default()
        }),
    ]);
    assert_eq!(
        p.check("safe.example.com", RecordType::A, localhost_v4()),
        ProfileMatch::Decided(MatchKind::Allow)
    );
}

#[test]
fn rewrite_short_circuits_aggregation() {
    // Even if a downstream filter would block, the rewrite from the first
    // filter wins immediately.
    let p = profile(vec![
        arc_filter(DnsFilterInputs {
            custom_rules: vec!["||internal.corp^$dnsrewrite=10.0.0.1".into()],
            ..Default::default()
        }),
        arc_filter(DnsFilterInputs {
            blocked_domains: vec!["internal.corp".into()],
            ..Default::default()
        }),
    ]);
    let outcome = p.check("internal.corp", RecordType::A, localhost_v4());
    match outcome {
        ProfileMatch::Rewrite { ip } => assert_eq!(ip, IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))),
        other => panic!("expected Rewrite, got {other:?}"),
    }
}

#[test]
fn important_block_overrides_plain_allow() {
    let p = profile(vec![
        arc_filter(DnsFilterInputs {
            allowlist: vec!["ads.com".into()],
            ..Default::default()
        }),
        arc_filter(DnsFilterInputs {
            custom_rules: vec!["||ads.com^$important".into()],
            ..Default::default()
        }),
    ]);
    assert_eq!(
        p.check("ads.com", RecordType::A, localhost_v4()),
        ProfileMatch::Decided(MatchKind::ImportantBlock)
    );
}

#[test]
fn important_allow_beats_important_block() {
    let p = profile(vec![
        arc_filter(DnsFilterInputs {
            custom_rules: vec!["||ads.com^$important".into()],
            ..Default::default()
        }),
        arc_filter(DnsFilterInputs {
            custom_rules: vec!["@@||ads.com^$important".into()],
            ..Default::default()
        }),
    ]);
    assert_eq!(
        p.check("ads.com", RecordType::A, localhost_v4()),
        ProfileMatch::Decided(MatchKind::ImportantAllow)
    );
}

#[test]
fn unmatched_query_returns_none() {
    let p = profile(vec![arc_filter(DnsFilterInputs {
        blocked_domains: vec!["ads.com".into()],
        ..Default::default()
    })]);
    assert_eq!(
        p.check("safe.com", RecordType::A, localhost_v4()),
        ProfileMatch::None
    );
}

#[test]
fn arc_swap_replacement_is_visible_to_check() {
    // Verify that swapping a filter under a profile is observed without
    // recomposing the profile — this is the core hot-path assumption.
    let filter_handle = arc_filter(DnsFilterInputs::default());
    let p = profile(vec![filter_handle.clone()]);

    assert_eq!(
        p.check("ads.com", RecordType::A, localhost_v4()),
        ProfileMatch::None
    );

    filter_handle.store(Arc::new(DnsFilter::build(DnsFilterInputs {
        blocked_domains: vec!["ads.com".into()],
        ..Default::default()
    })));

    assert_eq!(
        p.check("ads.com", RecordType::A, localhost_v4()),
        ProfileMatch::Decided(MatchKind::Block)
    );
}
