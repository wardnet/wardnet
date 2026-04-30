//! `client routes` -- prints the kernel routing table parsed from `ip -j route`.

use clap::Args;
use serde::Deserialize;
use tokio::process::Command;

use super::models::{ClientError, Route, RoutesResponse};

#[derive(Debug, Args)]
pub struct RoutesArgs {
    /// Optional table to query (e.g. `main`, `100`). Default: `main`.
    #[arg(long)]
    pub table: Option<String>,
}

/// Subset of `ip -j route` JSON output that we consume.
#[derive(Debug, Deserialize)]
struct IpRoute {
    dst: Option<String>,
    gateway: Option<String>,
    dev: Option<String>,
    table: Option<String>,
    protocol: Option<String>,
    scope: Option<String>,
    prefsrc: Option<String>,
    metric: Option<u32>,
}

pub async fn run(args: RoutesArgs) -> Result<RoutesResponse, ClientError> {
    let mut cmd = Command::new("ip");
    cmd.arg("-j").arg("route").arg("show");
    if let Some(t) = &args.table {
        cmd.arg("table").arg(t);
    }

    let output = cmd
        .output()
        .await
        .map_err(|e| ClientError::new(format!("failed to run ip -j route: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ClientError::new(format!(
            "ip route exited with {:?}: {}",
            output.status.code(),
            stderr.trim()
        )));
    }

    let raw: Vec<IpRoute> = serde_json::from_slice(&output.stdout)
        .map_err(|e| ClientError::new(format!("failed to parse ip -j route output: {e}")))?;

    let routes = raw
        .into_iter()
        .map(|r| Route {
            dst: r.dst.unwrap_or_else(|| "default".to_owned()),
            gateway: r.gateway,
            dev: r.dev,
            table: r.table,
            protocol: r.protocol,
            scope: r.scope,
            src: r.prefsrc,
            metric: r.metric,
        })
        .collect();

    Ok(RoutesResponse { routes })
}
