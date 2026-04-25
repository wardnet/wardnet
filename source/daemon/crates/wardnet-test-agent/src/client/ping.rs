//! `client ping` -- runs ICMP echo and reports counts + average RTT.

use std::sync::LazyLock;

use clap::Args;
use regex::Regex;
use tokio::process::Command;

use super::models::{ClientError, PingResponse};

#[derive(Debug, Args)]
pub struct PingArgs {
    /// Host or IP to ping.
    target: String,

    /// Number of echo requests to send.
    #[arg(short, long, default_value_t = 3)]
    count: u32,

    /// Timeout per probe in seconds.
    #[arg(long, default_value_t = 2)]
    timeout: u32,

    /// Source interface (`-I <iface>`).
    #[arg(short = 'I', long)]
    interface: Option<String>,
}

/// `5 packets transmitted, 5 received, 0% packet loss, time 4006ms`
static SUMMARY_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(\d+)\s+packets transmitted,\s+(\d+)\s+(?:packets\s+)?received,\s+(\d+(?:\.\d+)?)%",
    )
    .expect("ping summary regex is valid")
});

/// `rtt min/avg/max/mdev = 1.234/5.678/9.012/3.456 ms`
static RTT_RE: LazyLock<Regex> = LazyLock::new(|| {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_summary_and_rtt() {
        let raw = "PING 1.1.1.1 (1.1.1.1): 56 data bytes\n\
            64 bytes from 1.1.1.1: icmp_seq=0 ttl=58 time=8.123 ms\n\
            \n\
            --- 1.1.1.1 ping statistics ---\n\
            5 packets transmitted, 5 received, 0% packet loss, time 4006ms\n\
            rtt min/avg/max/mdev = 7.123/8.456/9.789/0.910 ms\n";
        let cap = SUMMARY_RE.captures(raw).expect("summary matched");
        assert_eq!(&cap[1], "5");
        assert_eq!(&cap[2], "5");
        assert_eq!(&cap[3], "0");

        let rtt = RTT_RE.captures(raw).expect("rtt matched");
        assert_eq!(&rtt[1], "8.456");
    }

    #[test]
    fn parses_partial_loss() {
        let raw = "3 packets transmitted, 1 received, 66% packet loss, time 2003ms\n";
        let cap = SUMMARY_RE.captures(raw).expect("summary matched");
        assert_eq!(&cap[1], "3");
        assert_eq!(&cap[2], "1");
        assert_eq!(&cap[3], "66");
        assert!(RTT_RE.captures(raw).is_none());
    }
}
