use std::collections::HashMap;

use axum::body::Bytes;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::time::Instant;

use super::*;

// ── Frame encoder tests ───────────────────────────────────────────────────────

#[test]
fn encode_connect_byte_layout() {
    let frame = encode_connect(0x0102_0304, 0x0506);
    assert_eq!(frame.len(), 7);
    assert_eq!(frame[0], FRAME_CONNECT);
    // conn_id big-endian
    assert_eq!(&frame[1..5], &[0x01, 0x02, 0x03, 0x04]);
    // dest_port big-endian
    assert_eq!(&frame[5..7], &[0x05, 0x06]);
}

#[test]
fn encode_data_byte_layout() {
    let payload = b"hello";
    let frame = encode_data(0x0000_0001, payload);
    assert_eq!(frame.len(), 5 + payload.len());
    assert_eq!(frame[0], FRAME_DATA);
    assert_eq!(&frame[1..5], &[0x00, 0x00, 0x00, 0x01]);
    assert_eq!(&frame[5..], b"hello");
}

#[test]
fn encode_close_byte_layout() {
    let frame = encode_close(0xDEAD_BEEF);
    assert_eq!(frame.len(), 5);
    assert_eq!(frame[0], FRAME_CLOSE);
    assert_eq!(&frame[1..5], &[0xDE, 0xAD, 0xBE, 0xEF]);
}

#[test]
fn encode_pong_byte_layout() {
    let frame = encode_pong();
    assert_eq!(frame.len(), 5);
    assert_eq!(frame[0], FRAME_PONG);
    // conn_id must be 0 for PONG
    assert_eq!(&frame[1..5], &[0x00, 0x00, 0x00, 0x00]);
}

// ── handle_pi_frame tests ─────────────────────────────────────────────────────

#[tokio::test]
async fn handle_pi_frame_short_frame_is_noop() {
    let (ws_tx, mut ws_rx) = mpsc::channel::<Vec<u8>>(8);
    let mut active: HashMap<u32, mpsc::Sender<Bytes>> = HashMap::new();
    let mut pending: HashMap<u32, tokio::net::TcpStream> = HashMap::new();
    let mut pending_deadlines: HashMap<u32, Instant> = HashMap::new();
    let (tcp_tx, _tcp_rx) = mpsc::channel::<(u32, Bytes)>(8);

    let short = Bytes::from_static(b"\x05\x00\x00"); // only 3 bytes
    handle_pi_frame(
        short,
        &ws_tx,
        &mut active,
        &mut pending,
        &mut pending_deadlines,
        &tcp_tx,
    )
    .await;

    // Nothing should have been sent
    assert!(
        ws_rx.try_recv().is_err(),
        "no output expected for short frame"
    );
    assert!(active.is_empty());
    assert!(pending.is_empty());
}

#[tokio::test]
async fn handle_pi_frame_ping_with_conn_id_zero_sends_pong() {
    let (ws_tx, mut ws_rx) = mpsc::channel::<Vec<u8>>(8);
    let mut active: HashMap<u32, mpsc::Sender<Bytes>> = HashMap::new();
    let mut pending: HashMap<u32, tokio::net::TcpStream> = HashMap::new();
    let mut pending_deadlines: HashMap<u32, Instant> = HashMap::new();
    let (tcp_tx, _tcp_rx) = mpsc::channel::<(u32, Bytes)>(8);

    // FRAME_PING with conn_id = 0
    let ping = Bytes::from_static(&[FRAME_PING, 0x00, 0x00, 0x00, 0x00]);
    handle_pi_frame(
        ping,
        &ws_tx,
        &mut active,
        &mut pending,
        &mut pending_deadlines,
        &tcp_tx,
    )
    .await;

    let pong_frame = ws_rx.try_recv().expect("PONG should have been sent");
    assert_eq!(pong_frame[0], FRAME_PONG);
    assert_eq!(&pong_frame[1..5], &[0x00, 0x00, 0x00, 0x00]);
}

#[tokio::test]
async fn handle_pi_frame_ping_with_nonzero_conn_id_is_noop() {
    let (ws_tx, mut ws_rx) = mpsc::channel::<Vec<u8>>(8);
    let mut active: HashMap<u32, mpsc::Sender<Bytes>> = HashMap::new();
    let mut pending: HashMap<u32, tokio::net::TcpStream> = HashMap::new();
    let mut pending_deadlines: HashMap<u32, Instant> = HashMap::new();
    let (tcp_tx, _tcp_rx) = mpsc::channel::<(u32, Bytes)>(8);

    // FRAME_PING with conn_id = 1 — invalid per protocol, should be ignored
    let ping = Bytes::from_static(&[FRAME_PING, 0x00, 0x00, 0x00, 0x01]);
    handle_pi_frame(
        ping,
        &ws_tx,
        &mut active,
        &mut pending,
        &mut pending_deadlines,
        &tcp_tx,
    )
    .await;

    assert!(
        ws_rx.try_recv().is_err(),
        "no output expected for non-zero PING"
    );
}

#[tokio::test]
async fn handle_pi_frame_close_removes_from_active() {
    let (ws_tx, _ws_rx) = mpsc::channel::<Vec<u8>>(8);
    let mut active: HashMap<u32, mpsc::Sender<Bytes>> = HashMap::new();
    let mut pending: HashMap<u32, tokio::net::TcpStream> = HashMap::new();
    let mut pending_deadlines: HashMap<u32, Instant> = HashMap::new();
    let (tcp_tx, _tcp_rx) = mpsc::channel::<(u32, Bytes)>(8);

    // Insert a dummy active entry for conn_id 7
    let (data_tx, _data_rx) = mpsc::channel::<Bytes>(4);
    active.insert(7, data_tx);

    let close_frame = Bytes::from(vec![FRAME_CLOSE, 0x00, 0x00, 0x00, 0x07]);
    handle_pi_frame(
        close_frame,
        &ws_tx,
        &mut active,
        &mut pending,
        &mut pending_deadlines,
        &tcp_tx,
    )
    .await;

    assert!(
        !active.contains_key(&7),
        "conn_id 7 should have been removed from active"
    );
}

#[tokio::test]
async fn handle_pi_frame_data_forwards_payload_to_active_sender() {
    let (ws_tx, _ws_rx) = mpsc::channel::<Vec<u8>>(8);
    let mut active: HashMap<u32, mpsc::Sender<Bytes>> = HashMap::new();
    let mut pending: HashMap<u32, tokio::net::TcpStream> = HashMap::new();
    let mut pending_deadlines: HashMap<u32, Instant> = HashMap::new();
    let (tcp_tx, _tcp_rx) = mpsc::channel::<(u32, Bytes)>(8);

    let (data_tx, mut data_rx) = mpsc::channel::<Bytes>(4);
    active.insert(42, data_tx);

    // FRAME_DATA for conn_id=42, payload = b"world"
    let mut frame = vec![FRAME_DATA, 0x00, 0x00, 0x00, 0x2A]; // 0x2A = 42
    frame.extend_from_slice(b"world");
    handle_pi_frame(
        Bytes::from(frame),
        &ws_tx,
        &mut active,
        &mut pending,
        &mut pending_deadlines,
        &tcp_tx,
    )
    .await;

    let received = data_rx.try_recv().expect("data should have been forwarded");
    assert_eq!(received.as_ref(), b"world");
}

#[tokio::test]
async fn handle_pi_frame_data_for_unknown_conn_id_is_noop() {
    let (ws_tx, mut ws_rx) = mpsc::channel::<Vec<u8>>(8);
    let mut active: HashMap<u32, mpsc::Sender<Bytes>> = HashMap::new();
    let mut pending: HashMap<u32, tokio::net::TcpStream> = HashMap::new();
    let mut pending_deadlines: HashMap<u32, Instant> = HashMap::new();
    let (tcp_tx, _tcp_rx) = mpsc::channel::<(u32, Bytes)>(8);

    let mut frame = vec![FRAME_DATA, 0x00, 0x00, 0x00, 0x63]; // conn_id = 99, not in active
    frame.extend_from_slice(b"data");
    handle_pi_frame(
        Bytes::from(frame),
        &ws_tx,
        &mut active,
        &mut pending,
        &mut pending_deadlines,
        &tcp_tx,
    )
    .await;

    assert!(
        ws_rx.try_recv().is_err(),
        "no WS output for unknown conn_id DATA"
    );
}

#[tokio::test]
async fn handle_pi_frame_ready_moves_pending_to_active() {
    let (ws_tx, _ws_rx) = mpsc::channel::<Vec<u8>>(8);
    let mut active: HashMap<u32, mpsc::Sender<Bytes>> = HashMap::new();
    let mut pending: HashMap<u32, tokio::net::TcpStream> = HashMap::new();
    let mut pending_deadlines: HashMap<u32, Instant> = HashMap::new();
    let (tcp_tx, _tcp_rx) = mpsc::channel::<(u32, Bytes)>(8);

    // Set up a real TCP pair
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let stream = TcpStream::connect(addr).await.unwrap();
    let _server_side = listener.accept().await.unwrap();

    let conn_id: u32 = 5;
    pending.insert(conn_id, stream);
    pending_deadlines.insert(conn_id, Instant::now() + std::time::Duration::from_secs(10));

    let ready_frame = Bytes::from(vec![FRAME_READY, 0x00, 0x00, 0x00, 0x05]);
    handle_pi_frame(
        ready_frame,
        &ws_tx,
        &mut active,
        &mut pending,
        &mut pending_deadlines,
        &tcp_tx,
    )
    .await;

    // Stream should have moved from pending to active
    assert!(
        !pending.contains_key(&conn_id),
        "pending should be cleared after READY"
    );
    assert!(
        !pending_deadlines.contains_key(&conn_id),
        "deadline should be removed after READY"
    );
    assert!(
        active.contains_key(&conn_id),
        "conn_id should now be in active"
    );
}

// ── tcp_reader tests ──────────────────────────────────────────────────────────

#[tokio::test]
async fn tcp_reader_sends_data_and_eof() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Connect from a client side
    let client = TcpStream::connect(addr).await.unwrap();
    let (mut server, _) = listener.accept().await.unwrap();

    let (out_tx, mut out_rx) = mpsc::channel::<(u32, Bytes)>(16);
    let (read_half, _write_half) = client.into_split();

    let conn_id: u32 = 99;
    tokio::spawn(tcp_reader(conn_id, read_half, out_tx));

    // Write known bytes from the server side and then close the connection
    server.write_all(b"hello reader").await.unwrap();
    // Give the reader task a moment to receive the data
    tokio::task::yield_now().await;

    // Read the data message
    let (cid, data) = out_rx.recv().await.expect("should receive data");
    assert_eq!(cid, conn_id);
    assert_eq!(data.as_ref(), b"hello reader");

    // Close the server side to trigger EOF
    drop(server);

    // Should receive empty EOF message
    let (cid_eof, eof_data) = out_rx.recv().await.expect("should receive EOF signal");
    assert_eq!(cid_eof, conn_id);
    assert!(eof_data.is_empty(), "EOF signal should be empty Bytes");
}

// ── tcp_writer tests ──────────────────────────────────────────────────────────

#[tokio::test]
async fn tcp_writer_sends_bytes_to_socket() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let client = TcpStream::connect(addr).await.unwrap();
    let (mut server, _) = listener.accept().await.unwrap();

    let (_read_half, write_half) = client.into_split();
    let (tx, rx) = mpsc::channel::<Bytes>(8);

    tokio::spawn(tcp_writer(write_half, rx));

    tx.send(Bytes::from_static(b"ping pong")).await.unwrap();
    // Drop sender so the writer task exits cleanly
    drop(tx);

    let mut buf = vec![0u8; 64];
    let n = server.read(&mut buf).await.unwrap();
    assert_eq!(&buf[..n], b"ping pong");
}
