use super::{extract_install_name, parse_sni};

/// A minimal TLS 1.2 `ClientHello` with SNI = "example.com", assembled by hand.
///
/// Structure:
///   TLS record:          16 03 01 [len2] ...
///   Handshake header:    01 [len3] ...
///   `ClientHello` body:  03 03 [random 32] 00 [cs 2+2] 01 00
///   Extensions:          [`ext_len2`] [SNI ext]
fn make_client_hello(sni: &str) -> Vec<u8> {
    // SNI extension payload
    let name_bytes = sni.as_bytes();
    let name_len = u16::try_from(name_bytes.len()).unwrap();
    let list_len = name_len + 3;
    let mut sni_ext = Vec::new();
    sni_ext.extend_from_slice(&list_len.to_be_bytes());
    sni_ext.push(0x00); // host_name type
    sni_ext.extend_from_slice(&name_len.to_be_bytes());
    sni_ext.extend_from_slice(name_bytes);

    // Extensions block: type(2) + len(2) + data
    let sni_ext_len = u16::try_from(sni_ext.len()).unwrap();
    let mut exts = Vec::new();
    exts.extend_from_slice(&0x0000u16.to_be_bytes()); // SNI extension type
    exts.extend_from_slice(&sni_ext_len.to_be_bytes());
    exts.extend_from_slice(&sni_ext);

    // ClientHello body
    let exts_len = u16::try_from(exts.len()).unwrap();
    let mut hello = Vec::new();
    hello.extend_from_slice(&0x0303u16.to_be_bytes()); // TLS 1.2 version
    hello.extend_from_slice(&[0u8; 32]); // random
    hello.push(0x00); // session_id_len
    hello.extend_from_slice(&0x0002u16.to_be_bytes()); // cipher_suites_len
    hello.extend_from_slice(&[0x00, 0x2f]); // one cipher suite
    hello.push(0x01); // compression_methods_len
    hello.push(0x00); // null compression
    hello.extend_from_slice(&exts_len.to_be_bytes());
    hello.extend_from_slice(&exts);

    // Handshake header: type(1) + length(3)
    let hello_len = u32::try_from(hello.len()).unwrap();
    let mut hs = vec![
        0x01u8, // ClientHello
        ((hello_len >> 16) & 0xff) as u8,
        ((hello_len >> 8) & 0xff) as u8,
        (hello_len & 0xff) as u8,
    ];
    hs.extend_from_slice(&hello);

    // TLS record header: type(1) + version(2) + length(2)
    let hs_len = u16::try_from(hs.len()).unwrap();
    let mut record = Vec::new();
    record.push(0x16); // handshake
    record.extend_from_slice(&0x0301u16.to_be_bytes()); // TLS 1.0 record version
    record.extend_from_slice(&hs_len.to_be_bytes());
    record.extend_from_slice(&hs);

    record
}

#[test]
fn parse_sni_extracts_hostname() {
    let buf = make_client_hello("happy-einstein.my.us.wardnet.network");
    let sni = parse_sni(&buf);
    assert_eq!(sni.as_deref(), Some("happy-einstein.my.us.wardnet.network"));
}

#[test]
fn parse_sni_returns_none_for_empty_buffer() {
    assert!(parse_sni(&[]).is_none());
}

#[test]
fn parse_sni_returns_none_for_non_handshake() {
    // First byte is 0x17 (application data), not 0x16 (handshake).
    let mut buf = make_client_hello("test.example.com");
    buf[0] = 0x17;
    assert!(parse_sni(&buf).is_none());
}

#[test]
fn parse_sni_returns_none_for_truncated_buffer() {
    let buf = make_client_hello("test.example.com");
    // Provide only the first 10 bytes.
    assert!(parse_sni(&buf[..10]).is_none());
}

#[test]
fn extract_install_name_simple() {
    assert_eq!(
        extract_install_name(
            "happy-einstein.my.us.wardnet.network",
            "my.us.wardnet.network"
        ),
        Some("happy-einstein")
    );
}

#[test]
fn extract_install_name_rejects_multi_label() {
    assert!(
        extract_install_name("foo.bar.my.us.wardnet.network", "my.us.wardnet.network").is_none()
    );
}

#[test]
fn extract_install_name_rejects_wrong_parent() {
    assert!(extract_install_name("foo.other.network", "my.us.wardnet.network").is_none());
}

#[test]
fn extract_install_name_rejects_bare_parent() {
    assert!(extract_install_name("my.us.wardnet.network", "my.us.wardnet.network").is_none());
}
