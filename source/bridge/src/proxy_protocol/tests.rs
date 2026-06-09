use std::net::SocketAddr;

use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};

use super::{Inspected, parse_v1, read_optional, read_required};

#[test]
fn parse_tcp4() {
    let addr = parse_v1(b"PROXY TCP4 1.2.3.4 5.6.7.8 1111 443\r\n")
        .unwrap()
        .unwrap();
    assert_eq!(addr, "1.2.3.4:1111".parse::<SocketAddr>().unwrap());
}

#[test]
fn parse_tcp6() {
    let addr = parse_v1(b"PROXY TCP6 2001:db8::1 2001:db8::2 4040 443\r\n")
        .unwrap()
        .unwrap();
    assert_eq!(addr, "[2001:db8::1]:4040".parse::<SocketAddr>().unwrap());
}

#[test]
fn parse_unknown_yields_no_addr() {
    assert_eq!(parse_v1(b"PROXY UNKNOWN\r\n").unwrap(), None);
}

#[test]
fn parse_rejects_non_crlf() {
    assert!(parse_v1(b"PROXY TCP4 1.2.3.4 5.6.7.8 1111 443").is_err());
}

#[test]
fn parse_rejects_garbage() {
    assert!(parse_v1(b"GET / HTTP/1.1\r\n").is_err());
}

/// `read_required` consumes **exactly** the header line — the bytes that follow
/// (here a fake TLS `ClientHello`) must be readable untouched afterwards.
#[tokio::test]
async fn read_required_does_not_over_read() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let payload = b"\x16\x03\x01\x00\x05HELLO"; // bytes after the header
    let client = tokio::spawn(async move {
        let mut s = TcpStream::connect(addr).await.unwrap();
        s.write_all(b"PROXY TCP4 9.9.9.9 1.1.1.1 5555 443\r\n")
            .await
            .unwrap();
        s.write_all(payload).await.unwrap();
        s.flush().await.unwrap();
        // keep the connection open until the server has read
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    });

    let (mut srv, _) = listener.accept().await.unwrap();
    let client_addr = read_required(&mut srv).await.unwrap().unwrap();
    assert_eq!(client_addr, "9.9.9.9:5555".parse::<SocketAddr>().unwrap());

    let mut rest = vec![0u8; payload.len()];
    srv.read_exact(&mut rest).await.unwrap();
    assert_eq!(&rest, payload, "ClientHello bytes must survive intact");

    client.await.unwrap();
}

/// `read_optional` returns `Direct` (consuming nothing) for a connection with no
/// PROXY header — the direct/local health-probe case.
#[tokio::test]
async fn read_optional_direct_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let payload = b"GET /health HTTP/1.1\r\n\r\n";
    let client = tokio::spawn(async move {
        let mut s = TcpStream::connect(addr).await.unwrap();
        s.write_all(payload).await.unwrap();
        s.flush().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    });

    let (mut srv, _) = listener.accept().await.unwrap();
    assert_eq!(read_optional(&mut srv).await.unwrap(), Inspected::Direct);

    // nothing was consumed — the request line is still there
    let mut rest = vec![0u8; payload.len()];
    srv.read_exact(&mut rest).await.unwrap();
    assert_eq!(&rest, payload);

    client.await.unwrap();
}

/// `read_optional` parses and consumes a present header.
#[tokio::test]
async fn read_optional_with_header() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let client = tokio::spawn(async move {
        let mut s = TcpStream::connect(addr).await.unwrap();
        s.write_all(b"PROXY TCP4 8.8.8.8 1.1.1.1 6000 80\r\nGET / HTTP/1.1\r\n")
            .await
            .unwrap();
        s.flush().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    });

    let (mut srv, _) = listener.accept().await.unwrap();
    match read_optional(&mut srv).await.unwrap() {
        Inspected::Header(Some(a)) => {
            assert_eq!(a, "8.8.8.8:6000".parse::<SocketAddr>().unwrap());
        }
        other => panic!("expected parsed header, got {other:?}"),
    }
    client.await.unwrap();
}
