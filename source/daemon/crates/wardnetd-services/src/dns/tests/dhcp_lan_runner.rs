//! Tests for the DHCP `.lan` hostname-label derivation.

use crate::dns::dhcp_lan_runner::lan_label;

#[test]
fn single_label_passthrough() {
    assert_eq!(lan_label("mypc").as_deref(), Some("mypc"));
}

#[test]
fn fqdn_keeps_only_first_label() {
    assert_eq!(lan_label("mypc.home.arpa").as_deref(), Some("mypc"));
}

#[test]
fn lowercased_and_trimmed() {
    assert_eq!(lan_label("  MyPC  ").as_deref(), Some("mypc"));
    assert_eq!(lan_label("HOST.local").as_deref(), Some("host"));
}

#[test]
fn empty_or_whitespace_is_none() {
    assert_eq!(lan_label(""), None);
    assert_eq!(lan_label("   "), None);
    // Leading dot → empty first label → None.
    assert_eq!(lan_label(".lan"), None);
}
