use std::sync::Arc;

use tokio::net::{TcpListener, TcpStream};

use super::{ForwardRequest, ForwardResult, TunnelRegistry};

#[tokio::test]
async fn register_and_unregister() {
    let reg = TunnelRegistry::new();
    assert!(!reg.is_connected("alice"));

    let _rx = reg.register("install-1", "alice");
    assert!(reg.is_connected("alice"));

    reg.unregister("install-1");
    assert!(!reg.is_connected("alice"));
}

#[tokio::test]
async fn forward_delivers_request() {
    let reg = Arc::new(TunnelRegistry::new());
    let mut rx = reg.register("install-2", "bob");

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let stream = TcpStream::connect(addr).await.unwrap();

    let req = ForwardRequest {
        stream,
        dest_port: 443,
    };
    let result = reg.forward("bob", req);
    assert!(
        matches!(result, ForwardResult::Accepted),
        "forward should succeed when tunnel is registered"
    );

    let received = rx.recv().await;
    assert!(received.is_some(), "receiver should get the request");
    assert_eq!(received.unwrap().dest_port, 443);
}

#[tokio::test]
async fn forward_returns_not_connected_when_unregistered() {
    let reg = TunnelRegistry::new();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let stream = TcpStream::connect(addr).await.unwrap();

    let req = ForwardRequest {
        stream,
        dest_port: 443,
    };
    let result = reg.forward("nobody", req);
    assert!(
        matches!(result, ForwardResult::NotConnected),
        "forward should return NotConnected when no tunnel is registered"
    );
}

#[tokio::test]
async fn forward_returns_buffer_full_when_channel_saturated() {
    let reg = TunnelRegistry::new();
    let rx = reg.register("install-full", "frank");

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Fill the channel to capacity (64 slots).
    for _ in 0..64 {
        let stream = TcpStream::connect(addr).await.unwrap();
        let result = reg.forward(
            "frank",
            ForwardRequest {
                stream,
                dest_port: 443,
            },
        );
        assert!(matches!(result, ForwardResult::Accepted));
    }

    // The 65th forward must fail because the buffer is full.
    let stream = TcpStream::connect(addr).await.unwrap();
    let result = reg.forward(
        "frank",
        ForwardRequest {
            stream,
            dest_port: 443,
        },
    );
    assert!(matches!(result, ForwardResult::BufferFull));

    drop(rx);
}

#[tokio::test]
async fn forward_returns_not_connected_when_receiver_dropped() {
    let reg = TunnelRegistry::new();
    // Register and immediately drop the receiver — the sender stays in by_name
    // but try_send will return TrySendError::Closed.
    let rx = reg.register("install-closed", "grace");
    drop(rx);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let stream = TcpStream::connect(listener.local_addr().unwrap())
        .await
        .unwrap();
    let result = reg.forward(
        "grace",
        ForwardRequest {
            stream,
            dest_port: 443,
        },
    );
    assert!(matches!(result, ForwardResult::NotConnected));
}

#[tokio::test]
async fn second_register_replaces_first() {
    let reg = TunnelRegistry::new();
    let _rx1 = reg.register("install-3", "carol");
    // Second registration for the same slug replaces the first sender.
    let _rx2 = reg.register("install-3", "carol");
    assert!(reg.is_connected("carol"));
    reg.unregister("install-3");
    assert!(!reg.is_connected("carol"));
}
