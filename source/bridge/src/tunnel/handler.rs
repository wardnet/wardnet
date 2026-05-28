use std::collections::HashMap;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

use crate::tunnel::registry::TunnelRegistry;

const FRAME_CONNECT: u8 = 0x01;
const FRAME_READY: u8 = 0x02;
const FRAME_DATA: u8 = 0x03;
const FRAME_CLOSE: u8 = 0x04;
const FRAME_PING: u8 = 0x05;
const FRAME_PONG: u8 = 0x06;

/// Run the WebSocket tunnel loop for one Pi connection.
///
/// Registers the Pi in the `registry`, then loops until the WebSocket closes
/// or the Pi disconnects. On exit the install is unregistered.
pub async fn run(ws: WebSocket, install_id: String, name: String, registry: Arc<TunnelRegistry>) {
    let mut forward_rx = registry.register(&install_id, &name);

    // Channel for outgoing WebSocket messages produced by background tasks.
    let (ws_out_tx, ws_out_rx) = mpsc::channel::<Vec<u8>>(256);

    // Channel for data flowing from TCP connections back to the WebSocket.
    // (conn_id, data) — empty data signals EOF.
    let (tcp_out_tx, mut tcp_out_rx) = mpsc::channel::<(u32, Vec<u8>)>(256);

    // Drive WebSocket reads and writes from a single task to avoid borrow issues.
    // The task communicates with the main loop via channels.
    let (from_pi_tx, mut from_pi_rx) = mpsc::channel::<Vec<u8>>(256);
    let ws_out_tx_clone = ws_out_tx.clone();

    let ws_task = tokio::spawn(drive_ws(ws, from_pi_tx, ws_out_rx));

    let mut next_id: u32 = 0;
    // conn_id → sender to the TCP writer task
    let mut active: HashMap<u32, mpsc::Sender<Vec<u8>>> = HashMap::new();
    // conn_id → TcpStream waiting for READY
    let mut pending: HashMap<u32, tokio::net::TcpStream> = HashMap::new();

    loop {
        tokio::select! {
            // Frame arriving from Pi
            frame = from_pi_rx.recv() => {
                let Some(data) = frame else { break; };
                handle_pi_frame(
                    data,
                    &ws_out_tx_clone,
                    &mut active,
                    &mut pending,
                    &tcp_out_tx,
                ).await;
            }

            // New inbound TCP connection from the SNI demuxer
            req = forward_rx.recv() => {
                let Some(req) = req else { break; };
                let conn_id = next_id;
                next_id = next_id.wrapping_add(1);
                let frame = encode_connect(conn_id, req.dest_port);
                let _ = ws_out_tx_clone.send(frame).await;
                pending.insert(conn_id, req.stream);
            }

            // Data or EOF from an active TCP connection
            item = tcp_out_rx.recv() => {
                let Some((conn_id, data)) = item else { break; };
                if data.is_empty() {
                    let _ = ws_out_tx_clone.send(encode_close(conn_id)).await;
                    active.remove(&conn_id);
                } else {
                    let _ = ws_out_tx_clone.send(encode_data(conn_id, &data)).await;
                }
            }
        }
    }

    // Signal the WS task to stop and wait for it.
    drop(ws_out_tx_clone);
    let _ = ws_task.await;
    registry.unregister(&install_id);
}

/// Drives a `WebSocket` to completion, routing frames via channels.
async fn drive_ws(
    mut ws: WebSocket,
    from_pi: mpsc::Sender<Vec<u8>>,
    mut to_pi: mpsc::Receiver<Vec<u8>>,
) {
    loop {
        tokio::select! {
            msg = ws.recv() => {
                let keep_going = match msg {
                    Some(Ok(Message::Binary(data))) => from_pi.send(data.to_vec()).await.is_ok(),
                    None | Some(Ok(Message::Close(_)) | Err(_)) => false,
                    _ => true,
                };
                if !keep_going { break; }
            }
            frame = to_pi.recv() => {
                match frame {
                    Some(f) => { let _ = ws.send(Message::Binary(Bytes::from(f))).await; }
                    None => break,
                }
            }
        }
    }
}

/// Dispatch a binary frame received from the Pi.
async fn handle_pi_frame(
    data: Vec<u8>,
    ws_out: &mpsc::Sender<Vec<u8>>,
    active: &mut HashMap<u32, mpsc::Sender<Vec<u8>>>,
    pending: &mut HashMap<u32, tokio::net::TcpStream>,
    tcp_out: &mpsc::Sender<(u32, Vec<u8>)>,
) {
    if data.len() < 5 {
        return;
    }
    let frame_type = data[0];
    let conn_id = u32::from_be_bytes([data[1], data[2], data[3], data[4]]);

    match frame_type {
        FRAME_READY => {
            if let Some(stream) = pending.remove(&conn_id) {
                let (tcp_tx, tcp_rx) = mpsc::channel::<Vec<u8>>(64);
                active.insert(conn_id, tcp_tx);
                let (read_half, write_half) = stream.into_split();
                let tcp_out_clone = tcp_out.clone();
                tokio::spawn(tcp_reader(conn_id, read_half, tcp_out_clone));
                tokio::spawn(tcp_writer(write_half, tcp_rx));
            }
        }
        FRAME_DATA => {
            if data.len() > 5
                && let Some(tx) = active.get(&conn_id)
            {
                let _ = tx.send(data[5..].to_vec()).await;
            }
        }
        FRAME_CLOSE => {
            active.remove(&conn_id);
        }
        FRAME_PING => {
            let _ = ws_out.send(encode_pong()).await;
        }
        _ => {}
    }
}

async fn tcp_reader(
    conn_id: u32,
    mut reader: tokio::net::tcp::OwnedReadHalf,
    out: mpsc::Sender<(u32, Vec<u8>)>,
) {
    let mut buf = vec![0u8; 16384];
    loop {
        match reader.read(&mut buf).await {
            Ok(0) | Err(_) => {
                let _ = out.send((conn_id, vec![])).await;
                break;
            }
            Ok(n) => {
                if out.send((conn_id, buf[..n].to_vec())).await.is_err() {
                    break;
                }
            }
        }
    }
}

async fn tcp_writer(mut writer: tokio::net::tcp::OwnedWriteHalf, mut rx: mpsc::Receiver<Vec<u8>>) {
    while let Some(data) = rx.recv().await {
        if writer.write_all(&data).await.is_err() {
            break;
        }
    }
}

// ── Frame encoders ────────────────────────────────────────────────────────────

fn encode_connect(conn_id: u32, dest_port: u16) -> Vec<u8> {
    let mut f = Vec::with_capacity(7);
    f.push(FRAME_CONNECT);
    f.extend_from_slice(&conn_id.to_be_bytes());
    f.extend_from_slice(&dest_port.to_be_bytes());
    f
}

fn encode_data(conn_id: u32, data: &[u8]) -> Vec<u8> {
    let mut f = Vec::with_capacity(5 + data.len());
    f.push(FRAME_DATA);
    f.extend_from_slice(&conn_id.to_be_bytes());
    f.extend_from_slice(data);
    f
}

fn encode_close(conn_id: u32) -> Vec<u8> {
    let mut f = Vec::with_capacity(5);
    f.push(FRAME_CLOSE);
    f.extend_from_slice(&conn_id.to_be_bytes());
    f
}

fn encode_pong() -> Vec<u8> {
    let mut f = Vec::with_capacity(5);
    f.push(FRAME_PONG);
    f.extend_from_slice(&0u32.to_be_bytes());
    f
}
