use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::interval;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use crate::config::DetectionConfig;
use crate::packet_capture::PacketCapture;
use crate::service::{DeviceDiscoveryService, ObservationResult};

/// Background device detection orchestrator.
///
/// Spawns five subtasks:
/// 1. Passive capture loop -- listens for ARP/IP packets, sends observations on mpsc channel
/// 2. Observation processor -- receives from channel, calls discovery service, logs at info level
/// 3. Batch flush loop -- every N seconds, flushes `last_seen` to DB
/// 4. Departure scanner -- every N seconds, emits `DeviceGone` for stale devices
/// 5. Active ARP scanner -- every N seconds, broadcasts ARP requests for all IPs in subnet
pub struct DeviceDetector {
    cancel: CancellationToken,
    handles: Vec<tokio::task::JoinHandle<()>>,
}

impl DeviceDetector {
    /// Start the device detector with the given dependencies.
    ///
    /// The `parent` span is used as the parent for the `device_detector` child
    /// span, ensuring all log output from spawned tasks includes the root
    /// version field.
    ///
    /// Returns a `DeviceDetector` whose background tasks run until
    /// [`shutdown`](Self::shutdown) is called.
    pub fn start(
        capture: Arc<dyn PacketCapture>,
        discovery: Arc<dyn DeviceDiscoveryService>,
        config: &DetectionConfig,
        interface: String,
        parent: &tracing::Span,
    ) -> Self {
        let cancel = CancellationToken::new();
        let (tx, rx) = mpsc::channel(10_000);
        let span = tracing::info_span!(parent: parent, "device_detector");

        let capture_handle = tokio::spawn(
            capture_task(capture.clone(), interface.clone(), tx, cancel.clone())
                .instrument(span.clone()),
        );

        let processor_handle = tokio::spawn(
            processor_task(rx, discovery.clone(), cancel.clone()).instrument(span.clone()),
        );

        let flush_handle = tokio::spawn(
            flush_task(
                discovery.clone(),
                config.batch_flush_interval_secs,
                cancel.clone(),
            )
            .instrument(span.clone()),
        );

        let departure_handle = tokio::spawn(
            departure_task(
                discovery,
                config.departure_timeout_secs,
                config.departure_scan_interval_secs,
                cancel.clone(),
            )
            .instrument(span.clone()),
        );

        let arp_handle = tokio::spawn(
            arp_scan_task(
                capture,
                interface,
                config.arp_scan_interval_secs,
                cancel.clone(),
            )
            .instrument(span),
        );

        Self {
            cancel,
            handles: vec![
                capture_handle,
                processor_handle,
                flush_handle,
                departure_handle,
                arp_handle,
            ],
        }
    }

    /// Cancel all background tasks and wait for them to finish.
    pub async fn shutdown(self) {
        self.cancel.cancel();
        for handle in self.handles {
            let _ = handle.await;
        }
        tracing::info!("device detector shut down");
    }
}

/// Passive capture loop. Runs until cancelled or the capture returns an error.
async fn capture_task(
    capture: Arc<dyn PacketCapture>,
    interface: String,
    sender: mpsc::Sender<crate::packet_capture::ObservedDevice>,
    cancel: CancellationToken,
) {
    if let Err(e) = capture.capture_loop(&interface, sender, cancel).await {
        tracing::error!(error = %e, interface = %interface, "capture loop exited with error on {interface}: {e}");
    }
}

/// Receives observations from the capture channel and processes them.
async fn processor_task(
    mut rx: mpsc::Receiver<crate::packet_capture::ObservedDevice>,
    discovery: Arc<dyn DeviceDiscoveryService>,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            obs = rx.recv() => {
                let Some(obs) = obs else { break };

                match discovery.process_observation(&obs).await {
                    Ok(ObservationResult::NewDevice { device_id, .. }) => {
                        tracing::info!(mac = %obs.mac, ip = %obs.ip, "device detected: mac={mac}, ip={ip}", mac = obs.mac, ip = obs.ip);

                        // Spawn background hostname resolution, inheriting
                        // the current span so log output includes the
                        // `device_detector` context.
                        let discovery_clone = discovery.clone();
                        let ip = obs.ip.clone();
                        let resolve_span = tracing::Span::current();
                        tokio::spawn(
                            async move {
                                if let Err(e) =
                                    discovery_clone.resolve_hostname(device_id, ip).await
                                {
                                    tracing::warn!(
                                        device_id = %device_id,
                                        error = %e,
                                        "hostname resolution failed for device {device_id}: {e}"
                                    );
                                }
                            }
                            .instrument(resolve_span),
                        );
                    }
                    Ok(ObservationResult::IpChanged { old_ip, .. }) => {
                        tracing::info!(
                            mac = %obs.mac,
                            old_ip = %old_ip,
                            new_ip = %obs.ip,
                            "device IP changed: mac={mac}, old_ip={old_ip}, new_ip={new_ip}", mac = obs.mac, new_ip = obs.ip
                        );
                    }
                    Ok(ObservationResult::Reappeared(_)) => {
                        tracing::info!(mac = %obs.mac, ip = %obs.ip, "device returned: mac={mac}, ip={ip}", mac = obs.mac, ip = obs.ip);
                    }
                    Ok(ObservationResult::Seen(_)) => {
                        // Too noisy to log.
                    }
                    Err(e) => {
                        tracing::warn!(
                            mac = %obs.mac,
                            ip = %obs.ip,
                            error = %e,
                            "failed to process device observation: mac={mac}, ip={ip}, error={e}", mac = obs.mac, ip = obs.ip
                        );
                    }
                }
            }
        }
    }
}

/// Periodically flush batched `last_seen` timestamps to the database.
async fn flush_task(
    discovery: Arc<dyn DeviceDiscoveryService>,
    interval_secs: u64,
    cancel: CancellationToken,
) {
    let mut tick = interval(Duration::from_secs(interval_secs));

    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            _ = tick.tick() => {}
        }

        match discovery.flush_last_seen().await {
            Ok(count) => {
                tracing::debug!(count, "flushed last_seen timestamps: count={count}");
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to flush last_seen timestamps: {e}");
            }
        }
    }
}

/// Periodically scan for departed devices.
async fn departure_task(
    discovery: Arc<dyn DeviceDiscoveryService>,
    timeout_secs: u64,
    interval_secs: u64,
    cancel: CancellationToken,
) {
    let mut tick = interval(Duration::from_secs(interval_secs));

    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            _ = tick.tick() => {}
        }

        match discovery.scan_departures(timeout_secs).await {
            Ok(departed) => {
                tracing::info!(
                    count = departed.len(),
                    "departure scan complete: count={count}",
                    count = departed.len()
                );
            }
            Err(e) => {
                tracing::warn!(error = %e, "departure scan failed: {e}");
            }
        }
    }
}

/// Periodically broadcast ARP requests to discover devices on the LAN.
async fn arp_scan_task(
    capture: Arc<dyn PacketCapture>,
    interface: String,
    interval_secs: u64,
    cancel: CancellationToken,
) {
    let mut tick = interval(Duration::from_secs(interval_secs));

    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            _ = tick.tick() => {}
        }

        if let Err(e) = capture.arp_scan(&interface).await {
            tracing::warn!(
                interface = %interface,
                error = %e,
                "ARP scan failed on {interface}: {e}"
            );
        } else {
            tracing::debug!(interface = %interface, "ARP scan sent on {interface}");
        }
    }
}
