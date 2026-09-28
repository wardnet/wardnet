//! The device-event catalogue. Its slugs are the stored `kind` column and the
//! wire form both SDKs match on, so they are pinned here rather than left to
//! whatever `serde` happens to derive.

use crate::device_event::DeviceEventKind;

/// `ALL` is what the timeline iterates; a variant missing from it would
/// silently never be rendered.
#[test]
fn all_contains_every_variant() {
    // Bump this alongside the enum; the count is the whole point of the test.
    assert_eq!(DeviceEventKind::ALL.len(), 7);

    let mut slugs: Vec<&str> = DeviceEventKind::ALL.iter().map(|k| k.as_str()).collect();
    slugs.sort_unstable();
    slugs.dedup();
    assert_eq!(
        slugs.len(),
        DeviceEventKind::ALL.len(),
        "slugs must be unique"
    );
}

#[test]
fn from_slug_round_trips_every_variant() {
    for &kind in DeviceEventKind::ALL {
        assert_eq!(DeviceEventKind::from_slug(kind.as_str()), Some(kind));
    }
}

/// A row written by a newer daemon and read after a downgrade must not blow up
/// the timeline — callers skip unknown slugs.
#[test]
fn from_slug_rejects_an_unknown_slug() {
    assert_eq!(DeviceEventKind::from_slug("teleported"), None);
    assert_eq!(DeviceEventKind::from_slug(""), None);
}

/// The serde form feeds the HTTP API and both SDKs, so it has to agree with
/// `as_str` rather than merely being close to it.
#[test]
fn serde_form_matches_as_str() {
    for &kind in DeviceEventKind::ALL {
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, format!("\"{}\"", kind.as_str()));

        let back: DeviceEventKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, kind);
    }
}

/// An arrival and a return are different facts — collapsing them would lose
/// the distinction the timeline exists to draw.
#[test]
fn discovered_and_returned_are_distinct() {
    assert_ne!(DeviceEventKind::Discovered, DeviceEventKind::Returned);
    assert_eq!(DeviceEventKind::Discovered.as_str(), "discovered");
    assert_eq!(DeviceEventKind::Returned.as_str(), "returned");
}
