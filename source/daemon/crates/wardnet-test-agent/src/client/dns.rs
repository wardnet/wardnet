//! `client dns-resolve` -- resolves a name via `dig` and prints addresses.

use clap::Args;
use tokio::process::Command;

use super::models::{ClientError, DnsResolveResponse};

#[derive(Debug, Args)]
pub struct DnsResolveArgs {
    /// Name to resolve.
    name: String,

    /// DNS server to query (e.g. `127.0.0.1`). Default: system resolver.
    #[arg(long)]
    server: Option<String>,

    /// Record type (`A`, `AAAA`, `TXT`, ...). Default: `A`.
    #[arg(long, default_value = "A")]
    record: String,

    /// Query timeout in seconds.
    #[arg(long, default_value_t = 3)]
    timeout: u32,
}

pub async fn run(args: DnsResolveArgs) -> Result<DnsResolveResponse, ClientError> {
    let mut cmd = Command::new("dig");
    cmd.arg("+short")
        .arg(format!("+time={}", args.timeout))
        .arg("+tries=1");
    if let Some(s) = &args.server {
        cmd.arg(format!("@{s}"));
    }
    cmd.arg(&args.name).arg(&args.record);

    let output = cmd
        .output()
        .await
        .map_err(|e| ClientError::new(format!("failed to run dig: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ClientError::new(format!(
            "dig exited with {:?}: {}",
            output.status.code(),
            stderr.trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let addrs: Vec<String> = stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect();

    Ok(DnsResolveResponse {
        name: args.name,
        server: args.server,
        addrs,
    })
}
