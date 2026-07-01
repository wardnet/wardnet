//! No-op Web Push sender for the mock daemon: logs the intended delivery and
//! reports success without touching the network, so dev runs never POST to a
//! real push service.

use async_trait::async_trait;
use wardnetd_services::push::sender::{PushTarget, SendOutcome, VapidKey, WebPushSender};

pub struct NoopWebPushSender;

#[async_trait]
impl WebPushSender for NoopWebPushSender {
    async fn send(
        &self,
        _vapid: &VapidKey,
        target: PushTarget<'_>,
        payload: Vec<u8>,
    ) -> SendOutcome {
        tracing::info!(
            endpoint = %target.endpoint,
            payload = %String::from_utf8_lossy(&payload),
            "mock: pretending to deliver web push",
        );
        SendOutcome::Delivered
    }
}
