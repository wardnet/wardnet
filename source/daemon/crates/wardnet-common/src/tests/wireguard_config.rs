use crate::wireguard_config::{WgConfigError, WgInterfaceConfig, WgPeerConfig, parse};

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

#[test]
fn interface_config_debug_redacts_private_key() {
    let iface = WgInterfaceConfig {
        private_key: "SECRET-PRIVATE-KEY-DO-NOT-LEAK=".to_owned(),
        address: vec!["10.0.0.1/24".to_owned()],
        listen_port: Some(51820),
        dns: vec!["1.1.1.1".to_owned()],
    };
    let rendered = format!("{iface:?}");
    assert!(!rendered.contains("SECRET-PRIVATE-KEY-DO-NOT-LEAK="));
    assert!(rendered.contains("[REDACTED]"));
    // Non-secret fields still render verbatim.
    assert!(rendered.contains("10.0.0.1/24"));
    assert!(rendered.contains("51820"));
    assert!(rendered.contains("1.1.1.1"));
}

#[test]
fn peer_config_debug_redacts_preshared_key() {
    let peer = WgPeerConfig {
        public_key: "PUBLIC-KEY-VISIBLE=".to_owned(),
        endpoint: Some("203.0.113.1:51820".to_owned()),
        allowed_ips: vec!["0.0.0.0/0".to_owned()],
        preshared_key: Some("SECRET-PSK-DO-NOT-LEAK=".to_owned()),
        persistent_keepalive: Some(25),
    };
    let rendered = format!("{peer:?}");
    assert!(!rendered.contains("SECRET-PSK-DO-NOT-LEAK="));
    assert!(rendered.contains("[REDACTED]"));
    // Non-secret fields (including the public key) still render verbatim.
    assert!(rendered.contains("PUBLIC-KEY-VISIBLE="));
    assert!(rendered.contains("203.0.113.1:51820"));
    assert!(rendered.contains("0.0.0.0/0"));
    assert!(rendered.contains("25"));
}

#[test]
fn peer_config_debug_without_preshared_key_shows_none() {
    let peer = WgPeerConfig {
        public_key: "PUBLIC-KEY-VISIBLE=".to_owned(),
        endpoint: None,
        allowed_ips: vec![],
        preshared_key: None,
        persistent_keepalive: None,
    };
    let rendered = format!("{peer:?}");
    // Absence of a PSK renders as `None`, not `[REDACTED]`.
    assert!(rendered.contains("preshared_key: None"));
    assert!(!rendered.contains("[REDACTED]"));
}
