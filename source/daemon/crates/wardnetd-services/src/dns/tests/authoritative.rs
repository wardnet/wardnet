use chrono::Utc;
use uuid::Uuid;
use wardnet_common::dns::{
    ConditionalForwardingRule, CustomDnsRecord, DnsRecordSource, DnsRecordType, DnsZone,
};

use crate::dns::authoritative::{AuthoritativeView, build_soa, parse_conditional_upstream};

fn zone(name: &str, enabled: bool) -> DnsZone {
    DnsZone {
        id: Uuid::new_v4(),
        name: name.to_owned(),
        enabled,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn record(
    domain: &str,
    rtype: DnsRecordType,
    value: &str,
    zone_id: Option<Uuid>,
    enabled: bool,
) -> CustomDnsRecord {
    CustomDnsRecord {
        id: Uuid::new_v4(),
        zone_id,
        domain: domain.to_owned(),
        record_type: rtype,
        value: value.to_owned(),
        ttl: 60,
        enabled,
        source: DnsRecordSource::Manual,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

fn rule(domain: &str, upstream: &str, enabled: bool) -> ConditionalForwardingRule {
    ConditionalForwardingRule {
        id: Uuid::new_v4(),
        domain: domain.to_owned(),
        upstream: upstream.to_owned(),
        enabled,
        created_at: Utc::now(),
    }
}

#[test]
fn build_filters_disabled_records() {
    let view = AuthoritativeView::build(
        &[],
        vec![record("host.lan", DnsRecordType::A, "1.2.3.4", None, false)],
        vec![],
    );
    assert!(view.lookup_all("host.lan").is_none());
}

#[test]
fn build_filters_records_in_disabled_zone() {
    let z = zone("lan", false);
    let r = record("host.lan", DnsRecordType::A, "1.2.3.4", Some(z.id), true);
    let view = AuthoritativeView::build(&[z], vec![r], vec![]);
    assert!(view.lookup_all("host.lan").is_none());
}

#[test]
fn build_includes_unzoned_records() {
    let r = record("host.lan", DnsRecordType::A, "1.2.3.4", None, true);
    let view = AuthoritativeView::build(&[], vec![r], vec![]);
    assert!(view.lookup_all("host.lan").is_some());
}

#[test]
fn build_includes_records_in_enabled_zone() {
    let z = zone("lan", true);
    let r = record("host.lan", DnsRecordType::A, "1.2.3.4", Some(z.id), true);
    let view = AuthoritativeView::build(&[z], vec![r], vec![]);
    let found = view.lookup_all("host.lan").unwrap();
    assert_eq!(found.len(), 1);
}

#[test]
fn lookup_exact_type_match() {
    let r = record("host.lan", DnsRecordType::A, "1.2.3.4", None, true);
    let view = AuthoritativeView::build(&[], vec![r], vec![]);
    let found = view.lookup("host.lan", DnsRecordType::A).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].value, "1.2.3.4");
}

#[test]
fn lookup_returns_empty_slice_for_known_domain_unknown_type() {
    let r = record("host.lan", DnsRecordType::A, "1.2.3.4", None, true);
    let view = AuthoritativeView::build(&[], vec![r], vec![]);
    // Domain is known (has an A) but no AAAA record → Some([])
    let found = view.lookup("host.lan", DnsRecordType::Aaaa);
    assert!(found.is_some());
    assert!(found.unwrap().is_empty());
}

#[test]
fn lookup_returns_none_for_unknown_domain() {
    let view = AuthoritativeView::empty();
    assert!(view.lookup("unknown.lan", DnsRecordType::A).is_none());
}

#[test]
fn lookup_all_returns_all_types() {
    let r1 = record("host.lan", DnsRecordType::A, "1.2.3.4", None, true);
    let r2 = record("host.lan", DnsRecordType::Aaaa, "::1", None, true);
    let view = AuthoritativeView::build(&[], vec![r1, r2], vec![]);
    let found = view.lookup_all("host.lan").unwrap();
    assert_eq!(found.len(), 2);
}

#[test]
fn match_forwarding_rule_exact() {
    let r = rule("corp.internal", "10.0.0.1:53", true);
    let view = AuthoritativeView::build(&[], vec![], vec![r]);
    let matched = view.match_forwarding_rule("corp.internal");
    assert!(matched.is_some());
    assert_eq!(matched.unwrap().upstream, "10.0.0.1:53");
}

#[test]
fn match_forwarding_rule_suffix() {
    let r = rule("corp.internal", "10.0.0.1:53", true);
    let view = AuthoritativeView::build(&[], vec![], vec![r]);
    let matched = view.match_forwarding_rule("host.corp.internal");
    assert!(matched.is_some());
}

#[test]
fn match_forwarding_rule_longest_wins() {
    let short = rule("internal", "1.1.1.1", true);
    let long = rule("corp.internal", "2.2.2.2", true);
    let view = AuthoritativeView::build(&[], vec![], vec![short, long]);
    let matched = view.match_forwarding_rule("host.corp.internal").unwrap();
    assert_eq!(matched.upstream, "2.2.2.2");
}

#[test]
fn match_forwarding_rule_no_match() {
    let r = rule("corp.internal", "10.0.0.1", true);
    let view = AuthoritativeView::build(&[], vec![], vec![r]);
    assert!(view.match_forwarding_rule("other.domain").is_none());
}

#[test]
fn match_forwarding_rule_ignores_disabled() {
    let r = rule("corp.internal", "10.0.0.1", false);
    let view = AuthoritativeView::build(&[], vec![], vec![r]);
    assert!(view.match_forwarding_rule("corp.internal").is_none());
}

#[test]
fn lookup_cname() {
    let r = record("alias.lan", DnsRecordType::Cname, "target.lan", None, true);
    let view = AuthoritativeView::build(&[], vec![r], vec![]);
    let found = view.lookup_cname("alias.lan");
    assert!(found.is_some());
    assert_eq!(found.unwrap().value, "target.lan");
}

#[test]
fn parse_conditional_upstream_bare_ipv4() {
    let addr = parse_conditional_upstream("1.2.3.4").unwrap();
    assert_eq!(addr.port(), 53);
    assert_eq!(addr.ip().to_string(), "1.2.3.4");
}

#[test]
fn parse_conditional_upstream_ipv4_with_port() {
    let addr = parse_conditional_upstream("1.2.3.4:5353").unwrap();
    assert_eq!(addr.port(), 5353);
}

#[test]
fn parse_conditional_upstream_ipv6_bare() {
    let addr = parse_conditional_upstream("::1").unwrap();
    assert_eq!(addr.port(), 53);
}

// ---------------------------------------------------------------------------
// Zone authority — enabled zones claim their suffix so unknown names under
// them are answered authoritatively (NXDOMAIN) instead of forwarded.
// ---------------------------------------------------------------------------

#[test]
fn authoritative_zone_exact_match() {
    let view = AuthoritativeView::build(&[zone("lan", true)], vec![], vec![]);
    let z = view
        .authoritative_zone("lan")
        .expect("zone apex is authoritative");
    assert_eq!(z.name, "lan");
}

#[test]
fn authoritative_zone_suffix_match() {
    let view = AuthoritativeView::build(&[zone("lan", true)], vec![], vec![]);
    assert!(view.authoritative_zone("host.lan").is_some());
    assert!(view.authoritative_zone("a.b.lan").is_some());
}

#[test]
fn authoritative_zone_unrelated_name_not_matched() {
    let view = AuthoritativeView::build(&[zone("lan", true)], vec![], vec![]);
    assert!(view.authoritative_zone("host.home").is_none());
}

#[test]
fn authoritative_zone_partial_label_not_matched() {
    // "wlan" must NOT match zone "lan" — the suffix has to follow a dot.
    let view = AuthoritativeView::build(&[zone("lan", true)], vec![], vec![]);
    assert!(view.authoritative_zone("wlan").is_none());
}

#[test]
fn authoritative_zone_ignores_disabled_zone() {
    let view = AuthoritativeView::build(&[zone("lan", false)], vec![], vec![]);
    assert!(view.authoritative_zone("host.lan").is_none());
}

#[test]
fn authoritative_zone_longest_suffix_wins() {
    let view =
        AuthoritativeView::build(&[zone("lan", true), zone("dev.lan", true)], vec![], vec![]);
    let z = view
        .authoritative_zone("box.dev.lan")
        .expect("nested zone is authoritative");
    assert_eq!(z.name, "dev.lan");
}

#[test]
fn authoritative_zone_serial_from_updated_at() {
    let mut z = zone("lan", true);
    z.updated_at = chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap();
    let view = AuthoritativeView::build(&[z], vec![], vec![]);
    assert_eq!(
        view.authoritative_zone("lan").unwrap().serial,
        1_700_000_000
    );
}

#[test]
fn build_soa_record_shape() {
    use hickory_proto::rr::{RData, RecordType};

    let rec = build_soa("lan", 42).expect("valid zone name builds an SOA");
    assert_eq!(rec.record_type(), RecordType::SOA);
    assert_eq!(rec.name.to_string(), "lan.", "SOA owner is the zone apex");
    assert_eq!(rec.ttl, 300, "SOA TTL doubles as the negative-cache TTL");
    assert!(
        matches!(rec.data, RData::SOA(_)),
        "authority record must carry SOA rdata"
    );
}
