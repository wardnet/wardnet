use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::config::Config;
use crate::tunnel::registry::TunnelRegistry;

use super::{extract_install_name, parse_sni, route};

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
    let buf = make_client_hello("happy-einstein.my.wardnet.services");
    let sni = parse_sni(&buf);
    assert_eq!(sni.as_deref(), Some("happy-einstein.my.wardnet.services"));
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
        extract_install_name("happy-einstein.my.wardnet.services", ".my.wardnet.services"),
        Some("happy-einstein")
    );
}

#[test]
fn extract_install_name_routes_multi_label_to_rightmost_vanity() {
    // A multi-label prefix (a per-service host) routes by the rightmost label
    // before the suffix — the vanity name — not rejected.
    assert_eq!(
        extract_install_name("foo.bar.my.wardnet.services", ".my.wardnet.services"),
        Some("bar")
    );
}

#[test]
fn extract_install_name_per_service_host() {
    // `<service>.<vanity>.<suffix>` routes to the vanity's tunnel.
    assert_eq!(
        extract_install_name("jellyfin.alice.my.wardnet.services", ".my.wardnet.services"),
        Some("alice")
    );
}

#[test]
fn extract_install_name_rejects_empty_label() {
    // A malformed prefix with a trailing/double dot yields an empty rightmost
    // label, which is not a routable vanity.
    assert!(extract_install_name("foo..my.wardnet.services", ".my.wardnet.services").is_none());
}

#[test]
fn extract_install_name_rejects_invalid_vanity() {
    // The rightmost label is held to registration's rules: a too-short label
    // (< 3 chars) could never name a real tunnel.
    assert!(extract_install_name("ab.my.wardnet.services", ".my.wardnet.services").is_none());
}

#[test]
fn extract_install_name_rejects_wrong_parent() {
    assert!(extract_install_name("foo.other.network", ".my.wardnet.services").is_none());
}

#[test]
fn extract_install_name_rejects_bare_parent() {
    assert!(extract_install_name("my.wardnet.services", ".my.wardnet.services").is_none());
}

// ── run() integration test ────────────────────────────────────────────────────

/// Exercises `run()`'s accept-loop body: bind, log, compute suffix, create
/// semaphore, enter the loop, accept a connection, and spawn a routing task.
#[tokio::test]
async fn run_binds_and_routes_accepted_connection() {
    use std::time::Duration;

    // Grab a free port by binding temporarily, then release it.
    let temp = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = temp.local_addr().unwrap().port();
    drop(temp);
    // Brief pause so the OS makes the port available again.
    tokio::time::sleep(Duration::from_millis(5)).await;

    let config = make_test_config("127.0.0.1:1");
    let registry = Arc::new(TunnelRegistry::new());
    let addr = format!("127.0.0.1:{port}");
    let addr2 = addr.clone();

    // Spawn run() — it binds on addr and loops forever.
    tokio::spawn(async move { super::run(&addr2, 443, config, registry).await });
    // Wait for the bind to complete.
    tokio::time::sleep(Duration::from_millis(20)).await;

    // Connect and send non-TLS bytes → run() accepts the connection and spawns route().
    let mut client = TcpStream::connect(&addr).await.unwrap();
    client.write_all(b"not-tls").await.unwrap();
    drop(client);

    // Allow route() to run and drop the connection.
    tokio::time::sleep(Duration::from_millis(50)).await;
}

// ── route() integration tests ─────────────────────────────────────────────────

fn make_test_config(caddy_addr: &str) -> Config {
    Config {
        listen_addr: "127.0.0.1:0".to_string(),
        database_url: "postgres://ignored".to_string(),
        global_database_url: "postgres://ignored-global".to_string(),
        cloudflare_api_token: "test-token".to_string(),
        cloudflare_zone_id: "test-zone".to_string(),
        region: "test".to_string(),
        subdomain_parent: "my.wardnet.services".to_string(),
        sni_listen_addr: "0.0.0.0:443".to_string(),
        dot_listen_addr: "0.0.0.0:853".to_string(),
        caddy_addr: caddy_addr.to_string(),
        bridge_hostname: "bridge.test.wardnet.network".to_string(),
    }
}

#[tokio::test]
async fn route_drops_connection_when_no_sni() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let mut client = TcpStream::connect(addr).await.unwrap();
    let (server, peer) = listener.accept().await.unwrap();

    let config = make_test_config("127.0.0.1:1");
    let registry = Arc::new(TunnelRegistry::new());
    let suffix = format!(".{}", config.subdomain_parent);

    // Send non-TLS data so parse_sni returns None
    client.write_all(b"not-tls-data").await.unwrap();
    // Don't hold the client reference — route should complete without error
    drop(client);

    let result = route(server, peer, 443, config, registry, &suffix).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn route_drops_connection_for_unroutable_sni() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let mut client = TcpStream::connect(addr).await.unwrap();
    let (server, peer) = listener.accept().await.unwrap();

    let config = make_test_config("127.0.0.1:1");
    let registry = Arc::new(TunnelRegistry::new());
    let suffix = format!(".{}", config.subdomain_parent);

    // SNI that doesn't match bridge_hostname or subdomain_parent suffix
    let hello = make_client_hello("unrelated.example.com");
    client.write_all(&hello).await.unwrap();
    drop(client);

    let result = route(server, peer, 443, config, registry, &suffix).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn route_drops_connection_when_install_not_connected() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let mut client = TcpStream::connect(addr).await.unwrap();
    let (server, peer) = listener.accept().await.unwrap();

    let config = make_test_config("127.0.0.1:1");
    let registry = Arc::new(TunnelRegistry::new());
    // No tunnel registered for "install" → registry.forward returns NotConnected
    let suffix = format!(".{}", config.subdomain_parent);

    let hello = make_client_hello("install.my.wardnet.services");
    client.write_all(&hello).await.unwrap();
    drop(client);

    let result = route(server, peer, 443, config, registry, &suffix).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn route_forwards_bridge_hostname_to_caddy() {
    // Set up a "caddy" listener on a random port
    let caddy_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let caddy_addr = caddy_listener.local_addr().unwrap().to_string();

    let sni_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let sni_addr = sni_listener.local_addr().unwrap();

    let config = make_test_config(&caddy_addr);
    let bridge_hostname = config.bridge_hostname.clone();
    let registry = Arc::new(TunnelRegistry::new());
    let suffix = format!(".{}", config.subdomain_parent);

    // Spawn a caddy acceptor that reads then closes
    tokio::spawn(async move {
        if let Ok((mut caddy_conn, _)) = caddy_listener.accept().await {
            let mut buf = vec![0u8; 256];
            let _ = caddy_conn.read(&mut buf).await;
            // Connection drops here
        }
    });

    // Connect a client, send a ClientHello with the bridge hostname as SNI
    let mut client = TcpStream::connect(sni_addr).await.unwrap();
    let (server, peer) = sni_listener.accept().await.unwrap();

    let hello = make_client_hello(&bridge_hostname);
    client.write_all(&hello).await.unwrap();
    drop(client);

    let result = route(server, peer, 443, config, registry, &suffix).await;
    // copy_bidirectional finishes when both sides close — that's expected
    assert!(result.is_ok());
}
