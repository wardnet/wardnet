//! `client interfaces` -- prints interfaces and their addresses parsed from
//! `ip -j addr show`.

use clap::Args;
use serde::Deserialize;
use tokio::process::Command;

use super::models::{ClientError, Interface, InterfaceAddr, InterfacesResponse};

#[derive(Debug, Args)]
pub struct InterfacesArgs {
    /// Limit output to a single interface name.
    #[arg(long)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct IpAddr {
    ifname: String,
    flags: Option<Vec<String>>,
    operstate: Option<String>,
    address: Option<String>,
    mtu: Option<u32>,
    addr_info: Option<Vec<IpAddrInfo>>,
}

#[derive(Debug, Deserialize)]
struct IpAddrInfo {
    family: Option<String>,
    local: Option<String>,
    prefixlen: Option<u8>,
}

pub async fn run(args: InterfacesArgs) -> Result<InterfacesResponse, ClientError> {
    let mut cmd = Command::new("ip");
    cmd.arg("-j").arg("addr").arg("show");
    if let Some(n) = &args.name {
        cmd.arg("dev").arg(n);
    }

    let output = cmd
        .output()
        .await
        .map_err(|e| ClientError::new(format!("failed to run ip -j addr: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ClientError::new(format!(
            "ip addr exited with {:?}: {}",
            output.status.code(),
            stderr.trim()
        )));
    }

    let raw: Vec<IpAddr> = serde_json::from_slice(&output.stdout)
        .map_err(|e| ClientError::new(format!("failed to parse ip -j addr output: {e}")))?;

    let interfaces = raw
        .into_iter()
        .map(|i| {
            let up = i.operstate.as_deref().map_or_else(
                || {
                    i.flags
                        .as_ref()
                        .is_some_and(|f| f.iter().any(|s| s == "UP"))
                },
                |s| s.eq_ignore_ascii_case("UP"),
            );
            let addrs = i
                .addr_info
                .unwrap_or_default()
                .into_iter()
                .filter_map(|a| {
                    Some(InterfaceAddr {
                        family: a.family?,
                        local: a.local?,
                        prefixlen: a.prefixlen?,
                    })
                })
                .collect();
            Interface {
                name: i.ifname,
                up,
                mac: i.address,
                mtu: i.mtu.unwrap_or(0),
                addrs,
            }
        })
        .collect();

    Ok(InterfacesResponse { interfaces })
}
