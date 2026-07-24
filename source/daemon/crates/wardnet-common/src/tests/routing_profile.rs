use crate::routing_profile::*;
use uuid::Uuid;

#[test]
fn exact_pattern_matches_only_itself() {
    assert!(domain_matches("netflix.com", "netflix.com"));
    assert!(!domain_matches("netflix.com", "www.netflix.com"));
    assert!(!domain_matches("netflix.com", "notnetflix.com"));
}

#[test]
fn wildcard_matches_apex_and_subdomains() {
    assert!(domain_matches("*.netflix.com", "netflix.com"));
    assert!(domain_matches("*.netflix.com", "www.netflix.com"));
    assert!(domain_matches("*.netflix.com", "a.b.netflix.com"));
    assert!(!domain_matches("*.netflix.com", "netflix.com.evil.com"));
    assert!(!domain_matches("*.netflix.com", "faknetflix.com"));
}

#[test]
fn matching_is_case_and_trailing_dot_insensitive() {
    assert!(domain_matches("Netflix.COM", "netflix.com."));
    assert!(domain_matches("*.Example.com", "WWW.example.COM."));
}

#[test]
fn bare_wildcard_never_matches() {
    assert!(!domain_matches("*.", "anything.com"));
}

#[test]
fn specificity_prefers_exact_then_more_labels() {
    assert!(pattern_specificity("foo.example.com") > pattern_specificity("*.example.com"));
    assert!(pattern_specificity("*.example.com") > pattern_specificity("*.com"));
    // Exact beats wildcard on the same base.
    assert!(pattern_specificity("example.com") > pattern_specificity("*.example.com"));
}

#[test]
fn validate_pattern_accepts_reasonable_forms() {
    assert!(validate_pattern("netflix.com").is_ok());
    assert!(validate_pattern("*.netflix.com").is_ok());
    assert!(validate_pattern("a-b.example.co.uk").is_ok());
}

#[test]
fn validate_pattern_rejects_bad_forms() {
    assert!(validate_pattern("").is_err());
    assert!(validate_pattern("   ").is_err());
    assert!(validate_pattern("*.").is_err());
    assert!(validate_pattern("foo..bar").is_err());
    assert!(validate_pattern("foo/bar").is_err());
    assert!(validate_pattern("a.*.com").is_err());
}

#[test]
fn domain_target_storage_roundtrip() {
    let tunnel_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let tunnel = DomainRoutingTarget::Tunnel { tunnel_id };
    let (kind, id) = tunnel.to_storage();
    assert_eq!(kind, "tunnel");
    assert_eq!(DomainRoutingTarget::from_storage(kind, id), Some(tunnel));

    let (kind, id) = DomainRoutingTarget::Direct.to_storage();
    assert_eq!(kind, "direct");
    assert_eq!(id, None);
    assert_eq!(
        DomainRoutingTarget::from_storage(kind, id),
        Some(DomainRoutingTarget::Direct)
    );

    // A tunnel kind without an id is not reconstructable.
    assert_eq!(DomainRoutingTarget::from_storage("tunnel", None), None);
    assert_eq!(DomainRoutingTarget::from_storage("bogus", None), None);
}

#[test]
fn domain_target_serialises_tagged() {
    let json = serde_json::to_string(&DomainRoutingTarget::Direct).unwrap();
    assert_eq!(json, "{\"type\":\"direct\"}");
    let tunnel_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let json = serde_json::to_string(&DomainRoutingTarget::Tunnel { tunnel_id }).unwrap();
    assert!(json.contains("\"type\":\"tunnel\""));
    assert!(json.contains("\"tunnel_id\""));
}
