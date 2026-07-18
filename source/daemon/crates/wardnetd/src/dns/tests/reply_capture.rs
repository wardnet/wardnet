//! Unit tests for [`ReplyCapture`] — the capacity-1 capture socket that
//! turns the pipeline's "send on a socket" into "response bytes out" for
//! stream transports (#911).
//!
//! The pipeline sends exactly one response per query, so the interesting
//! cases are the degenerate ones: an extra send (must be dropped, not an
//! error) and a receiver that went away mid-query (must behave like a
//! UDP datagram to a vanished client — fire and forget).

use std::net::SocketAddr;
use std::sync::Arc;

use wardnetd_services::dns::server::DnsSocket;

use crate::dns::reply_capture::ReplyCapture;

fn client_addr() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 5353))
}

#[tokio::test]
async fn captures_the_single_response_frame() {
    let (capture, mut rx) = ReplyCapture::channel();
    let socket: Arc<dyn DnsSocket> = Arc::new(capture);

    let sent = socket
        .send_to(&[0xCA, 0xFE, 0x00], client_addr())
        .await
        .expect("send_to succeeds");
    assert_eq!(sent, 3, "send_to reports the full frame length");

    let frame = rx.recv().await.expect("the response frame is captured");
    assert_eq!(frame, vec![0xCA, 0xFE, 0x00]);
}

#[tokio::test]
async fn extra_sends_are_dropped_without_error() {
    // A double-send is a pipeline defect, not a supported path — but it
    // must degrade to a dropped frame, never an error or a stall on the
    // full capacity-1 channel.
    let (capture, mut rx) = ReplyCapture::channel();
    let socket: Arc<dyn DnsSocket> = Arc::new(capture);

    socket
        .send_to(&[1], client_addr())
        .await
        .expect("first send succeeds");
    let sent = socket
        .send_to(&[2], client_addr())
        .await
        .expect("second send must not error");
    assert_eq!(sent, 1);

    assert_eq!(
        rx.recv().await.expect("first frame arrives"),
        vec![1],
        "the first response wins"
    );
    assert!(
        rx.try_recv().is_err(),
        "the extra frame must have been discarded"
    );
}

#[tokio::test]
async fn dropped_receiver_is_tolerated() {
    // The transport side can vanish mid-query (client reset the stream
    // before the answer was ready). The pipeline's send must stay
    // fire-and-forget, like a UDP datagram to a client that went away.
    let (capture, rx) = ReplyCapture::channel();
    drop(rx);
    let socket: Arc<dyn DnsSocket> = Arc::new(capture);

    let sent = socket
        .send_to(&[9, 9], client_addr())
        .await
        .expect("send after the receiver dropped must not error");
    assert_eq!(sent, 2);
}

#[tokio::test]
async fn recv_from_is_unsupported() {
    // The pipeline never reads from its reply socket; the transport owns
    // the inbound side. Make the misuse loud rather than a silent hang.
    let (capture, _rx) = ReplyCapture::channel();
    let mut buf = [0u8; 16];
    let err = capture
        .recv_from(&mut buf)
        .await
        .expect_err("recv_from must fail");
    assert_eq!(err.kind(), std::io::ErrorKind::Unsupported);
}
