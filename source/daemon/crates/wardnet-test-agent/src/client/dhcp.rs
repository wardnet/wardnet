//! `client dhcp-renew` -- releases and renews the DHCP lease on an interface.

use clap::{Args, ValueEnum};
use tokio::process::Command;

use super::models::{ClientError, DhcpRenewResponse};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum DhcpClient {
    Dhclient,
    Dhcpcd,
}

impl DhcpClient {
    fn binary(self) -> &'static str {
        match self {
            Self::Dhclient => "dhclient",
            Self::Dhcpcd => "dhcpcd",
        }
    }

    fn release_args(self, iface: &str) -> Vec<String> {
        match self {
            Self::Dhclient => vec!["-r".to_owned(), iface.to_owned()],
            Self::Dhcpcd => vec!["-k".to_owned(), iface.to_owned()],
        }
    }

    fn renew_args(self, iface: &str) -> Vec<String> {
        match self {
            Self::Dhclient => vec![iface.to_owned()],
            Self::Dhcpcd => vec!["-n".to_owned(), iface.to_owned()],
        }
    }
}

#[derive(Debug, Args)]
pub struct DhcpRenewArgs {
    /// Interface to renew on.
    interface: String,

    /// DHCP client implementation to invoke.
    #[arg(long, value_enum, default_value_t = DhcpClient::Dhclient)]
    client: DhcpClient,
}

pub async fn run(args: DhcpRenewArgs) -> Result<DhcpRenewResponse, ClientError> {
    if !is_valid_interface_name(&args.interface) {
        return Err(ClientError::new(format!(
            "invalid interface name: {}",
            args.interface
        )));
    }

    let bin = args.client.binary();

    let release = Command::new(bin)
        .args(args.client.release_args(&args.interface))
        .output()
        .await
        .map_err(|e| ClientError::new(format!("failed to run {bin} release: {e}")))?;

    let renew = Command::new(bin)
        .args(args.client.renew_args(&args.interface))
        .output()
        .await
        .map_err(|e| ClientError::new(format!("failed to run {bin} renew: {e}")))?;

    let mut stdout = String::from_utf8_lossy(&release.stdout).into_owned();
    stdout.push_str(&String::from_utf8_lossy(&renew.stdout));
    let mut stderr = String::from_utf8_lossy(&release.stderr).into_owned();
    stderr.push_str(&String::from_utf8_lossy(&renew.stderr));

    Ok(DhcpRenewResponse {
        interface: args.interface,
        client: bin.to_owned(),
        success: renew.status.success(),
        stdout,
        stderr,
    })
}

fn is_valid_interface_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 15
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}
