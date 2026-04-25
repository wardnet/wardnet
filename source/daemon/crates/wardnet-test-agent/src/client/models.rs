//! Response and error types for client subcommands.

use serde::Serialize;

/// Standard error envelope printed to stdout when a subcommand fails.
#[derive(Debug, Serialize)]
pub struct ClientError {
    /// Human-readable error message.
    pub error: String,
}

impl ClientError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self { error: msg.into() }
    }
}

impl<E: std::fmt::Display> From<E> for ClientError {
    fn from(value: E) -> Self {
        Self::new(value.to_string())
    }
}

// ---------------------------------------------------------------------------
// routes
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct Route {
    pub dst: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gateway: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dev: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub table: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub src: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metric: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct RoutesResponse {
    pub routes: Vec<Route>,
}

// ---------------------------------------------------------------------------
// interfaces
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct InterfaceAddr {
    pub family: String,
    pub local: String,
    pub prefixlen: u8,
}

#[derive(Debug, Serialize)]
pub struct Interface {
    pub name: String,
    pub up: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mac: Option<String>,
    pub mtu: u32,
    pub addrs: Vec<InterfaceAddr>,
}

#[derive(Debug, Serialize)]
pub struct InterfacesResponse {
    pub interfaces: Vec<Interface>,
}

// ---------------------------------------------------------------------------
// dns-resolve
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct DnsResolveResponse {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    pub addrs: Vec<String>,
}

// ---------------------------------------------------------------------------
// ping
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct PingResponse {
    pub target: String,
    pub transmitted: u32,
    pub received: u32,
    pub packet_loss_pct: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rtt_avg_ms: Option<f64>,
}

// ---------------------------------------------------------------------------
// dhcp-renew
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct DhcpRenewResponse {
    pub interface: String,
    pub client: String,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}
