//! Unit tests for kernel output parsing helpers.

use super::*;

#[test]
fn parse_ip_rule_output() {
    let raw = "\
0:\tfrom all lookup local
100:\tfrom 10.232.1.210/32 lookup 100
32766:\tfrom all lookup main
32767:\tfrom all lookup default
";
    let rules: Vec<IpRule> = IP_RULE_RE
        .captures_iter(raw)
        .filter_map(|cap| {
            let priority = cap[1].parse::<u32>().ok()?;
            Some(IpRule {
                priority,
                from: cap[2].to_owned(),
                table: cap[3].to_owned(),
            })
        })
        .collect();

    assert_eq!(rules.len(), 4);
    assert_eq!(rules[0].priority, 0);
    assert_eq!(rules[0].from, "all");
    assert_eq!(rules[0].table, "local");
    assert_eq!(rules[1].priority, 100);
    assert_eq!(rules[1].from, "10.232.1.210/32");
    assert_eq!(rules[1].table, "100");
    assert_eq!(rules[2].priority, 32766);
    assert_eq!(rules[2].table, "main");
}

#[test]
fn parse_nft_tables() {
    let raw = "\
table inet wardnet {
    chain postrouting {
        type nat hook postrouting priority srcnat; policy accept;
        oifname \"wg_ward0\" masquerade
    }
}
table inet filter {
    chain input {
    }
}
";
    let tables: Vec<String> = NFT_TABLE_RE
        .captures_iter(raw)
        .map(|cap| cap[1].to_owned())
        .collect();

    assert_eq!(tables, vec!["inet wardnet", "inet filter"]);

    let masq: Vec<String> = NFT_MASQ_RE
        .captures_iter(raw)
        .map(|cap| cap[1].to_owned())
        .collect();

    assert_eq!(masq, vec!["wg_ward0"]);
}

#[test]
fn parse_wg_show_output() {
    let raw = "\
interface: wg_ward0
  public key: abcdefghijklmnopqrstuvwxyz123456789012345=
  private key: (hidden)
  listening port: 51820

peer: QRSTUVWXYZ123456789012345abcdefghijklmnop=
  endpoint: 10.232.1.250:51820
  allowed ips: 10.99.1.0/24
  latest handshake: 1 minute, 30 seconds ago
  transfer: 1.24 KiB received, 3.48 KiB sent
";
    let (pk, port, peers) = parse_wg_show(raw);

    assert_eq!(
        pk.as_deref(),
        Some("abcdefghijklmnopqrstuvwxyz123456789012345=")
    );
    assert_eq!(port, Some(51820));
    assert_eq!(peers.len(), 1);

    let peer = &peers[0];
    assert_eq!(
        peer.public_key,
        "QRSTUVWXYZ123456789012345abcdefghijklmnop="
    );
    assert_eq!(peer.endpoint.as_deref(), Some("10.232.1.250:51820"));
    assert_eq!(peer.allowed_ips, vec!["10.99.1.0/24"]);
    assert_eq!(
        peer.latest_handshake.as_deref(),
        Some("1 minute, 30 seconds ago")
    );
    // 1.24 KiB ~= 1269 bytes
    assert_eq!(peer.transfer_rx, 1269);
    // 3.48 KiB ~= 3563 bytes
    assert_eq!(peer.transfer_tx, 3563);
}

#[test]
fn parse_wg_show_no_peers() {
    let raw = "\
interface: wg_ward0
  public key: abcdefghijklmnopqrstuvwxyz123456789012345=
  private key: (hidden)
  listening port: 51820
";
    let (pk, port, peers) = parse_wg_show(raw);

    assert!(pk.is_some());
    assert_eq!(port, Some(51820));
    assert!(peers.is_empty());
}

#[test]
fn parse_mtu_from_link_output() {
    let raw = "2: wg_ward0: <POINTOPOINT,NOARP,UP,LOWER_UP> mtu 1420 qdisc noqueue state UNKNOWN";
    let mtu = parse_mtu(raw);
    assert_eq!(mtu, Some(1420));
}

#[test]
fn parse_mtu_missing() {
    let raw = "Device \"wg_ward0\" does not exist.";
    let mtu = parse_mtu(raw);
    assert_eq!(mtu, None);
}

#[test]
fn valid_interface_names() {
    assert!(is_valid_interface_name("wg_ward0"));
    assert!(is_valid_interface_name("eth0"));
    assert!(is_valid_interface_name("lo"));

    // Too long (Linux limit is 15 chars).
    assert!(!is_valid_interface_name("a_very_long_interface_name"));
    // Empty.
    assert!(!is_valid_interface_name(""));
    // Contains disallowed characters.
    assert!(!is_valid_interface_name("wg-ward0"));
    assert!(!is_valid_interface_name("wg ward0"));
    assert!(!is_valid_interface_name("wg;rm -rf"));
}

#[test]
fn byte_value_parsing() {
    assert_eq!(parse_byte_value("0 B received"), 0);
    assert_eq!(parse_byte_value("1.24 KiB received"), 1269);
    assert_eq!(parse_byte_value("2.00 MiB sent"), 2_097_152);
    assert_eq!(parse_byte_value(""), 0);
    assert_eq!(parse_byte_value("garbage"), 0);
}
