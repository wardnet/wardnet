use std::sync::Arc;

use futures::StreamExt;
use rtnetlink::MulticastGroup;
use rtnetlink::packet_core::NetlinkPayload;
use rtnetlink::packet_route::RouteNetlinkMessage;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use wardnet_common::event::WardnetEvent;

use wardnetd_services::event::EventPublisher;

/// Lowest routing table number managed by Wardnet.
///
/// `RoutingServiceImpl::table_for_index` maps tunnel interface index *n* to
/// table `100 + n`, so 100 is the first table we ever file a route in.
const WARDNET_MIN_TABLE: u32 = 100;

/// Highest routing table number managed by Wardnet.
///
/// The kernel reserves the top four tables — 252 (`compat`), 253 (`default`),
/// 254 (`main`) and 255 (`local`) — so Wardnet's range stops just below them.
/// Treating those as ours republishes ordinary kernel routing churn as a
/// Wardnet fault: a live Pi logged `detected route deletion in wardnet-managed
/// table table=254` for a routine change to `main`, raising a spurious
/// `RouteTableLost` diagnostic.
///
/// 252 earns its place twice. Besides being reserved, it is the value the
/// kernel stamps into the u8 `header.table` for any table id too wide to fit
/// there, so a deletion in someone else's table 1000 arriving without an
/// `RTA_TABLE` attribute resolves to 252 through `route_table`'s fallback.
const WARDNET_MAX_TABLE: u32 = 251;

/// Background task that subscribes to kernel route change events via netlink.
///
/// Watches for `RTM_DELROUTE` messages on Wardnet-managed routing tables
/// (100..=251). When such a deletion is detected — typically because the
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
                tracing::info!("route monitor started, watching for route deletions on tables {WARDNET_MIN_TABLE}..={WARDNET_MAX_TABLE}");
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
pub(crate) fn handle_message(payload: RouteNetlinkMessage, events: &dyn EventPublisher) {
    let RouteNetlinkMessage::DelRoute(route) = payload else {
        return;
    };

    let table = crate::policy_router_netlink::route_table(&route);

    if !(WARDNET_MIN_TABLE..=WARDNET_MAX_TABLE).contains(&table) {
        return;
    }

    tracing::warn!(table, "detected route deletion in wardnet-managed table");

    events.publish(WardnetEvent::RouteTableLost {
        table,
        timestamp: chrono::Utc::now(),
    });
}
