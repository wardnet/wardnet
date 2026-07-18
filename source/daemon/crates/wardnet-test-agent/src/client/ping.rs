//! `client ping` -- runs ICMP echo and reports counts + average RTT.

use std::sync::LazyLock;

use clap::Args;
use regex::Regex;
use tokio::process::Command;

use super::models::{ClientError, PingResponse};

#[derive(Debug, Args)]
pub struct PingArgs {
    /// Host or IP to ping.
    pub target: String,

    /// Number of echo requests to send.
    #[arg(short, long, default_value_t = 3)]
    pub count: u32,

    /// Timeout per probe in seconds.
    #[arg(long, default_value_t = 2)]
    pub timeout: u32,

    /// Source interface (`-I <iface>`).
    #[arg(short = 'I', long)]
    pub interface: Option<String>,
}

/// `5 packets transmitted, 5 received, 0% packet loss, time 4006ms`
pub(crate) static SUMMARY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(\d+)\s+packets transmitted,\s+(\d+)\s+(?:packets\s+)?received,\s+(\d+(?:\.\d+)?)%",
    )
    .expect("ping summary regex is valid")
});

/// `rtt min/avg/max/mdev = 1.234/5.678/9.012/3.456 ms`
pub(crate) static RTT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:rtt|round-trip)\s+min/avg/max(?:/mdev)?\s*=\s*[\d.]+/([\d.]+)/")
        .expect("ping rtt regex is valid")
});

pub async fn run(args: PingArgs) -> Result<PingResponse, ClientError> {
    let mut cmd = Command::new("ping");
    cmd.arg("-c")
        .arg(args.count.to_string())
        .arg("-W")
        .arg(args.timeout.to_string());
    if let Some(iface) = &args.interface {
        cmd.arg("-I").arg(iface);
    }
    cmd.arg(&args.target);

    let output = cmd
        .output()
        .await
        .map_err(|e| ClientError::new(format!("failed to run ping: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    let (transmitted, received, packet_loss_pct) = SUMMARY_RE
        .captures(&stdout)
        .and_then(|cap| {
            let t = cap[1].parse::<u32>().ok()?;
            let r = cap[2].parse::<u32>().ok()?;
            let l = cap[3].parse::<f64>().ok()?;
            Some((t, r, l))
        })
        .unwrap_or((args.count, 0, 100.0));

    let rtt_avg_ms = RTT_RE
        .captures(&stdout)
        .and_then(|cap| cap[1].parse::<f64>().ok());

    Ok(PingResponse {
        target: args.target,
        transmitted,
        received,
        packet_loss_pct,
        rtt_avg_ms,
    })
}
