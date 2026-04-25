//! Handlers that expose kernel networking state (`ip rule`, `nft`, `wg`, `ip link`).

use axum::Json;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use regex::Regex;
use std::sync::LazyLock;
use tokio::process::Command;
use tracing::warn;

use crate::server::models::{
    ErrorResponse, IpRule, IpRulesResponse, LinkShowResponse, NftRulesResponse, WgPeer,
    WgShowResponse,
};

/// Regex for parsing a single line of `ip rule list` output.
///
/// Captures: priority, from-selector, table name/number.
/// Example line: `100:    from 10.232.1.210/32 lookup 100`
static IP_RULE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^(\d+):\s+from\s+(\S+)\s+lookup\s+(\S+)").expect("ip rule regex is valid")
});

/// Regex for extracting table declarations from `nft list ruleset`.
///
/// Example: `table inet wardnet {`
static NFT_TABLE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^table\s+(\S+\s+\S+)\s+\{").expect("nft table regex is valid")
});

/// Regex for detecting masquerade rules tied to `WireGuard` interfaces.
///
/// Example: `oifname "wg_ward0" masquerade`
static NFT_MASQ_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"oifname\s+"(wg_[^"]+)"\s+masquerade"#).expect("nft masquerade regex is valid")
});

/// Characters allowed in interface names (prevent command injection).
fn is_valid_interface_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 15
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Returns a `400 Bad Request` JSON response for an invalid interface name.
fn bad_interface(name: &str) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: format!("invalid interface name: {name}"),
        }),
    )
}

/// Returns a `500 Internal Server Error` JSON response.
fn internal_error(msg: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse { error: msg.into() }),
    )
}

/// `GET /ip-rules` -- runs `ip rule list` and returns parsed + raw output.
pub async fn get_ip_rules() -> impl IntoResponse {
    let output = match Command::new("ip").arg("rule").arg("list").output().await {
        Ok(o) => o,
        Err(e) => {
            warn!(error = %e, "failed to run `ip rule list`");
            return internal_error(format!("failed to run ip rule list: {e}")).into_response();
        }
    };

    let raw = String::from_utf8_lossy(&output.stdout).into_owned();

    let rules: Vec<IpRule> = IP_RULE_RE
        .captures_iter(&raw)
        .filter_map(|cap| {
            let priority = cap[1].parse::<u32>().ok()?;
            Some(IpRule {
                priority,
                from: cap[2].to_owned(),
                table: cap[3].to_owned(),
            })
        })
        .collect();

    Json(IpRulesResponse { rules, raw }).into_response()
}

/// `GET /nft-rules` -- runs `nft list ruleset` and returns parsed tables, masquerade info, and raw output.
pub async fn get_nft_rules() -> impl IntoResponse {
    let output = match Command::new("nft")
        .arg("list")
        .arg("ruleset")
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            warn!(error = %e, "failed to run `nft list ruleset`");
            return internal_error(format!("failed to run nft list ruleset: {e}")).into_response();
        }
    };

    let raw = String::from_utf8_lossy(&output.stdout).into_owned();

    let tables: Vec<String> = NFT_TABLE_RE
        .captures_iter(&raw)
        .map(|cap| cap[1].to_owned())
        .collect();

    let has_masquerade_for: Vec<String> = NFT_MASQ_RE
        .captures_iter(&raw)
        .map(|cap| cap[1].to_owned())
        .collect();

    Json(NftRulesResponse {
        raw,
        tables,
        has_masquerade_for,
    })
    .into_response()
}

/// `GET /wg/:interface` -- runs `wg show <interface>` and returns parsed output.
pub async fn get_wg_show(Path(interface): Path<String>) -> impl IntoResponse {
    if !is_valid_interface_name(&interface) {
        return bad_interface(&interface).into_response();
    }

    let output = match Command::new("wg")
        .arg("show")
        .arg(&interface)
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            warn!(error = %e, interface, "failed to run `wg show`");
            return internal_error(format!("failed to run wg show: {e}")).into_response();
        }
    };

    // A non-zero exit code (or empty stdout) means the interface does not exist.
    if !output.status.success() || output.stdout.is_empty() {
        return Json(WgShowResponse {
            interface,
            exists: false,
            public_key: None,
            listening_port: None,
            peers: None,
        })
        .into_response();
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let (public_key, listening_port, peers) = parse_wg_show(&raw);

    Json(WgShowResponse {
        interface,
        exists: true,
        public_key,
        listening_port,
        peers: Some(peers),
    })
    .into_response()
}

/// Parses the output of `wg show <iface>` into interface fields and a list of peers.
fn parse_wg_show(raw: &str) -> (Option<String>, Option<u16>, Vec<WgPeer>) {
    let mut public_key: Option<String> = None;
    let mut listening_port: Option<u16> = None;
    let mut peers: Vec<WgPeer> = Vec::new();
    let mut current_peer: Option<WgPeer> = None;

    for line in raw.lines() {
        let trimmed = line.trim();

        if let Some(val) = trimmed.strip_prefix("public key:") {
            let val = val.trim();
            if let Some(p) = current_peer.as_mut() {
                val.clone_into(&mut p.public_key);
            } else {
                public_key = Some(val.to_owned());
            }
        } else if let Some(val) = trimmed.strip_prefix("listening port:") {
            listening_port = val.trim().parse().ok();
        } else if let Some(val) = trimmed.strip_prefix("peer:") {
            // Flush previous peer, start a new one.
            if let Some(p) = current_peer.take() {
                peers.push(p);
            }
            current_peer = Some(WgPeer {
                public_key: val.trim().to_owned(),
                endpoint: None,
                allowed_ips: Vec::new(),
                latest_handshake: None,
                transfer_rx: 0,
                transfer_tx: 0,
            });
        } else if let Some(val) = trimmed.strip_prefix("endpoint:") {
            if let Some(p) = current_peer.as_mut() {
                p.endpoint = Some(val.trim().to_owned());
            }
        } else if let Some(val) = trimmed.strip_prefix("allowed ips:") {
            if let Some(p) = current_peer.as_mut() {
                p.allowed_ips = val.split(',').map(|s| s.trim().to_owned()).collect();
            }
        } else if let Some(val) = trimmed.strip_prefix("latest handshake:") {
            if let Some(p) = current_peer.as_mut() {
                p.latest_handshake = Some(val.trim().to_owned());
            }
        } else if let Some(val) = trimmed.strip_prefix("transfer:")
            && let Some(p) = current_peer.as_mut()
        {
            parse_transfer(val.trim(), p);
        }
    }

    // Flush the last peer.
    if let Some(p) = current_peer.take() {
        peers.push(p);
    }

    (public_key, listening_port, peers)
}

/// Parses a `wg show` transfer line such as `1.24 KiB received, 3.48 KiB sent`.
fn parse_transfer(val: &str, peer: &mut WgPeer) {
    // Format: "<amount> <unit> received, <amount> <unit> sent"
    let parts: Vec<&str> = val.split(',').collect();
    if let Some(rx_part) = parts.first() {
        peer.transfer_rx = parse_byte_value(rx_part);
    }
    if let Some(tx_part) = parts.get(1) {
        peer.transfer_tx = parse_byte_value(tx_part);
    }
}

/// Converts a human-readable byte string (e.g. `"1.24 KiB received"`) into bytes.
fn parse_byte_value(s: &str) -> u64 {
    let tokens: Vec<&str> = s.split_whitespace().collect();
    if tokens.len() < 2 {
        return 0;
    }
    let amount: f64 = tokens[0].parse().unwrap_or(0.0);
    let unit = tokens[1];
    let multiplier: f64 = match unit {
        "KiB" => 1024.0,
        "MiB" => 1024.0 * 1024.0,
        "GiB" => 1024.0 * 1024.0 * 1024.0,
        "TiB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        // "B" and anything unrecognised
        _ => 1.0,
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        (amount * multiplier) as u64
    }
}

/// `GET /link/:interface` -- runs `ip link show <interface>` and returns parsed state.
pub async fn get_link_show(Path(interface): Path<String>) -> impl IntoResponse {
    if !is_valid_interface_name(&interface) {
        return bad_interface(&interface).into_response();
    }

    let output = match Command::new("ip")
        .arg("link")
        .arg("show")
        .arg(&interface)
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => {
            warn!(error = %e, interface, "failed to run `ip link show`");
            return internal_error(format!("failed to run ip link show: {e}")).into_response();
        }
    };

    if !output.status.success() {
        return Json(LinkShowResponse {
            name: interface,
            exists: false,
            up: false,
            mtu: None,
        })
        .into_response();
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let up = raw.contains("UP") || raw.contains(",UP,") || raw.contains("<UP");
    let mtu = parse_mtu(&raw);

    Json(LinkShowResponse {
        name: interface,
        exists: true,
        up,
        mtu,
    })
    .into_response()
}

/// Extracts the MTU value from `ip link show` output.
///
/// Looks for `mtu <number>` in the first line.
fn parse_mtu(raw: &str) -> Option<u32> {
    static MTU_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"mtu\s+(\d+)").expect("mtu regex is valid"));

    MTU_RE
        .captures(raw)
        .and_then(|cap| cap[1].parse::<u32>().ok())
}

#[cfg(test)]
mod tests;
