//! Curated tracker categorisation.
//!
//! Maps well-known tracking and telemetry domains to the company that
//! operates them, so the analytics layer can answer "which trackers is this
//! network blocking, and who runs them" rather than showing a flat list of
//! opaque hostnames. It is a deliberately small, hand-curated table — the
//! heavy blocklists (Steven Black, OISD, …) still decide *what* gets blocked;
//! this table only *labels* the blocks that happen to be recognisable
//! trackers, grouped by their parent organisation.
//!
//! Matching is by domain suffix: a lookup for `pagead2.googlesyndication.com`
//! walks its parent domains (`googlesyndication.com`, `com`) until one is
//! present in the table. Entries are registrable-domain suffixes, so every
//! subdomain of a listed tracker is attributed to the same company.
//!
//! The company name is the label surfaced in "top trackers blocked". The list
//! is intentionally conservative: entries are limited to domains whose sole or
//! primary purpose is advertising, analytics, or cross-site tracking. Mixed-use
//! domains that also serve first-party content are left out to avoid
//! mislabelling legitimate traffic.

use std::collections::HashMap;
use std::sync::LazyLock;

/// Registrable-domain → operating-company pairs.
///
/// Kept alphabetically by company then domain so additions stay reviewable and
/// merge cleanly. Domains are lowercase, without a leading dot.
const TRACKER_ENTRIES: &[(&str, &str)] = &[
    // Adobe
    ("omtrdc.net", "Adobe"),
    ("demdex.net", "Adobe"),
    ("2o7.net", "Adobe"),
    ("everesttech.net", "Adobe"),
    // Amazon
    ("amazon-adsystem.com", "Amazon"),
    ("assoc-amazon.com", "Amazon"),
    ("serving-sys.com", "Amazon"),
    // AppNexus / Xandr (Microsoft)
    ("adnxs.com", "AppNexus"),
    ("adnxs-simple.com", "AppNexus"),
    // Braze
    ("appboy.com", "Braze"),
    // Branch
    ("branch.io", "Branch"),
    ("app.link", "Branch"),
    // Chartbeat
    ("chartbeat.com", "Chartbeat"),
    ("chartbeat.net", "Chartbeat"),
    // Criteo
    ("criteo.com", "Criteo"),
    ("criteo.net", "Criteo"),
    // Comscore
    ("scorecardresearch.com", "Comscore"),
    ("comscore.com", "Comscore"),
    // Google / Alphabet
    ("doubleclick.net", "Google"),
    ("googlesyndication.com", "Google"),
    ("googleadservices.com", "Google"),
    ("google-analytics.com", "Google"),
    ("googletagmanager.com", "Google"),
    ("googletagservices.com", "Google"),
    ("g.doubleclick.net", "Google"),
    ("adservice.google.com", "Google"),
    ("app-measurement.com", "Google"),
    ("crashlytics.com", "Google"),
    // HubSpot
    ("hubspot.com", "HubSpot"),
    ("hs-analytics.net", "HubSpot"),
    ("hs-scripts.com", "HubSpot"),
    // The Trade Desk
    ("adsrvr.org", "The Trade Desk"),
    // Meta / Facebook
    ("facebook.com", "Meta"),
    ("connect.facebook.net", "Meta"),
    ("atdmt.com", "Meta"),
    ("fbcdn.net", "Meta"),
    // Microsoft
    ("bat.bing.com", "Microsoft"),
    ("clarity.ms", "Microsoft"),
    // Mixpanel
    ("mixpanel.com", "Mixpanel"),
    ("mxpnl.com", "Mixpanel"),
    // New Relic
    ("nr-data.net", "New Relic"),
    ("newrelic.com", "New Relic"),
    // Outbrain
    ("outbrain.com", "Outbrain"),
    // Oracle
    ("bluekai.com", "Oracle"),
    ("bkrtx.com", "Oracle"),
    ("addthis.com", "Oracle"),
    // Quantcast
    ("quantserve.com", "Quantcast"),
    ("quantcount.com", "Quantcast"),
    // Segment
    ("segment.com", "Segment"),
    ("segment.io", "Segment"),
    // Sentry
    ("sentry.io", "Sentry"),
    // Snap
    ("sc-static.net", "Snap"),
    ("snapchat.com", "Snap"),
    // Taboola
    ("taboola.com", "Taboola"),
    // TikTok / ByteDance
    ("tiktok.com", "TikTok"),
    ("byteoversea.com", "TikTok"),
    ("ttwstatic.com", "TikTok"),
    // Twitter / X
    ("ads-twitter.com", "X"),
    ("t.co", "X"),
    ("analytics.twitter.com", "X"),
    // Yandex
    ("yandex.ru", "Yandex"),
    ("mc.yandex.ru", "Yandex"),
    ("appmetrica.yandex.net", "Yandex"),
];

/// Lazily-built lookup from registrable-domain suffix to company name.
static TRACKER_MAP: LazyLock<HashMap<&'static str, &'static str>> =
    LazyLock::new(|| TRACKER_ENTRIES.iter().copied().collect());

/// Return the company operating `domain` if it is a recognised tracker.
///
/// Matches by suffix: the domain and each of its parent domains are checked in
/// turn, so any subdomain of a listed tracker resolves to the same company.
/// The lookup is case-insensitive on ASCII and tolerant of a trailing dot
/// (fully-qualified names). Returns `None` for domains not in the catalogue.
#[must_use]
pub fn company_for_domain(domain: &str) -> Option<&'static str> {
    let domain = domain.trim_end_matches('.').to_ascii_lowercase();
    if domain.is_empty() {
        return None;
    }

    // Walk parent domains: "a.b.example.com" → "b.example.com" → "example.com".
    let mut candidate: &str = &domain;
    loop {
        if let Some(&company) = TRACKER_MAP.get(candidate) {
            return Some(company);
        }
        let idx = candidate.find('.')?;
        candidate = &candidate[idx + 1..];
    }
}

/// Whether `domain` is a recognised tracker.
#[must_use]
pub fn is_tracker(domain: &str) -> bool {
    company_for_domain(domain).is_some()
}

/// Number of distinct companies represented in the catalogue.
///
/// Exposed mainly so callers (and tests) can reason about catalogue coverage
/// without walking the raw entry table.
#[must_use]
pub fn company_count() -> usize {
    TRACKER_ENTRIES
        .iter()
        .map(|(_, company)| *company)
        .collect::<std::collections::BTreeSet<_>>()
        .len()
}
