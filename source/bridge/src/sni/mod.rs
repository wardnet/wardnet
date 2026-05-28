use std::net::SocketAddr;
use std::sync::Arc;

use tokio::net::{TcpListener, TcpStream};

use crate::config::Config;
use crate::tunnel::registry::{ForwardRequest, TunnelRegistry};

/// Maximum bytes to peek for SNI extraction.
const PEEK_SIZE: usize = 1024;

/// Run the SNI-routing TCP listener.
///
/// Accepts TLS connections and routes them based on the SNI hostname without
/// terminating TLS:
/// - `bridge_hostname` → forwarded to Caddy at `caddy_addr` (HTTP API).
/// - `{name}.{subdomain_parent}` → forwarded to the registered Pi tunnel.
/// - Anything else → connection dropped.
///
/// # Errors
/// Returns an error if the listener cannot be bound.
pub async fn run(
    addr: &str,
    dest_port: u16,
    config: Config,
    registry: Arc<TunnelRegistry>,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    tracing::info!(addr, dest_port, "SNI demuxer listening");

    loop {
        let (stream, peer) = listener.accept().await?;
        let config = config.clone();
        let registry = Arc::clone(&registry);
        tokio::spawn(async move {
            if let Err(e) = route(stream, peer, dest_port, config, registry).await {
                tracing::debug!(peer = %peer, error = %e, "SNI demux error");
            }
        });
    }
}

async fn route(
    mut stream: TcpStream,
    peer: SocketAddr,
    dest_port: u16,
    config: Config,
    registry: Arc<TunnelRegistry>,
) -> anyhow::Result<()> {
    let mut peek_buf = vec![0u8; PEEK_SIZE];
    let n = stream.peek(&mut peek_buf).await?;

    let sni = parse_sni(&peek_buf[..n]);

    match sni.as_deref() {
        Some(hostname) if hostname == config.bridge_hostname => {
            // Forward to Caddy — it owns the bridge's own TLS certificate.
            let mut caddy = TcpStream::connect(&config.caddy_addr).await?;
            tokio::io::copy_bidirectional(&mut stream, &mut caddy).await?;
        }
        Some(hostname) => {
            if let Some(name) = extract_install_name(hostname, &config.subdomain_parent) {
                let req = ForwardRequest { stream, dest_port };
                if !registry.forward(name, req).await {
                    tracing::debug!(peer = %peer, name, "no active tunnel for install");
                }
            } else {
                tracing::debug!(peer = %peer, sni = hostname, "unroutable SNI, dropping");
            }
        }
        None => {
            tracing::debug!(peer = %peer, "no SNI in ClientHello, dropping");
        }
    }

    Ok(())
}

/// Extract the install slug from an SNI hostname.
///
/// `"happy-einstein.my.us.wardnet.network"` with parent `"my.us.wardnet.network"`
/// returns `Some("happy-einstein")`. Returns `None` when the hostname does not
/// end with `.{subdomain_parent}` or the prefix contains a dot (multi-level).
fn extract_install_name<'a>(hostname: &'a str, subdomain_parent: &str) -> Option<&'a str> {
    let name = hostname.strip_suffix(&format!(".{subdomain_parent}"))?;
    if name.contains('.') {
        return None;
    }
    Some(name)
}

/// Parse the SNI hostname from the first bytes of a TLS `ClientHello`.
///
/// Uses only the bytes already available via `TcpStream::peek`; returns `None`
/// if the buffer is too short, the record is not a `ClientHello`, or the SNI
/// extension is absent.
pub fn parse_sni(buf: &[u8]) -> Option<String> {
    // TLS record: content_type(1) + version(2) + length(2)
    if buf.len() < 5 {
        return None;
    }
    if buf[0] != 0x16 {
        // Not a handshake record.
        return None;
    }
    let record_len = u16::from_be_bytes([buf[3], buf[4]]) as usize;
    if buf.len() < 5 + record_len {
        return None;
    }
    let hs = &buf[5..5 + record_len];

    // Handshake: msg_type(1) + length(3)
    if hs.len() < 4 || hs[0] != 0x01 {
        return None;
    }
    let hs_body_len = (u32::from_be_bytes([0, hs[1], hs[2], hs[3]])) as usize;
    if hs.len() < 4 + hs_body_len {
        return None;
    }
    let hello = &hs[4..4 + hs_body_len];

    // ClientHello: version(2) + random(32) + session_id_len(1)
    if hello.len() < 35 {
        return None;
    }
    let mut pos = 35 + hello[34] as usize; // skip session_id

    // cipher_suites_len(2) + cipher_suites
    if hello.len() < pos + 2 {
        return None;
    }
    pos += 2 + u16::from_be_bytes([hello[pos], hello[pos + 1]]) as usize;

    // compression_methods_len(1) + methods
    if hello.len() < pos + 1 {
        return None;
    }
    pos += 1 + hello[pos] as usize;

    // extensions_len(2)
    if hello.len() < pos + 2 {
        return None;
    }
    let ext_len = u16::from_be_bytes([hello[pos], hello[pos + 1]]) as usize;
    pos += 2;

    if hello.len() < pos + ext_len {
        return None;
    }
    let exts = &hello[pos..pos + ext_len];
    let mut i = 0;

    while i + 4 <= exts.len() {
        let ext_type = u16::from_be_bytes([exts[i], exts[i + 1]]);
        let elen = u16::from_be_bytes([exts[i + 2], exts[i + 3]]) as usize;
        i += 4;
        if i + elen > exts.len() {
            break;
        }
        if ext_type == 0x0000 {
            // SNI extension: list_len(2) + entry_type(1) + name_len(2) + name
            let sni_data = &exts[i..i + elen];
            if sni_data.len() < 5 || sni_data[2] != 0x00 {
                return None;
            }
            let name_len = u16::from_be_bytes([sni_data[3], sni_data[4]]) as usize;
            if sni_data.len() < 5 + name_len {
                return None;
            }
            return std::str::from_utf8(&sni_data[5..5 + name_len])
                .ok()
                .map(str::to_string);
        }
        i += elen;
    }

    None
}

#[cfg(test)]
mod tests;
