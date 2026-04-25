//! Request and response types for the test agent API.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

/// Response for the `GET /health` endpoint.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    /// Always `"ok"` when the agent is running.
    pub status: &'static str,
}

// ---------------------------------------------------------------------------
// Pid
// ---------------------------------------------------------------------------

/// Response for the `GET /pid` endpoint.
#[derive(Debug, Serialize)]
pub struct PidResponse {
    /// Process id read from the daemon pidfile.
    pub pid: i32,
    /// Whether `/proc/<pid>` exists (i.e. the process is alive).
    pub running: bool,
}

// ---------------------------------------------------------------------------
// ip rule
// ---------------------------------------------------------------------------

/// A single parsed entry from `ip rule list`.
#[derive(Debug, Serialize)]
pub struct IpRule {
    /// Rule priority (the number before the colon).
    pub priority: u32,
    /// The `from` selector (e.g. `"10.232.1.210/32"` or `"all"`).
    pub from: String,
    /// The routing table name or number.
    pub table: String,
}

/// Response for the `GET /ip-rules` endpoint.
#[derive(Debug, Serialize)]
pub struct IpRulesResponse {
    /// Parsed rules extracted from the raw output.
    pub rules: Vec<IpRule>,
    /// Unmodified output of `ip rule list` for debugging.
    pub raw: String,
}

// ---------------------------------------------------------------------------
// nft
// ---------------------------------------------------------------------------

/// Response for the `GET /nft-rules` endpoint.
#[derive(Debug, Serialize)]
pub struct NftRulesResponse {
    /// Unmodified output of `nft list ruleset`.
    pub raw: String,
    /// Table names found in the ruleset (e.g. `["inet wardnet"]`).
    pub tables: Vec<String>,
    /// Interface names that have a masquerade rule (`oifname "wg_*" masquerade`).
    pub has_masquerade_for: Vec<String>,
}

// ---------------------------------------------------------------------------
// WireGuard
// ---------------------------------------------------------------------------

/// A single `WireGuard` peer parsed from `wg show`.
#[derive(Debug, Serialize)]
pub struct WgPeer {
    /// The peer's public key.
    pub public_key: String,
    /// The peer's endpoint address (if known).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// Allowed IP ranges for this peer.
    pub allowed_ips: Vec<String>,
    /// Time of the latest handshake (human-readable string from `wg`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_handshake: Option<String>,
    /// Bytes received.
    pub transfer_rx: u64,
    /// Bytes transmitted.
    pub transfer_tx: u64,
}

/// Response for the `GET /wg/:interface` endpoint.
#[derive(Debug, Serialize)]
pub struct WgShowResponse {
    /// The queried interface name.
    pub interface: String,
    /// Whether the interface exists.
    pub exists: bool,
    /// The interface's public key (present only when the interface exists).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    /// The interface's listening port (present only when the interface exists).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub listening_port: Option<u16>,
    /// Connected peers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peers: Option<Vec<WgPeer>>,
}

// ---------------------------------------------------------------------------
// ip link
// ---------------------------------------------------------------------------

/// Response for the `GET /link/:interface` endpoint.
#[derive(Debug, Serialize)]
pub struct LinkShowResponse {
    /// The queried interface name.
    pub name: String,
    /// Whether the interface exists.
    pub exists: bool,
    /// Whether the interface is UP.
    pub up: bool,
    /// The interface MTU (0 when the interface does not exist).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mtu: Option<u32>,
}

// ---------------------------------------------------------------------------
// Container exec
// ---------------------------------------------------------------------------

/// Request body for the `POST /container/exec` endpoint.
#[derive(Debug, Deserialize)]
pub struct ContainerExecRequest {
    /// Name of the container to execute the command in.
    pub container: String,
    /// The command and its arguments.
    pub command: Vec<String>,
}

/// Response for the `POST /container/exec` endpoint.
#[derive(Debug, Serialize)]
pub struct ContainerExecResponse {
    /// Process exit code.
    pub exit_code: i32,
    /// Captured standard output.
    pub stdout: String,
    /// Captured standard error.
    pub stderr: String,
}

// ---------------------------------------------------------------------------
// Shared error
// ---------------------------------------------------------------------------

/// Generic JSON error response returned on failures.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    /// Human-readable error message.
    pub error: String,
}
