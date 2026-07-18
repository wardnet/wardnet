//! Unit tests for the DNS query-stream WebSocket filter
//! (`GET /api/dns/log/stream`).

use wardnet_common::api::QueryLogEvent;
use wardnet_common::dns::DnsQueryResult;

use crate::api::dns_log_ws::ClientFilter;

fn event(domain: &str, ip: &str, result: &str) -> QueryLogEvent {
    QueryLogEvent {
        timestamp: "2026-05-05T00:00:00Z".to_owned(),
        client_ip: ip.to_owned(),
        domain: domain.to_owned(),
        query_type: "A".to_owned(),
        result: DnsQueryResult::parse(result),
        upstream: None,
        latency_ms: 0.0,
        device_id: None,
    }
}

#[test]
fn empty_filter_matches_anything() {
    let f = ClientFilter::default();
    assert!(f.matches(&event("example.com", "10.0.0.1", "forwarded")));
}

#[test]
fn domain_filter_uses_substring() {
    let f = ClientFilter {
        domain: "ads".to_owned(),
        ..Default::default()
    };
    assert!(f.matches(&event("ads.tracker.io", "10.0.0.1", "blocked")));
    assert!(!f.matches(&event("example.com", "10.0.0.1", "forwarded")));
}

#[test]
fn client_ip_filter_is_exact() {
    let f = ClientFilter {
        client_ip: "10.0.0.5".to_owned(),
        ..Default::default()
    };
    assert!(f.matches(&event("a.com", "10.0.0.5", "forwarded")));
    assert!(!f.matches(&event("a.com", "10.0.0.6", "forwarded")));
}

#[test]
fn results_filter_is_any_of() {
    let f = ClientFilter {
        results: vec!["blocked".to_owned(), "rewritten".to_owned()],
        ..Default::default()
    };
    assert!(f.matches(&event("a.com", "10.0.0.5", "blocked")));
    assert!(f.matches(&event("a.com", "10.0.0.5", "rewritten")));
    assert!(!f.matches(&event("a.com", "10.0.0.5", "forwarded")));
}

#[test]
fn all_filters_combine_with_and() {
    let f = ClientFilter {
        domain: "tracker".to_owned(),
        client_ip: "10.0.0.5".to_owned(),
        results: vec!["blocked".to_owned()],
    };
    assert!(f.matches(&event("ads.tracker.io", "10.0.0.5", "blocked")));
    // Domain miss
    assert!(!f.matches(&event("ok.com", "10.0.0.5", "blocked")));
    // IP miss
    assert!(!f.matches(&event("ads.tracker.io", "10.0.0.6", "blocked")));
    // Result miss
    assert!(!f.matches(&event("ads.tracker.io", "10.0.0.5", "forwarded")));
}

#[test]
fn apply_command_json_set_filter_updates_fields() {
    let mut f = ClientFilter::default();
    let ok = f.apply_command_json(
        r#"{"type":"set_filter","domain":"ads","client_ip":"10.0.0.5","results":["blocked"]}"#,
    );
    assert!(ok);
    assert_eq!(f.domain, "ads");
    assert_eq!(f.client_ip, "10.0.0.5");
    assert_eq!(f.results, vec!["blocked".to_owned()]);
}

#[test]
fn apply_command_json_partial_only_overwrites_some_fields() {
    let mut f = ClientFilter {
        domain: "existing".to_owned(),
        client_ip: "10.0.0.5".to_owned(),
        ..Default::default()
    };
    let ok = f.apply_command_json(r#"{"type":"set_filter","domain":"new"}"#);
    assert!(ok);
    assert_eq!(f.domain, "new");
    // Untouched.
    assert_eq!(f.client_ip, "10.0.0.5");
}

#[test]
fn apply_command_json_invalid_json_is_ignored() {
    let mut f = ClientFilter::default();
    let ok = f.apply_command_json("not json");
    assert!(!ok);
}

#[test]
fn apply_command_json_unknown_command_is_ignored() {
    let mut f = ClientFilter::default();
    // The `Unknown` variant matches any other tag; this exercises
    // the early-return in `apply_command_json`.
    let ok = f.apply_command_json(r#"{"type":"something_else"}"#);
    assert!(!ok);
}
