//! Host-independent tests for the netlink firewall manager's pure helpers.
//!
//! The rustables socket calls have no mockable boundary (they need a real
//! kernel) and are exercised by the e2e harness, mirroring how
//! `policy_router_netlink` is covered. What we CAN test here is the comment
//! UDATA TLV codec that drives restart-survivable rule identification.

use crate::firewall_netlink::{
    IFNAMSIZ, ZONE_ESTABLISHED_COMMENT, ZONE_ISOJUMP_COMMENT, ZONE_ISOLATION, ZONE_NATEXEMPT,
    ZONE_NATEXEMPT_COMMENT, ZONE_NATEXEMPT_JUMP_COMMENT, build_natexempt_batch,
    build_zone_isolation_batch, chain_has_comment, chain_ref, comment_udata,
    inbound_wg_iface_exact_value, masquerade_rule, natexempt_jump_rule, parse_comment_udata,
    rule_comment, wardnet_table, zone_rule_ip,
};
use rustables::{Batch, Rule};
use wardnetd_services::routing::firewall::{ExceptionAllow, ZoneIsolationRules};

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
fn zone_rule_ip_keys_device_gates_on_their_ipv4() {
    // The per-device egress and admin-UI gates are IP-keyed; their trailing
    // segment is a real device IP and must decode as one so orphan cleanup and
    // re-keying keep working.
    assert_eq!(
        zone_rule_ip("wardnet:zone:egress:192.168.1.10"),
        Some("192.168.1.10")
    );
    assert_eq!(
        zone_rule_ip("wardnet:zone:adminui:10.0.0.5"),
        Some("10.0.0.5")
    );
}

#[test]
fn zone_rule_ip_rejects_zone_control_comments() {
    // The base-forward jump and the isolation ACCEPT rules share the zone
    // comment prefix but carry no device IP. Treating their trailing segment as
    // an "IP" is exactly what let orphan cleanup delete the jump rule (and, with
    // it, silently disable all L3 zone isolation for the boot).
    assert_eq!(zone_rule_ip("wardnet:zone:isojump"), None);
    assert_eq!(zone_rule_ip("wardnet:zone:allow"), None);
    // A truncated/empty tail is not an IP either.
    assert_eq!(zone_rule_ip("wardnet:zone:"), None);
    assert_eq!(zone_rule_ip("wardnet:zone:not-an-ip"), None);
}

#[test]
fn zone_rule_ip_ignores_foreign_comment_namespaces() {
    // Comments outside `wardnet:zone:` are never device zone rules, even when
    // their trailing segment happens to be an IP.
    assert_eq!(zone_rule_ip("wardnet:inbound-wg:listen"), None);
    assert_eq!(zone_rule_ip("wardnet:dns:192.168.1.100"), None);
    assert_eq!(zone_rule_ip("wardnet:wg_ward0"), None);
}

#[test]
fn isojump_only_table_produces_no_orphans_and_survives_removal() {
    // Regression for the startup collision: right after `routing.reconcile()`
    // installs the FORWARD→zone_isolation jump but before any device rule
    // exists, the wardnet table's FORWARD/INPUT comments are just the jump's.
    // `list_zone_rule_ips` and `remove_zone_rules` both key on `zone_rule_ip`, so
    // model that state through the same gate the socket layer uses.
    let forward_input_comments = ["wardnet:zone:isojump"];

    // `list_zone_rule_ips` derives its orphan candidates via `zone_rule_ip`; the
    // control jump must not surface as a device IP, or reconcile would delete it.
    let orphan_candidates: Vec<&str> = forward_input_comments
        .iter()
        .filter_map(|c| zone_rule_ip(c))
        .collect();
    assert!(
        orphan_candidates.is_empty(),
        "isojump control rule must not be listed as a device IP: {orphan_candidates:?}"
    );

    // `remove_zone_rules(device_ip)` selects rules where `zone_rule_ip == ip`, so
    // no reachable device_ip — including the "isojump" literal the old suffix
    // match keyed on — can ever match the jump rule's comment.
    for device_ip in ["isojump", "192.168.1.10", "10.0.0.1"] {
        let would_delete = forward_input_comments
            .iter()
            .any(|c| zone_rule_ip(c) == Some(device_ip));
        assert!(
            !would_delete,
            "remove_zone_rules({device_ip}) must not match the isojump jump rule"
        );
    }
}

#[test]
fn zone_isolation_established_return_accept_is_first_rule() {
    // The stateful cross-zone return-accept (`ct state established,related
    // accept`) must be the FIRST rule in the rebuilt `zone_isolation` chain so a
    // Chromecast reply stream (sport=8009) is accepted before the stateless
    // dport allows and the cross-subnet deny below it. Building the batch is pure
    // (no socket I/O), so we can assert on the emitted rule order host-independently.
    let table = wardnet_table();
    let iso = chain_ref(&table, ZONE_ISOLATION);
    let mut batch = Batch::new();

    let rules = ZoneIsolationRules {
        allows: vec![ExceptionAllow {
            from_cidr: "192.168.200.0/24".to_owned(),
            to_cidr: "192.168.201.0/24".to_owned(),
            proto: "tcp".to_owned(),
            port_start: 8009,
            port_end: 8009,
            bidirectional: true,
        }],
        deny_pairs: vec![("192.168.200.0/24".to_owned(), "192.168.201.0/24".to_owned())],
        member_isolation_subnets: vec!["192.168.200.0/24".to_owned()],
        nat_exempt_pairs: vec![("192.168.200.0/24".to_owned(), "192.168.201.0/24".to_owned())],
    };

    let order = build_zone_isolation_batch(&iso, &rules, &mut batch)
        .expect("building the zone_isolation batch must succeed");

    // The established return-accept is present and sits at the very top.
    assert_eq!(
        order.first().copied(),
        Some(ZONE_ESTABLISHED_COMMENT),
        "established return-accept must be the first rule: {order:?}"
    );
    assert_eq!(
        order
            .iter()
            .filter(|c| **c == ZONE_ESTABLISHED_COMMENT)
            .count(),
        1,
        "exactly one established return-accept rule expected: {order:?}"
    );
    // A bidirectional allow expands to two ACCEPTs, both after the established rule.
    assert!(
        order[1..].iter().all(|c| *c != ZONE_ESTABLISHED_COMMENT),
        "established rule must not repeat after the first slot: {order:?}"
    );
}

#[test]
fn zone_natexempt_chain_has_accept_for_pair_in_both_directions() {
    // Each exception pair yields TWO accept rules in `zone_natexempt` — the
    // forward and the reverse direction — so the un-NAT applies to both the
    // sender→receiver and receiver→sender legs. Building the batch is pure.
    let table = wardnet_table();
    let natexempt = chain_ref(&table, ZONE_NATEXEMPT);
    let mut batch = Batch::new();

    let pairs = vec![("192.168.200.0/24".to_owned(), "192.168.201.0/24".to_owned())];
    let order = build_natexempt_batch(&natexempt, &pairs, &mut batch)
        .expect("building the zone_natexempt batch must succeed");

    assert_eq!(
        order,
        vec![ZONE_NATEXEMPT_COMMENT, ZONE_NATEXEMPT_COMMENT],
        "one pair must emit two accepts (both directions): {order:?}"
    );

    // Two distinct pairs → four accepts (two per pair).
    let two_pairs = vec![
        ("10.0.1.0/24".to_owned(), "10.0.2.0/24".to_owned()),
        ("10.0.3.5/32".to_owned(), "10.0.4.6/32".to_owned()),
    ];
    let mut batch2 = Batch::new();
    let order2 = build_natexempt_batch(&natexempt, &two_pairs, &mut batch2).unwrap();
    assert_eq!(
        order2.len(),
        4,
        "two pairs must emit four accepts: {order2:?}"
    );
    assert!(
        order2.iter().all(|c| *c == ZONE_NATEXEMPT_COMMENT),
        "every emitted rule is a NAT-exempt accept: {order2:?}"
    );
}

#[test]
fn chain_has_comment_drives_jump_idempotence() {
    // `ensure_isolation_jumps` adds a base-chain jump only when its comment is
    // ABSENT (present ⇒ skip, so re-runs don't stack duplicates). Exercise that
    // guard predicate directly: building rules with a comment is pure (no socket).
    let table = wardnet_table();
    let forward = chain_ref(&table, ZONE_ISOLATION);
    let iso_rule = Rule::new(&forward)
        .unwrap()
        .with_userdata(comment_udata(ZONE_ISOJUMP_COMMENT));
    let rules = vec![iso_rule];

    // Present → guard reports true → the add is skipped.
    assert!(
        chain_has_comment(&rules, ZONE_ISOJUMP_COMMENT),
        "isojump comment must be detected as present"
    );
    // A different jump comment is absent from the same chain → its add proceeds.
    assert!(
        !chain_has_comment(&rules, ZONE_NATEXEMPT_JUMP_COMMENT),
        "natexempt jump must read as absent when only the isojump is present"
    );
    // An empty chain (post-flush / fresh table) reports neither → both jumps add.
    assert!(!chain_has_comment(&[], ZONE_ISOJUMP_COMMENT));
    assert!(!chain_has_comment(&[], ZONE_NATEXEMPT_JUMP_COMMENT));
}

#[test]
fn postrouting_natexempt_jump_precedes_masquerade() {
    // `init_wardnet_table` adds the `jump zone_natexempt` rule to postrouting
    // before `add_masquerade` appends any masquerade (rustables always appends —
    // there is no insert-at-index), so the jump is evaluated first and NAT-exempt
    // flows are accepted before masquerade can rewrite their source. Model that
    // exact emission order with the real rule builders and assert the ordering.
    let table = wardnet_table();
    let postrouting = chain_ref(&table, "postrouting");

    let jump = natexempt_jump_rule(&postrouting).expect("jump rule builds");
    let masq = masquerade_rule(&postrouting, "eth0").expect("masquerade rule builds");

    let order: Vec<String> = [&jump, &masq]
        .iter()
        .filter_map(|r| rule_comment(r))
        .collect();

    assert_eq!(
        order,
        vec![
            ZONE_NATEXEMPT_JUMP_COMMENT.to_owned(),
            "wardnet:eth0".to_owned()
        ],
        "the natexempt jump must be emitted before the masquerade: {order:?}"
    );

    let jump_idx = order
        .iter()
        .position(|c| c == ZONE_NATEXEMPT_JUMP_COMMENT)
        .expect("jump present");
    let masq_idx = order
        .iter()
        .position(|c| c == "wardnet:eth0")
        .expect("masquerade present");
    assert!(
        jump_idx < masq_idx,
        "jump ({jump_idx}) must precede masquerade ({masq_idx})"
    );
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
