//! Host-independent tests for the netlink firewall manager's pure helpers.
//!
//! The rustables socket calls have no mockable boundary (they need a real
//! kernel) and are exercised by the e2e harness, mirroring how
//! `policy_router_netlink` is covered. What we CAN test here is the comment
//! UDATA TLV codec that drives restart-survivable rule identification.

use crate::firewall_netlink::{
    IFNAMSIZ, comment_udata, inbound_wg_iface_exact_value, parse_comment_udata,
};

#[test]
fn comment_udata_encodes_type_len_value_nul() {
    // [type=0][len=3][b'a', b'b', NUL] — len counts the NUL terminator.
    assert_eq!(comment_udata("ab"), vec![0u8, 3, b'a', b'b', 0]);
}

#[test]
fn comment_udata_round_trips() {
    for comment in [
        "wardnet:wg_ward0",
        "wardnet:rst:192.168.1.10",
        "wardnet:dns:192.168.1.100",
        "", // empty comment still round-trips
    ] {
        let encoded = comment_udata(comment);
        assert_eq!(
            parse_comment_udata(&encoded).as_deref(),
            Some(comment),
            "round-trip failed for {comment:?}"
        );
    }
}

#[test]
fn parse_comment_udata_strips_trailing_nul() {
    // Manually-built TLV with a NUL terminator.
    let data = [0u8, 5, b'h', b'e', b'l', b'l', 0];
    assert_eq!(parse_comment_udata(&data).as_deref(), Some("hell"));
}

#[test]
fn parse_comment_udata_returns_none_for_empty() {
    assert_eq!(parse_comment_udata(&[]), None);
}

#[test]
fn parse_comment_udata_skips_non_comment_tlvs() {
    // A non-comment TLV (type=3, len=2) followed by the comment TLV (type=0).
    let mut data = vec![3u8, 2, 0xAA, 0xBB];
    data.extend_from_slice(&comment_udata("wardnet:wg_ward0"));
    assert_eq!(
        parse_comment_udata(&data).as_deref(),
        Some("wardnet:wg_ward0")
    );
}

#[test]
fn parse_comment_udata_handles_truncated_tlv() {
    // Claims len=10 but only 2 value bytes follow — must not panic, returns None.
    let data = [0u8, 10, b'x', b'y'];
    assert_eq!(parse_comment_udata(&data), None);
}

#[test]
fn parse_comment_udata_returns_none_when_no_comment_tlv() {
    // Only a non-comment TLV present.
    let data = [7u8, 1, 0x42];
    assert_eq!(parse_comment_udata(&data), None);
}

#[test]
fn inbound_wg_iface_exact_value_zero_pads_to_ifnamsiz() {
    // The #810 exclusion loads `meta oifname` (a full IFNAMSIZ-wide interface
    // name) and compares it `!=` this value, so the value must be the inbound
    // server interface name `wg_wardin0` zero-padded to IFNAMSIZ — byte-for-byte
    // what the kernel places in the comparison register.
    let value = inbound_wg_iface_exact_value();
    assert_eq!(value.len(), IFNAMSIZ, "must be exactly IFNAMSIZ bytes");

    let name = b"wg_wardin0";
    let mut expected = vec![0u8; IFNAMSIZ];
    expected[..name.len()].copy_from_slice(name);
    assert_eq!(value, expected, "decodes to wg_wardin0 zero-padded");

    // The whole point of the exact (unmasked) match is that it must NOT collide
    // with any other `wg_ward*`-prefixed interface (e.g. a provider tunnel
    // `wg_ward0`), which the masked prefix match would otherwise catch.
    let other_name = b"wg_ward0";
    let mut other = vec![0u8; IFNAMSIZ];
    other[..other_name.len()].copy_from_slice(other_name);
    assert_ne!(
        value, other,
        "must not equal another wg_ward*-prefixed interface's padded name"
    );
}
