//! Raw ARP send/capture endpoints used by the GARP-failover e2e
//! (issue #213).
//!
//! `/arp/send` builds and emits ARP frames. The caller fully controls
//! every field so the same handler is used to:
//! - drive passive learning of `garp_router_mac` (a single
//!   gratuitous-reply from a synthetic router MAC), and
//! - emit any future ARP-shaped traffic the e2e suite needs without
//!   adding a new endpoint each time.
//!
//! `/arp/capture` opens a pnet datalink channel for a bounded window
//! and returns every parsed ARP frame seen during it. The e2e uses
//! it to assert the GARP claim/farewell pulses arrive on the LAN
//! bridge.
//!
//! Both endpoints run inside `spawn_blocking` because pnet's
//! datalink channel is synchronous.

use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

use axum::Json;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use pnet::datalink::{self, Channel, Config, NetworkInterface};
use pnet::packet::Packet;
use pnet::packet::arp::{ArpHardwareTypes, ArpOperation, ArpPacket, MutableArpPacket};
use pnet::packet::ethernet::{EtherTypes, EthernetPacket, MutableEthernetPacket};
use pnet::util::MacAddr;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::models::ClientError;

const READ_POLL: Duration = Duration::from_millis(100);

#[derive(Debug, Deserialize)]
pub struct ArpSendRequest {
    /// ARP opcode. Defaults to 2 (reply).
    #[serde(default = "default_opcode")]
    pub opcode: u16,
    /// Sender hardware address (`AA:BB:CC:DD:EE:FF`).
    pub sender_mac: String,
    /// Sender protocol (IPv4) address.
    pub sender_ip: String,
    /// Target hardware address. Defaults to broadcast for gratuitous
    /// announcements.
    #[serde(default = "default_broadcast_mac")]
    pub target_mac: String,
    /// Target protocol (IPv4) address.
    pub target_ip: String,
    /// Number of pulses to send. Defaults to 1.
    #[serde(default = "default_count")]
    pub count: u32,
    /// Milliseconds to sleep between pulses. Defaults to 0 (no gap).
    #[serde(default)]
    pub interval_ms: u64,
    /// Network interface to send from. Defaults to "eth0".
    #[serde(default = "default_interface")]
    pub interface: String,
}

fn default_opcode() -> u16 {
    2
}
fn default_broadcast_mac() -> String {
    "ff:ff:ff:ff:ff:ff".to_owned()
}
fn default_count() -> u32 {
    1
}
fn default_interface() -> String {
    "eth0".to_owned()
}

#[derive(Debug, Deserialize)]
pub struct ArpCaptureRequest {
    /// How long to capture for, in milliseconds.
    pub duration_ms: u64,
    /// Network interface to capture on. Defaults to "eth0".
    #[serde(default = "default_interface")]
    pub interface: String,
    /// Optional filter applied to each frame. When set, only frames
    /// matching every populated field are returned.
    #[serde(default)]
    pub filter: Option<ArpCaptureFilter>,
}

#[derive(Debug, Deserialize, Default)]
pub struct ArpCaptureFilter {
    pub sender_ip: Option<String>,
    pub sender_mac: Option<String>,
    pub opcode: Option<u16>,
}

#[derive(Debug, Serialize)]
pub struct CapturedArpFrame {
    pub opcode: u16,
    pub sender_ip: String,
    pub sender_mac: String,
    pub target_ip: String,
    pub target_mac: String,
    /// Milliseconds since the capture window started.
    pub timestamp_ms: u128,
    /// Ethernet source / destination — useful to assert the L2
    /// headers also flipped on a GARP failover (decision 4).
    pub eth_source: String,
    pub eth_destination: String,
}

/// `POST /arp/send`
///
/// Returns `200` with an empty JSON object on success — every other
/// client-serve endpoint also returns JSON, and the e2e helper
/// (`agentPost`) unconditionally `res.json()`s the body.
pub async fn post_arp_send(Json(req): Json<ArpSendRequest>) -> impl IntoResponse {
    let result = tokio::task::spawn_blocking(move || run_send(&req))
        .await
        .map_err(|e| anyhow::anyhow!("arp send blocking task panicked: {e}"));

    match result {
        Ok(Ok(())) => Json(json!({})).into_response(),
        Ok(Err(e)) | Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ClientError::new(e.to_string())),
        )
            .into_response(),
    }
}

/// `POST /arp/capture`
pub async fn post_arp_capture(Json(req): Json<ArpCaptureRequest>) -> impl IntoResponse {
    let result = tokio::task::spawn_blocking(move || run_capture(req))
        .await
        .map_err(|e| anyhow::anyhow!("arp capture blocking task panicked: {e}"));

    match result {
        Ok(Ok(frames)) => Json(frames).into_response(),
        Ok(Err(e)) | Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ClientError::new(e.to_string())),
        )
            .into_response(),
    }
}

fn run_send(req: &ArpSendRequest) -> anyhow::Result<()> {
    let sender_mac: MacAddr = req
        .sender_mac
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid sender_mac {}: {e}", req.sender_mac))?;
    let target_mac: MacAddr = req
        .target_mac
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid target_mac {}: {e}", req.target_mac))?;
    let sender_ip: Ipv4Addr = req
        .sender_ip
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid sender_ip {}: {e}", req.sender_ip))?;
    let target_ip: Ipv4Addr = req
        .target_ip
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid target_ip {}: {e}", req.target_ip))?;

    let frame = build_arp_frame(req.opcode, sender_mac, sender_ip, target_mac, target_ip)
        .ok_or_else(|| anyhow::anyhow!("failed to build ARP frame"))?;

    let iface = find_interface(&req.interface)?;
    let Channel::Ethernet(mut tx, _rx) = datalink::channel(&iface, Config::default())
        .map_err(|e| anyhow::anyhow!("datalink channel failed on {}: {e}", req.interface))?
    else {
        anyhow::bail!("unsupported channel type for {}", req.interface);
    };

    for pulse in 0..req.count {
        match tx.send_to(&frame, None) {
            Some(Ok(())) => {}
            Some(Err(e)) => anyhow::bail!("send failed at pulse {}: {e}", pulse + 1),
            None => anyhow::bail!("datalink rejected pulse {}", pulse + 1),
        }
        if pulse + 1 < req.count && req.interval_ms > 0 {
            std::thread::sleep(Duration::from_millis(req.interval_ms));
        }
    }

    Ok(())
}

fn run_capture(req: ArpCaptureRequest) -> anyhow::Result<Vec<CapturedArpFrame>> {
    let iface = find_interface(&req.interface)?;
    let config = Config {
        read_timeout: Some(READ_POLL),
        ..Config::default()
    };
    let Channel::Ethernet(_tx, mut rx) = datalink::channel(&iface, config)
        .map_err(|e| anyhow::anyhow!("datalink channel failed on {}: {e}", req.interface))?
    else {
        anyhow::bail!("unsupported channel type for {}", req.interface);
    };

    let filter = req.filter.unwrap_or_default();
    let filter_sender_ip: Option<Ipv4Addr> = filter
        .sender_ip
        .as_deref()
        .map(str::parse)
        .transpose()
        .map_err(|e| anyhow::anyhow!("invalid filter.sender_ip: {e}"))?;
    let filter_sender_mac: Option<MacAddr> = filter
        .sender_mac
        .as_deref()
        .map(str::parse)
        .transpose()
        .map_err(|e| anyhow::anyhow!("invalid filter.sender_mac: {e}"))?;

    let started = Instant::now();
    let deadline = started + Duration::from_millis(req.duration_ms);
    let mut out = Vec::new();
    while Instant::now() < deadline {
        match rx.next() {
            Ok(frame) => {
                if let Some(captured) = parse_arp(frame, started)
                    && passes_filter(
                        &captured,
                        filter_sender_ip,
                        filter_sender_mac,
                        filter.opcode,
                    )
                {
                    out.push(captured);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) => return Err(anyhow::anyhow!("ARP capture read failed: {e}")),
        }
    }
    Ok(out)
}

fn passes_filter(
    frame: &CapturedArpFrame,
    sender_ip: Option<Ipv4Addr>,
    sender_mac: Option<MacAddr>,
    opcode: Option<u16>,
) -> bool {
    if let Some(ip) = sender_ip
        && frame.sender_ip != ip.to_string()
    {
        return false;
    }
    if let Some(mac) = sender_mac
        && frame.sender_mac.to_lowercase() != mac.to_string().to_lowercase()
    {
        return false;
    }
    if let Some(op) = opcode
        && frame.opcode != op
    {
        return false;
    }
    true
}

fn parse_arp(bytes: &[u8], started: Instant) -> Option<CapturedArpFrame> {
    let eth = EthernetPacket::new(bytes)?;
    if eth.get_ethertype() != EtherTypes::Arp {
        return None;
    }
    let arp = ArpPacket::new(eth.payload())?;
    Some(CapturedArpFrame {
        opcode: arp.get_operation().0,
        sender_ip: arp.get_sender_proto_addr().to_string(),
        sender_mac: arp.get_sender_hw_addr().to_string(),
        target_ip: arp.get_target_proto_addr().to_string(),
        target_mac: arp.get_target_hw_addr().to_string(),
        timestamp_ms: started.elapsed().as_millis(),
        eth_source: eth.get_source().to_string(),
        eth_destination: eth.get_destination().to_string(),
    })
}

fn build_arp_frame(
    opcode: u16,
    sender_mac: MacAddr,
    sender_ip: Ipv4Addr,
    target_mac: MacAddr,
    target_ip: Ipv4Addr,
) -> Option<Vec<u8>> {
    let mut buf = vec![0u8; 42];
    {
        let mut eth = MutableEthernetPacket::new(&mut buf)?;
        // Ethernet dest follows the ARP target HW: broadcast for
        // gratuitous announcements, the actual MAC for unicast — same
        // value either way, so we just mirror it.
        eth.set_destination(target_mac);
        eth.set_source(sender_mac);
        eth.set_ethertype(EtherTypes::Arp);
    }
    {
        let mut arp = MutableArpPacket::new(&mut buf[14..])?;
        arp.set_hardware_type(ArpHardwareTypes::Ethernet);
        arp.set_protocol_type(EtherTypes::Ipv4);
        arp.set_hw_addr_len(6);
        arp.set_proto_addr_len(4);
        arp.set_operation(ArpOperation::new(opcode));
        arp.set_sender_hw_addr(sender_mac);
        arp.set_sender_proto_addr(sender_ip);
        arp.set_target_hw_addr(target_mac);
        arp.set_target_proto_addr(target_ip);
    }
    Some(buf)
}

fn find_interface(name: &str) -> anyhow::Result<NetworkInterface> {
    datalink::interfaces()
        .into_iter()
        .find(|iface| iface.name == name)
        .ok_or_else(|| anyhow::anyhow!("interface '{name}' not found"))
}
