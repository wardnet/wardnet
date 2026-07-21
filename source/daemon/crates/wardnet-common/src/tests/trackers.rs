use crate::trackers::{company_count, company_for_domain, is_tracker};

#[test]
fn exact_registrable_domain_matches() {
    assert_eq!(company_for_domain("doubleclick.net"), Some("Google"));
    assert_eq!(company_for_domain("criteo.com"), Some("Criteo"));
    assert_eq!(
        company_for_domain("scorecardresearch.com"),
        Some("Comscore")
    );
}

#[test]
fn subdomains_resolve_to_the_parent_company() {
    // Ad-serving subdomains are the common case: they must attribute to the
    // registrable domain's owner, not fall through to `None`.
    assert_eq!(
        company_for_domain("pagead2.googlesyndication.com"),
        Some("Google")
    );
    assert_eq!(
        company_for_domain("stats.g.doubleclick.net"),
        Some("Google")
    );
    assert_eq!(company_for_domain("connect.facebook.net"), Some("Meta"));
    assert_eq!(company_for_domain("analytics.tiktok.com"), Some("TikTok"));
}

#[test]
fn matching_is_case_insensitive_and_ignores_trailing_dot() {
    // DNS names arrive in mixed case and are sometimes fully qualified with a
    // trailing dot; both must still match.
    assert_eq!(company_for_domain("DoubleClick.NET"), Some("Google"));
    assert_eq!(company_for_domain("googlesyndication.com."), Some("Google"));
}

#[test]
fn unknown_and_empty_domains_are_not_trackers() {
    assert_eq!(company_for_domain("example.com"), None);
    assert_eq!(company_for_domain("wardnet.network"), None);
    assert_eq!(company_for_domain(""), None);
    assert_eq!(company_for_domain("."), None);
    assert!(!is_tracker("nas.lan"));
}

#[test]
fn a_bare_public_suffix_does_not_match_a_tracker() {
    // The walk must not treat a plain TLD as a tracker just because a listed
    // tracker happens to live under it.
    assert!(!is_tracker("net"));
    assert!(!is_tracker("com"));
}

#[test]
fn catalogue_covers_a_reasonable_spread_of_companies() {
    // Guards against an accidental table truncation collapsing coverage.
    assert!(
        company_count() >= 20,
        "expected a broad tracker catalogue, got {} companies",
        company_count()
    );
}
