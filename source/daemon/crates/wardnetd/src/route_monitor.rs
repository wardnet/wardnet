use std::sync::Arc;

use futures::StreamExt;
use rtnetlink::MulticastGroup;
use rtnetlink::packet_core::NetlinkPayload;
use rtnetlink::packet_route::RouteNetlinkMessage;
use rtnetlink::packet_route::route::RouteAttribute;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use wardnet_common::event::WardnetEvent;

use wardnetd_services::event::EventPublisher;

/// Minimum routing table number managed by Wardnet.
///
/// Tables below 100 are kernel defaults (local, main, default) and must not
/// be treated as Wardnet-managed.
const WARDNET_MIN_TABLE: u32 = 100;

/// Background task that subscribes to kernel route change events via netlink.
///
/// Watches for `RTM_DELROUTE` messages on Wardnet-managed routing tables
/// (tables >= 100). When such a deletion is detected — typically because the
/// interface was removed or recreated externally — it publishes a
/// [`WardnetEvent::RouteTableLost`] so the routing service can re-add the
/// route.
pub struct RouteMonitor {
    cancel: CancellationToken,
    handle: tokio::task::JoinHandle<()>,
}

impl RouteMonitor {
    /// Start the route monitor.
    ///
    /// Opens a second netlink connection subscribed to the `RTNLGRP_IPV4_ROUTE`
    /// multicast group. The `parent` span is used for structured logging.
    pub fn start(events: Arc<dyn EventPublisher>, parent: &tracing::Span) -> anyhow::Result<Self> {
        let cancel = CancellationToken::new();
        let span = tracing::info_span!(parent: parent, "route_monitor");

        // Open a netlink connection with route multicast subscription.
        let (connection, _, mut messages) =
            rtnetlink::new_multicast_connection(&[MulticastGroup::Ipv4Route])?;

        tokio::spawn(connection);

        let cancel_clone = cancel.clone();
        let handle = tokio::spawn(
            async move {
                tracing::info!("route monitor started, watching for route deletions on tables >= {WARDNET_MIN_TABLE}");
                loop {
                    tokio::select! {
                        () = cancel_clone.cancelled() => break,
                        msg = messages.next() => {
                            if let Some((message, _)) = msg {
                                if let NetlinkPayload::InnerMessage(inner) = message.payload {
                                    handle_message(inner, &*events);
                                }
                            } else {
                                tracing::warn!("route monitor: netlink channel closed");
                                break;
                            }
                        }
                    }
                }
            }
            .instrument(span),
        );

        Ok(Self { cancel, handle })
    }

    /// Cancel the background task and wait for it to finish.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        let _ = self.handle.await;
        tracing::info!("route monitor shut down");
    }
}

/// Inspect a netlink route message and publish `RouteTableLost` if it's a
/// deletion from a Wardnet-managed table.
fn handle_message(payload: RouteNetlinkMessage, events: &dyn EventPublisher) {
    let RouteNetlinkMessage::DelRoute(route) = payload else {
        return;
    };

    // Extract the table number. For tables > 255 the value is in an attribute;
    // for smaller tables it's in the header.
    let table = route
        .attributes
        .iter()
        .find_map(|a| {
            if let RouteAttribute::Table(t) = a {
                Some(*t)
            } else {
                None
            }
        })
        .unwrap_or(u32::from(route.header.table));

    if table < WARDNET_MIN_TABLE {
        return;
    }

    tracing::warn!(table, "detected route deletion in wardnet-managed table");

    events.publish(WardnetEvent::RouteTableLost {
        table,
        timestamp: chrono::Utc::now(),
    });
}
