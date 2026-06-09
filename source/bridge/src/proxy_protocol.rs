//! PROXY protocol **v1** header handling.
//!
//! The bridge sits behind a transparent L4 reverse proxy (nginx with
//! `proxy_protocol`) that prepends a single human-readable
//! `PROXY TCP4 <src> <dst> <sport> <dport>\r\n` line before the forwarded bytes.
//! That line carries the **real client address** — which the bridge's per-IP
//! registration controls depend on (a plain L4 forward would otherwise collapse
//! every connection to the proxy's loopback address).
//!
//! The header is read **byte-by-byte up to and including the terminating CRLF and
//! not one byte further**, so the wrapped protocol's first byte (the TLS
//! `ClientHello` on `:8443` / `:8853`, or the HTTP request line on `:8080`) stays in
//! the socket buffer for a subsequent non-consuming [`TcpStream::peek`]. A
//! buffering reader (`BufReader`) must **not** be used here — it would greedily
//! pull the `ClientHello` into its own buffer and break the SNI peek.

use std::net::{IpAddr, SocketAddr};

use tokio::io::AsyncReadExt as _;
use tokio::net::TcpStream;

/// `PROXY ` — the v1 signature.
const SIGNATURE: &[u8] = b"PROXY ";
/// Maximum length of a v1 header line including the trailing CRLF (RFC: 107).
const MAX_HEADER_LEN: usize = 107;

/// Read and consume a **required** PROXY v1 header from `stream`.
///
/// Returns the real client [`SocketAddr`] for `TCP4`/`TCP6`, or `None` for the
/// `UNKNOWN` transport family (the header is still consumed). After this call the
/// stream is positioned at the first byte of the wrapped protocol.
///
/// # Errors
/// Returns an error if the stream does not begin with the `PROXY ` signature, the
/// line exceeds [`MAX_HEADER_LEN`] without a CRLF, or the header is malformed.
pub async fn read_required(stream: &mut TcpStream) -> anyhow::Result<Option<SocketAddr>> {
    let mut line: Vec<u8> = Vec::with_capacity(MAX_HEADER_LEN);
    loop {
        let b = stream.read_u8().await?;
        line.push(b);
        if line.ends_with(b"\r\n") {
            break;
        }
        if line.len() > MAX_HEADER_LEN {
            anyhow::bail!("PROXY v1 header exceeded {MAX_HEADER_LEN} bytes without CRLF");
        }
    }
    parse_v1(&line)
}

/// Outcome of a tolerant PROXY v1 inspection.
#[derive(Debug, PartialEq, Eq)]
pub enum Inspected {
    /// A PROXY header was present and consumed; carries the real client address
    /// (`None` for the `UNKNOWN` family).
    Header(Option<SocketAddr>),
    /// No PROXY header — a direct connection (e.g. a local health probe). No bytes
    /// were consumed.
    Direct,
}

/// Tolerant variant for listeners that may also receive **direct** connections
/// (a health probe hitting `:8080` without going through nginx).
///
/// Peeks for the `PROXY ` signature without consuming: if present, consumes and
/// parses the header ([`Inspected::Header`]); if absent, returns
/// [`Inspected::Direct`] having consumed nothing, so the caller can serve the
/// connection normally.
///
/// # Errors
/// Propagates peek/read failures and malformed-header errors.
pub async fn read_optional(stream: &mut TcpStream) -> anyhow::Result<Inspected> {
    if peek_signature(stream).await? {
        Ok(Inspected::Header(read_required(stream).await?))
    } else {
        Ok(Inspected::Direct)
    }
}

/// Non-consuming check for the `PROXY ` signature at the head of `stream`.
async fn peek_signature(stream: &TcpStream) -> anyhow::Result<bool> {
    let mut buf = [0u8; SIGNATURE.len()];
    loop {
        let n = stream.peek(&mut buf).await?;
        if n == 0 {
            return Ok(false); // peer closed before sending anything
        }
        if buf[..n] != SIGNATURE[..n] {
            return Ok(false); // diverges from the signature ⇒ not a PROXY header
        }
        if n >= SIGNATURE.len() {
            return Ok(true);
        }
        // Matching prefix but the rest of the first segment is still in flight;
        // yield and re-peek. Bounded in practice by the caller's accept timeout.
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    }
}

/// Parse a complete PROXY v1 header line (including the trailing CRLF).
fn parse_v1(line: &[u8]) -> anyhow::Result<Option<SocketAddr>> {
    let line = line
        .strip_suffix(b"\r\n")
        .ok_or_else(|| anyhow::anyhow!("PROXY v1 header not CRLF-terminated"))?;
    let text = std::str::from_utf8(line)
        .map_err(|_| anyhow::anyhow!("PROXY v1 header is not valid UTF-8"))?;

    let mut parts = text.split(' ');
    if parts.next() != Some("PROXY") {
        anyhow::bail!("missing PROXY v1 signature");
    }

    match parts.next() {
        Some("TCP4" | "TCP6") => {
            let src_ip = parts
                .next()
                .ok_or_else(|| anyhow::anyhow!("PROXY v1: missing source address"))?;
            // dst address — skipped.
            parts.next();
            let src_port = parts
                .next()
                .ok_or_else(|| anyhow::anyhow!("PROXY v1: missing source port"))?;
            // Parse IP and port separately so both IPv4 and IPv6 work (an IPv6
            // literal can't simply be `"{ip}:{port}"`).
            let ip: IpAddr = src_ip
                .parse()
                .map_err(|e| anyhow::anyhow!("PROXY v1: invalid source IP: {e}"))?;
            let port: u16 = src_port
                .parse()
                .map_err(|e| anyhow::anyhow!("PROXY v1: invalid source port: {e}"))?;
            Ok(Some(SocketAddr::new(ip, port)))
        }
        Some("UNKNOWN") => Ok(None),
        other => anyhow::bail!("PROXY v1: unsupported transport family {other:?}"),
    }
}

#[cfg(test)]
mod tests;
