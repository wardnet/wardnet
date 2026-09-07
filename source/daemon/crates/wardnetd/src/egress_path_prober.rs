//! Real [`EgressPathProber`]: a two-stage probe over one egress path.
//!
//! Linux binds the socket to the tunnel interface (`SO_BINDTODEVICE`, via
//! `reqwest::ClientBuilder::interface` and `socket2` for the raw connect) so the
//! probe traverses that path rather than the default route. `None` probes
//! unbound — the direct/WAN path.
//!
//! # Why two stages
//!
//! A bare TCP connect is three small packets. Under an MTU/MSS asymmetry those
//! fit perfectly while every real connection stalls on the first full-size
//! segment, so a connect-only probe would report the path healthy right through
//! the outage this exists to explain. The transfer stage completes a TLS
//! handshake — whose certificate chain is several KB, in full-MTU segments — so
//! the pair distinguishes "the path is dead" from "the path drops large
//! packets".

use std::time::{Duration, Instant};

use async_trait::async_trait;
use wardnet_common::egress_path::{PathProbeOutcome, StageOutcome};
use wardnetd_services::egress_path::{EgressPathProber, ProbeTarget};

/// Budget for the TCP handshake.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Budget for the TLS handshake plus the small request.
///
/// Longer than the connect budget because this is the stage that stalls when
/// the path is degraded, and a blackholed segment is only visible as a timeout.
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(10);

pub struct RealEgressPathProber {
    target: ProbeTarget,
    /// The URL used for the transfer stage. Shares the endpoint the tunnel exit
    /// probe already contacts, so this adds no new third party.
    transfer_url: String,
}

impl RealEgressPathProber {
    #[must_use]
    pub fn new(target: ProbeTarget, transfer_url: String) -> Self {
        Self {
            target,
            transfer_url,
        }
    }
}

#[cfg(target_os = "linux")]
async fn connect_stage(target: &ProbeTarget, interface: Option<&str>) -> StageOutcome {
    use socket2::{Domain, Protocol, Socket, Type};

    let started = Instant::now();
    let address = format!("{}:{}", target.host, target.port);
    let Ok(addr) = address.parse::<std::net::SocketAddr>() else {
        return StageOutcome::failed(format!("probe target {address} is not an IP literal"));
    };

    let result = tokio::time::timeout(CONNECT_TIMEOUT, async {
        let socket = Socket::new(Domain::for_address(addr), Type::STREAM, Some(Protocol::TCP))?;
        if let Some(iface) = interface {
            socket.bind_device(Some(iface.as_bytes()))?;
        }
        socket.set_nonblocking(true)?;
        // A non-blocking connect returns EINPROGRESS; hand the fd to tokio and
        // let it drive the handshake to completion.
        match socket.connect(&addr.into()) {
            Ok(()) => {}
            Err(e) if e.raw_os_error() == Some(libc::EINPROGRESS) => {}
            Err(e) => return Err(e),
        }
        let stream = tokio::net::TcpStream::from_std(std::net::TcpStream::from(socket))?;
        stream.writable().await?;
        if let Some(e) = stream.take_error()? {
            return Err(e);
        }
        Ok::<(), std::io::Error>(())
    })
    .await;

    match result {
        Ok(Ok(())) => StageOutcome::succeeded(elapsed_ms(started)),
        Ok(Err(e)) => StageOutcome::failed(e.to_string()),
        Err(_) => StageOutcome::failed(format!(
            "no TCP handshake within {}s",
            CONNECT_TIMEOUT.as_secs()
        )),
    }
}

#[cfg(target_os = "linux")]
async fn transfer_stage(url: &str, interface: Option<&str>) -> StageOutcome {
    let started = Instant::now();

    let client = match crate::reqwest_client::interface_bound_builder(interface)
        .timeout(TRANSFER_TIMEOUT)
        .build()
    {
        Ok(client) => client,
        Err(e) => return StageOutcome::failed(format!("client build failed: {e}")),
    };

    match client.get(url).send().await {
        Ok(resp) => match resp.bytes().await {
            // The body is read to completion deliberately: a stalled path can
            // complete the TLS handshake and then hang mid-body, and a probe
            // that stopped at the status line would miss it.
            Ok(_) => StageOutcome::succeeded(elapsed_ms(started)),
            Err(e) => StageOutcome::failed(format!("transfer stalled mid-body: {e}")),
        },
        Err(e) if e.is_timeout() => StageOutcome::failed(format!(
            "no response within {}s — the connection established but did not carry data",
            TRANSFER_TIMEOUT.as_secs()
        )),
        Err(e) => StageOutcome::failed(e.to_string()),
    }
}

#[cfg(target_os = "linux")]
fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

#[async_trait]
impl EgressPathProber for RealEgressPathProber {
    #[cfg(target_os = "linux")]
    async fn probe(&self, interface_name: Option<&str>) -> PathProbeOutcome {
        let connect = connect_stage(&self.target, interface_name).await;
        if !connect.ok {
            // Nothing to transfer over. Leaving `transfer` as `None` is what
            // keeps a dead path from also reading as degraded.
            return PathProbeOutcome {
                connect,
                transfer: None,
            };
        }
        let transfer = transfer_stage(&self.transfer_url, interface_name).await;
        PathProbeOutcome {
            connect,
            transfer: Some(transfer),
        }
    }

    #[cfg(not(target_os = "linux"))]
    async fn probe(&self, _interface_name: Option<&str>) -> PathProbeOutcome {
        let _ = (&self.target, &self.transfer_url);
        PathProbeOutcome {
            connect: StageOutcome::failed(
                "SO_BINDTODEVICE is Linux-only; egress path probing requires Linux",
            ),
            transfer: None,
        }
    }
}
