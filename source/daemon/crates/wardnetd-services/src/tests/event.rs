use chrono::Utc;
use uuid::Uuid;
use wardnet_common::event::WardnetEvent;

use crate::event::{BroadcastEventBus, EventPublisher};

fn tunnel_up_event() -> WardnetEvent {
    WardnetEvent::TunnelUp {
        tunnel_id: Uuid::new_v4(),
        interface_name: "wg0".to_owned(),
        endpoint: "1.2.3.4:51820".to_owned(),
        timestamp: Utc::now(),
    }
}

#[tokio::test]
async fn publish_with_subscriber_receives_event() {
    let bus = BroadcastEventBus::new(16);
    let mut rx = bus.subscribe();

    let event = tunnel_up_event();
    bus.publish(event.clone());

    let received = rx.recv().await.expect("should receive event");
    assert!(matches!(received, WardnetEvent::TunnelUp { .. }));
}

#[tokio::test]
async fn publish_without_subscriber_does_not_panic() {
    let bus = BroadcastEventBus::new(16);
    // No subscriber — should not panic.
    bus.publish(tunnel_up_event());
}

#[tokio::test]
async fn multiple_subscribers_each_get_event() {
    let bus = BroadcastEventBus::new(16);
    let mut rx1 = bus.subscribe();
    let mut rx2 = bus.subscribe();

    bus.publish(tunnel_up_event());

    let r1 = rx1.recv().await.expect("subscriber 1 should receive");
    let r2 = rx2.recv().await.expect("subscriber 2 should receive");

    assert!(matches!(r1, WardnetEvent::TunnelUp { .. }));
    assert!(matches!(r2, WardnetEvent::TunnelUp { .. }));
}
