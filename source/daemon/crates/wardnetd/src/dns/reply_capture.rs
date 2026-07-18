//! [`ReplyCapture`] — a [`DnsSocket`] that hands the pipeline's single
//! response back to a stream transport (#911).
//!
//! The resolve core keeps the "send on a socket" seam (see `pipeline.rs`),
//! which suits UDP but leaves stream transports — DNS-over-TLS first —
//! needing "query bytes in → response bytes out". `ReplyCapture` bridges
//! that: the transport hands the pipeline a capture socket per query and
//! awaits the captured frame on the paired receiver.
//!
//! The pipeline sends **at most one** response per query: exactly one on
//! every resolution path, none for a malformed or question-less packet
//! (see [`QueryPipeline::handle`](crate::dns::pipeline::QueryPipeline::handle)).
//! A capacity-1 channel is therefore sufficient — but the transport must
//! bound its receive (time it out, or drop the capture before awaiting)
//! rather than assume a frame always arrives.

use std::net::SocketAddr;

use async_trait::async_trait;
use tokio::sync::mpsc;
use wardnetd_services::dns::server::DnsSocket;

/// Capture side of a one-response channel. Build with
/// [`ReplyCapture::channel`]; pass the capture to
/// [`QueryPipeline::handle`](crate::dns::pipeline::QueryPipeline::handle)
/// as the reply socket and await the response bytes on the receiver.
pub struct ReplyCapture {
    tx: mpsc::Sender<Vec<u8>>,
}

impl ReplyCapture {
    /// Create a capture socket and the receiver its response arrives on.
    /// One pair serves one query: a reused, undrained pair silently
    /// drops later frames (see `send_to`), so make a fresh channel per
    /// query rather than recycling one across a connection.
    #[must_use]
    pub fn channel() -> (Self, mpsc::Receiver<Vec<u8>>) {
        // Capacity 1: the pipeline sends at most one response per query.
        // A second send (a defect, not a supported path) or a receiver
        // that went away must degrade to a dropped datagram, never an
        // error or a stall — see `send_to`.
        let (tx, rx) = mpsc::channel(1);
        (Self { tx }, rx)
    }
}

#[async_trait]
impl DnsSocket for ReplyCapture {
    /// The pipeline never reads from its reply socket; the transport owns
    /// the inbound side of the stream.
    async fn recv_from(&self, _buf: &mut [u8]) -> std::io::Result<(usize, SocketAddr)> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "ReplyCapture is send-only",
        ))
    }

    /// Capture the response frame. Mirrors UDP's fire-and-forget
    /// semantics: a full channel (an extra send) or a dropped receiver
    /// (the client tore the stream down mid-query) discards the frame and
    /// still reports success, so no pipeline path starts failing on a
    /// transport that merely went away.
    async fn send_to(&self, buf: &[u8], _target: SocketAddr) -> std::io::Result<usize> {
        match self.tx.try_send(buf.to_vec()) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                tracing::debug!("ReplyCapture: dropping extra response frame");
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                tracing::debug!("ReplyCapture: receiver gone; dropping response frame");
            }
        }
        Ok(buf.len())
    }
}
