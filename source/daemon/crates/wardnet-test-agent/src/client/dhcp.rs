//! `client dhcp-renew` -- releases and renews the DHCP lease on an interface.

use clap::{Args, ValueEnum};
use tokio::process::Command;

use super::models::{ClientError, DhcpRenewResponse};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum DhcpClient {
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

    fn release_args(self, iface: &str, script: Option<&str>) -> Vec<String> {
        match self {
            Self::Dhclient => {
                let mut args = Vec::with_capacity(4);
                if let Some(s) = script {
                    args.push("-sf".to_owned());
                    args.push(s.to_owned());
                }
                args.push("-r".to_owned());
                args.push(iface.to_owned());
                args
            }
            // dhcpcd's interface-config plumbing isn't script-driven the same
            // way; the `script` arg is dhclient-only.
            Self::Dhcpcd => vec!["-k".to_owned(), iface.to_owned()],
        }
    }

    fn renew_args(self, iface: &str, script: Option<&str>) -> Vec<String> {
        match self {
            Self::Dhclient => {
                let mut args = Vec::with_capacity(3);
                if let Some(s) = script {
                    args.push("-sf".to_owned());
                    args.push(s.to_owned());
                }
                args.push(iface.to_owned());
                args
            }
            Self::Dhcpcd => vec!["-n".to_owned(), iface.to_owned()],
        }
    }
}

/// Env var that points at a custom dhclient script. The e2e client
/// images set this to a script that adds the leased IP without
/// flushing the docker-IPAM-assigned address. Unset in any other
/// context, so dhclient falls back to its default `/sbin/dhclient-script`.
const DHCLIENT_SCRIPT_ENV: &str = "WARDNET_TEST_DHCLIENT_SCRIPT";

#[derive(Debug, Args)]
pub struct DhcpRenewArgs {
    /// Interface to renew on.
    pub interface: String,

    /// DHCP client implementation to invoke.
    #[arg(long, value_enum, default_value_t = DhcpClient::Dhclient)]
    pub client: DhcpClient,
}

pub async fn run(args: DhcpRenewArgs) -> Result<DhcpRenewResponse, ClientError> {
    if !is_valid_interface_name(&args.interface) {
        return Err(ClientError::new(format!(
            "invalid interface name: {}",
            args.interface
        )));
    }

    let bin = args.client.binary();
    let script = std::env::var(DHCLIENT_SCRIPT_ENV).ok();
    let script_ref = script.as_deref();

    let release = Command::new(bin)
        .args(args.client.release_args(&args.interface, script_ref))
        .output()
        .await
        .map_err(|e| ClientError::new(format!("failed to run {bin} release: {e}")))?;

    let renew = Command::new(bin)
        .args(args.client.renew_args(&args.interface, script_ref))
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
        release_success: release.status.success(),
        renew_success: renew.status.success(),
        stdout,
        stderr,
    })
}

fn is_valid_interface_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 15
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}
