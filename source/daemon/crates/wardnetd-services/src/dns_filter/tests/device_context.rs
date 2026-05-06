//! `DeviceFilterContext::check` aggregates across the device's profile set.
//! The kill switch (`enabled`) and the global emergency stop are honoured
//! one level above (in `DnsFilterServiceImpl::check`) — these tests cover
//! the per-device profile-aggregation half of that contract.

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;

use arc_swap::ArcSwap;
use hickory_proto::rr::RecordType;
use uuid::Uuid;
use wardnet_common::dns::FilterAction;

use crate::dns_filter::device_context::DeviceFilterContext;
use crate::dns_filter::filter::{DnsFilter, DnsFilterInputs};
use crate::dns_filter::filter_profile::RuntimeDnsFilterProfile;

fn localhost_v4() -> IpAddr {
    IpAddr::V4(Ipv4Addr::LOCALHOST)
}

fn arc_filter(inputs: DnsFilterInputs) -> Arc<ArcSwap<DnsFilter>> {
    Arc::new(ArcSwap::from_pointee(DnsFilter::build(inputs)))
}

fn arc_profile(
    name: &str,
    filters: Vec<Arc<ArcSwap<DnsFilter>>>,
) -> Arc<ArcSwap<RuntimeDnsFilterProfile>> {
    Arc::new(ArcSwap::from_pointee(RuntimeDnsFilterProfile::new(
        Uuid::new_v4(),
        name.to_string(),
        filters,
    )))
}

#[test]
fn unfiltered_context_passes_everything() {
    let ctx = DeviceFilterContext::unfiltered();
    assert!(ctx.enabled);
    assert!(ctx.profiles.is_empty());
    assert_eq!(
        ctx.check("ads.com", RecordType::A, localhost_v4()),
        FilterAction::Pass
    );
}

#[test]
fn single_profile_block_is_returned() {
    let ctx = DeviceFilterContext {
        enabled: true,
        profiles: vec![arc_profile(
            "p",
            vec![arc_filter(DnsFilterInputs {
                blocked_domains: vec!["ads.com".into()],
                ..Default::default()
            })],
        )],
    };
    assert_eq!(
        ctx.check("ads.com", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
}

#[test]
fn passes_when_no_profile_matches() {
    let ctx = DeviceFilterContext {
        enabled: true,
        profiles: vec![arc_profile(
            "p",
            vec![arc_filter(DnsFilterInputs {
                blocked_domains: vec!["ads.com".into()],
                ..Default::default()
            })],
        )],
    };
    assert_eq!(
        ctx.check("safe.com", RecordType::A, localhost_v4()),
        FilterAction::Pass
    );
}

#[test]
fn allow_in_one_profile_overrides_block_in_another() {
    // Two stacked profiles; the allow precedes block in MatchKind rank, so
    // the union of profiles passes the domain.
    let ctx = DeviceFilterContext {
        enabled: true,
        profiles: vec![
            arc_profile(
                "block",
                vec![arc_filter(DnsFilterInputs {
                    blocked_domains: vec!["example.com".into()],
                    ..Default::default()
                })],
            ),
            arc_profile(
                "allow",
                vec![arc_filter(DnsFilterInputs {
                    allowlist: vec!["safe.example.com".into()],
                    ..Default::default()
                })],
            ),
        ],
    };
    assert_eq!(
        ctx.check("safe.example.com", RecordType::A, localhost_v4()),
        FilterAction::Pass
    );
    // But a sibling subdomain that isn't on the allowlist still blocks.
    assert_eq!(
        ctx.check("ads.example.com", RecordType::A, localhost_v4()),
        FilterAction::Block
    );
}

#[test]
fn rewrite_short_circuits_at_profile_layer() {
    // First profile rewrites; second profile would block. Rewrite wins.
    let ctx = DeviceFilterContext {
        enabled: true,
        profiles: vec![
            arc_profile(
                "rewrite",
                vec![arc_filter(DnsFilterInputs {
                    custom_rules: vec!["||internal.corp^$dnsrewrite=192.0.2.1".into()],
                    ..Default::default()
                })],
            ),
            arc_profile(
                "block",
                vec![arc_filter(DnsFilterInputs {
                    blocked_domains: vec!["internal.corp".into()],
                    ..Default::default()
                })],
            ),
        ],
    };
    let action = ctx.check("internal.corp", RecordType::A, localhost_v4());
    match action {
        FilterAction::Rewrite { ip } => assert_eq!(ip, IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1))),
        other => panic!("expected Rewrite, got {other:?}"),
    }
}

#[test]
fn empty_context_with_enabled_false_still_passes_via_check() {
    // `check` doesn't read `enabled` — that gate is the service's
    // responsibility. With no profiles and any enabled value, the device
    // passes unconditionally because no rule matched.
    let ctx = DeviceFilterContext {
        enabled: false,
        profiles: vec![],
    };
    assert_eq!(
        ctx.check("ads.com", RecordType::A, localhost_v4()),
        FilterAction::Pass
    );
}
