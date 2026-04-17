use crate::wireguard_config::{WgConfigError, parse};

#[test]
fn parse_minimal_config() {
    let input = "\
[Interface]
PrivateKey = AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=

[Peer]
PublicKey = BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB=
";
    let config = parse(input).unwrap();
    assert_eq!(
        config.interface.private_key,
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
    );
    assert!(config.interface.address.is_empty());
    assert!(config.interface.listen_port.is_none());
    assert!(config.interface.dns.is_empty());
    assert_eq!(config.peers.len(), 1);
    assert_eq!(
        config.peers[0].public_key,
        "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB="
    );
    assert!(config.peers[0].endpoint.is_none());
    assert!(config.peers[0].allowed_ips.is_empty());
    assert!(config.peers[0].preshared_key.is_none());
    assert!(config.peers[0].persistent_keepalive.is_none());
}

#[test]
fn parse_full_config() {
    let input = "\
[Interface]
PrivateKey = AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=
Address = 10.0.0.1/24
ListenPort = 51820
DNS = 1.1.1.1

[Peer]
PublicKey = BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB=
Endpoint = 203.0.113.1:51820
AllowedIPs = 0.0.0.0/0
PresharedKey = CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC=
PersistentKeepalive = 25
";
    let config = parse(input).unwrap();
    assert_eq!(config.interface.address, vec!["10.0.0.1/24"]);
    assert_eq!(config.interface.listen_port, Some(51820));
    assert_eq!(config.interface.dns, vec!["1.1.1.1"]);
    assert_eq!(
        config.peers[0].endpoint.as_deref(),
        Some("203.0.113.1:51820")
    );
    assert_eq!(config.peers[0].allowed_ips, vec!["0.0.0.0/0"]);
    assert_eq!(
        config.peers[0].preshared_key.as_deref(),
        Some("CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC=")
    );
    assert_eq!(config.peers[0].persistent_keepalive, Some(25));
}

#[test]
fn parse_comma_separated_address() {
    let input = "\
[Interface]
PrivateKey = AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=
Address = 10.0.0.1/24, 10.0.0.2/24

[Peer]
PublicKey = BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB=
";
    let config = parse(input).unwrap();
    assert_eq!(config.interface.address, vec!["10.0.0.1/24", "10.0.0.2/24"]);
}

#[test]
fn parse_multiple_dns() {
    let input = "\
[Interface]
PrivateKey = AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=
DNS = 1.1.1.1, 8.8.8.8

[Peer]
PublicKey = BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB=
";
    let config = parse(input).unwrap();
    assert_eq!(config.interface.dns, vec!["1.1.1.1", "8.8.8.8"]);
}

#[test]
fn reject_missing_interface() {
    let input = "\
[Peer]
PublicKey = BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB=
";
    let err = parse(input).unwrap_err();
    assert!(matches!(err, WgConfigError::MissingInterface));
}

#[test]
fn reject_missing_private_key() {
    let input = "\
[Interface]
Address = 10.0.0.1/24

[Peer]
PublicKey = BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB=
";
    let err = parse(input).unwrap_err();
    assert!(matches!(err, WgConfigError::MissingPrivateKey));
}

#[test]
fn reject_missing_peer() {
    let input = "\
[Interface]
PrivateKey = AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=
";
    let err = parse(input).unwrap_err();
    assert!(matches!(err, WgConfigError::MissingPeer));
}

#[test]
fn reject_missing_public_key() {
    let input = "\
[Interface]
PrivateKey = AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=

[Peer]
Endpoint = 203.0.113.1:51820
";
    let err = parse(input).unwrap_err();
    assert!(matches!(err, WgConfigError::MissingPublicKey));
}

#[test]
fn handle_comments_and_blank_lines() {
    let input = "\
# This is a comment
; Another comment

[Interface]
PrivateKey = AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=

# Comment between sections

[Peer]
; Peer comment
PublicKey = BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB=
";
    let config = parse(input).unwrap();
    assert_eq!(
        config.interface.private_key,
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
    );
    assert_eq!(config.peers.len(), 1);
}

#[test]
fn handle_windows_line_endings() {
    let input = "[Interface]\r\nPrivateKey = AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\r\n\r\n[Peer]\r\nPublicKey = BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB=\r\n";
    let config = parse(input).unwrap();
    assert_eq!(
        config.interface.private_key,
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
    );
    assert_eq!(config.peers.len(), 1);
}
