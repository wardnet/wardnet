use uuid::Uuid;
use wardnet_common::routing_profile::DomainRoutingTarget;

use crate::routing_profile::view::{CompiledRule, DeviceRoutingContext};

fn tunnel(n: u128) -> DomainRoutingTarget {
    DomainRoutingTarget::Tunnel {
        tunnel_id: Uuid::from_u128(n),
    }
}

fn rule(pattern: &str, target: DomainRoutingTarget) -> CompiledRule {
    CompiledRule {
        pattern: pattern.to_owned(),
        target,
    }
}

#[test]
fn no_profiles_matches_nothing() {
    let ctx = DeviceRoutingContext::default();
    assert!(ctx.resolve("netflix.com").is_none());
}

#[test]
fn single_profile_matches_by_pattern() {
    let ctx = DeviceRoutingContext {
        profiles: vec![vec![rule("*.netflix.com", tunnel(1))]],
    };
    assert_eq!(ctx.resolve("www.netflix.com"), Some(&tunnel(1)));
    assert_eq!(ctx.resolve("netflix.com"), Some(&tunnel(1)));
    assert!(ctx.resolve("example.com").is_none());
}

#[test]
fn first_matching_profile_wins_over_specificity() {
    // Profile 0 (higher priority) has only a broad wildcard; profile 1 has an
    // exact rule. Profile order must win: the broad wildcard in profile 0
    // decides, even though profile 1's rule is more specific.
    let ctx = DeviceRoutingContext {
        profiles: vec![
            vec![rule("*.example.com", tunnel(1))],
            vec![rule("api.example.com", tunnel(2))],
        ],
    };
    assert_eq!(ctx.resolve("api.example.com"), Some(&tunnel(1)));
}

#[test]
fn within_a_profile_most_specific_wins() {
    let ctx = DeviceRoutingContext {
        profiles: vec![vec![
            rule("*.example.com", tunnel(1)),
            rule("api.example.com", tunnel(2)),
        ]],
    };
    assert_eq!(ctx.resolve("api.example.com"), Some(&tunnel(2)));
    assert_eq!(ctx.resolve("www.example.com"), Some(&tunnel(1)));
}

#[test]
fn direct_target_resolves() {
    let ctx = DeviceRoutingContext {
        profiles: vec![vec![rule("bank.example.com", DomainRoutingTarget::Direct)]],
    };
    assert_eq!(
        ctx.resolve("bank.example.com"),
        Some(&DomainRoutingTarget::Direct)
    );
}

#[test]
fn lower_priority_profile_used_when_higher_does_not_match() {
    let ctx = DeviceRoutingContext {
        profiles: vec![
            vec![rule("*.netflix.com", tunnel(1))],
            vec![rule("*.bbc.co.uk", tunnel(2))],
        ],
    };
    assert_eq!(ctx.resolve("www.bbc.co.uk"), Some(&tunnel(2)));
}
