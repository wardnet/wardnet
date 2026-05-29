use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::time::Instant;
use tracing::Instrument as _;

use crate::tunnel::registry::TunnelRegistry;

const FRAME_CONNECT: u8 = 0x01;
const FRAME_READY: u8 = 0x02;
const FRAME_DATA: u8 = 0x03;
const FRAME_CLOSE: u8 = 0x04;
const FRAME_PING: u8 = 0x05;
const FRAME_PONG: u8 = 0x06;

/// Maximum number of inbound connections waiting for a Pi READY response.
/// Caps FD usage: connections beyond this limit are dropped immediately.
const MAX_PENDING: usize = 256;
/// Time to wait for a Pi READY response before closing the pending TCP stream.
const PENDING_TIMEOUT: Duration = Duration::from_secs(10);
/// Interval at which the bridge sends a WS-level Ping to detect dead tunnels.
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);
/// Close the tunnel if no message is received from the Pi within this window.
const IDLE_TIMEOUT: Duration = Duration::from_secs(90);

/// Run the WebSocket tunnel loop for one Pi connection.
///
/// Registers the Pi in the `registry`, then loops until the WebSocket closes
/// or the Pi disconnects. On exit the install is unregistered.
pub async fn run(ws: WebSocket, install_id: String, name: String, registry: Arc<TunnelRegistry>) {
    let mut forward_rx = registry.register(&install_id, &name);

    // Outgoing WebSocket frames (bridge → Pi).
    let (ws_out_tx, ws_out_rx) = mpsc::channel::<Vec<u8>>(256);
    // TCP reader → main loop: (conn_id, data); empty Bytes signals EOF.
    let (tcp_out_tx, mut tcp_out_rx) = mpsc::channel::<(u32, Bytes)>(256);
    // drive_ws → main loop: inbound binary frames from Pi.
    let (from_pi_tx, mut from_pi_rx) = mpsc::channel::<Bytes>(256);
    let ws_out_tx_clone = ws_out_tx.clone();

    let span = tracing::Span::current();
    let ws_task = tokio::spawn(drive_ws(ws, from_pi_tx, ws_out_rx).instrument(span));

    let mut next_id: u32 = 0;
    // conn_id → sender to the TCP writer task
    let mut active: HashMap<u32, mpsc::Sender<Bytes>> = HashMap::new();
    // conn_id → TcpStream waiting for READY
    let mut pending: HashMap<u32, tokio::net::TcpStream> = HashMap::new();
    // conn_id → deadline by which READY must arrive
    let mut pending_deadlines: HashMap<u32, Instant> = HashMap::new();

    let mut cleanup_interval = tokio::time::interval(Duration::from_secs(5));
    cleanup_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // Skip the immediate first tick so cleanup doesn't fire before any work.
    cleanup_interval.reset();

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
                    &mut pending_deadlines,
                    &tcp_out_tx,
                ).await;
            }

            // New inbound TCP connection from the SNI demuxer
            req = forward_rx.recv() => {
                let Some(req) = req else { break; };
                let conn_id = next_id;
                next_id = next_id.wrapping_add(1);

                // On u32 wrap-around, close any displaced pending entry gracefully.
                if let Some(_old) = pending.remove(&conn_id) {
                    pending_deadlines.remove(&conn_id);
                    tracing::warn!(conn_id, "conn_id wrapped, closing displaced pending entry");
                }

                // Hard cap: drop the connection if the pending map is full.
                if pending.len() >= MAX_PENDING {
                    tracing::debug!(conn_id, "pending map at capacity, dropping inbound connection");
                    // req.stream is dropped here, closing the TCP connection.
                    continue;
                }

                let frame = encode_connect(conn_id, req.dest_port);
                if ws_out_tx_clone.send(frame).await.is_ok() {
                    pending.insert(conn_id, req.stream);
                    pending_deadlines.insert(conn_id, Instant::now() + PENDING_TIMEOUT);
                }
                // If the send fails, the ws task has already closed; the loop
                // will exit on the next from_pi_rx or forward_rx recv.
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

            // Periodic sweep: drop pending entries whose READY deadline has passed.
            _ = cleanup_interval.tick() => {
                let now = Instant::now();
                let expired: Vec<u32> = pending_deadlines
                    .iter()
                    .filter(|(_, deadline)| now >= **deadline)
                    .map(|(id, _)| *id)
                    .collect();
                for conn_id in expired {
                    pending.remove(&conn_id);
                    pending_deadlines.remove(&conn_id);
                    tracing::debug!(conn_id, "READY timed out, dropping pending connection");
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
///
/// Sends a WS-level Ping every [`KEEPALIVE_INTERVAL`] seconds and closes the
/// tunnel if no message arrives within [`IDLE_TIMEOUT`] seconds.
async fn drive_ws(
    mut ws: WebSocket,
    from_pi: mpsc::Sender<Bytes>,
    mut to_pi: mpsc::Receiver<Vec<u8>>,
) {
    let mut keepalive =
        tokio::time::interval_at(Instant::now() + KEEPALIVE_INTERVAL, KEEPALIVE_INTERVAL);
    keepalive.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            result = tokio::time::timeout(IDLE_TIMEOUT, ws.recv()) => {
                let keep_going = match result {
                    Err(_) => {
                        tracing::debug!("tunnel idle timeout, closing");
                        false
                    }
                    Ok(msg) => match msg {
                        Some(Ok(Message::Binary(data))) => from_pi.send(data).await.is_ok(),
                        None | Some(Ok(Message::Close(_)) | Err(_)) => false,
                        _ => true,
                    },
                };
                if !keep_going { break; }
            }
            frame = to_pi.recv() => {
                match frame {
                    Some(f) => { let _ = ws.send(Message::Binary(Bytes::from(f))).await; }
                    None => break,
                }
            }
            _ = keepalive.tick() => {
                if ws.send(Message::Ping(Bytes::new())).await.is_err() {
                    break;
                }
            }
        }
    }
}

/// Dispatch a binary frame received from the Pi.
async fn handle_pi_frame(
    data: Bytes,
    ws_out: &mpsc::Sender<Vec<u8>>,
    active: &mut HashMap<u32, mpsc::Sender<Bytes>>,
    pending: &mut HashMap<u32, tokio::net::TcpStream>,
    pending_deadlines: &mut HashMap<u32, Instant>,
    tcp_out: &mpsc::Sender<(u32, Bytes)>,
) {
    if data.len() < 5 {
        return;
    }
    let frame_type = data[0];
    let conn_id = u32::from_be_bytes([data[1], data[2], data[3], data[4]]);

    match frame_type {
        FRAME_READY => {
            pending_deadlines.remove(&conn_id);
            if let Some(stream) = pending.remove(&conn_id) {
                let (tcp_tx, tcp_rx) = mpsc::channel::<Bytes>(64);
                active.insert(conn_id, tcp_tx);
                let (read_half, write_half) = stream.into_split();
                let tcp_out_clone = tcp_out.clone();
                let span = tracing::Span::current();
                tokio::spawn(
                    tcp_reader(conn_id, read_half, tcp_out_clone).instrument(span.clone()),
                );
                tokio::spawn(tcp_writer(write_half, tcp_rx).instrument(span));
            }
        }
        FRAME_DATA => {
            if data.len() > 5
                && let Some(tx) = active.get(&conn_id)
            {
                let _ = tx.send(data.slice(5..)).await;
            }
        }
        FRAME_CLOSE => {
            active.remove(&conn_id);
        }
        // The protocol specifies conn_id=0 for PING frames.
        FRAME_PING if conn_id == 0 => {
            let _ = ws_out.send(encode_pong()).await;
        }
        _ => {}
    }
}

async fn tcp_reader(
    conn_id: u32,
    mut reader: tokio::net::tcp::OwnedReadHalf,
    out: mpsc::Sender<(u32, Bytes)>,
) {
    let mut buf = vec![0u8; 16384];
    loop {
        match reader.read(&mut buf).await {
            Ok(0) | Err(_) => {
                let _ = out.send((conn_id, Bytes::new())).await;
                break;
            }
            Ok(n) => {
                if out
                    .send((conn_id, Bytes::copy_from_slice(&buf[..n])))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        }
    }
}

async fn tcp_writer(mut writer: tokio::net::tcp::OwnedWriteHalf, mut rx: mpsc::Receiver<Bytes>) {
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

#[cfg(test)]
mod tests;
