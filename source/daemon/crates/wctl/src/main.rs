use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use wardnet_common::api::TunnelTestResponse;
use wardnet_common::config::ApplicationConfiguration;

#[derive(Parser)]
#[command(name = "wctl", about = "Wardnet CLI", version = env!("WARDNET_VERSION"))]
struct Cli {
    /// Output as JSON
    #[arg(long, global = true)]
    json: bool,

    /// Path to config file
    #[arg(long, global = true, default_value = "/etc/wardnet/wardnet.toml")]
    config: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show system status
    Status,
    /// Manage devices
    #[command(subcommand)]
    Devices(DevicesCommand),
    /// Manage tunnels
    #[command(subcommand)]
    Tunnels(TunnelsCommand),
    /// Manage auto-update
    #[command(subcommand)]
    Update(UpdateCommand),
    /// Export and restore encrypted backup bundles
    #[command(subcommand)]
    Backup(BackupCommand),
}

#[derive(Subcommand)]
enum BackupCommand {
    /// Show the current backup subsystem phase.
    Status,
    /// Export an encrypted `.wardnet.age` bundle to `--out`.
    Export {
        /// Destination path for the bundle.
        #[arg(long)]
        out: String,
        /// Read the passphrase from this file instead of prompting.
        /// Use `-` to read from stdin.
        #[arg(long)]
        passphrase_file: Option<String>,
    },
    /// Restore a previously-exported bundle.
    Import {
        /// Path to the `.wardnet.age` bundle.
        bundle: String,
        /// Read the passphrase from this file instead of prompting.
        /// Use `-` to read from stdin.
        #[arg(long)]
        passphrase_file: Option<String>,
    },
    /// List `.bak-<timestamp>` snapshots retained from prior restores.
    Snapshots,
}

#[derive(Subcommand)]
enum UpdateCommand {
    /// Show current auto-update status (version, channel, pending install).
    Status,
    /// Force a manifest refresh against the active channel.
    Check,
    /// Install the latest known release (or a specific version).
    Install {
        /// Optional version to install (must match the channel's latest).
        #[arg(long)]
        version: Option<String>,
    },
    /// Roll back to the `<live>.old` binary.
    Rollback,
}

#[derive(Subcommand)]
enum DevicesCommand {
    /// List all devices
    List,
    /// Show details for a specific device
    Show {
        /// Device ID
        id: String,
    },
    /// Set routing rule for a device
    SetRule {
        /// Device ID
        id: String,
        /// Routing target (direct, default, or a tunnel ID)
        target: String,
    },
}

#[derive(Subcommand)]
enum TunnelsCommand {
    /// List all tunnels
    List,
    /// Show details for a specific tunnel
    Show {
        /// Tunnel ID
        id: String,
    },
    /// Add a new tunnel
    Add {
        /// Tunnel label
        #[arg(long)]
        label: String,
        /// Country code (e.g., US, DE)
        #[arg(long)]
        country: String,
        /// `WireGuard` interface name
        #[arg(long)]
        interface: String,
    },
    /// Remove a tunnel
    Remove {
        /// Tunnel ID
        id: String,
    },
    /// Probe a tunnel for its exit IP, country, and latency.
    Test {
        /// Tunnel ID
        id: String,
    },
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let json = cli.json;

    match cli.command {
        Commands::Status => {
            println!("status: not yet implemented");
            std::process::ExitCode::SUCCESS
        }
        Commands::Devices(cmd) => {
            match cmd {
                DevicesCommand::List => println!("devices list: not yet implemented"),
                DevicesCommand::Show { id } => {
                    println!("devices show {id}: not yet implemented");
                }
                DevicesCommand::SetRule { id, target } => {
                    println!("devices set-rule {id} {target}: not yet implemented");
                }
            }
            std::process::ExitCode::SUCCESS
        }
        Commands::Tunnels(cmd) => match cmd {
            TunnelsCommand::List => {
                println!("tunnels list: not yet implemented");
                std::process::ExitCode::SUCCESS
            }
            TunnelsCommand::Show { id } => {
                println!("tunnels show {id}: not yet implemented");
                std::process::ExitCode::SUCCESS
            }
            TunnelsCommand::Add {
                label,
                country,
                interface,
            } => {
                println!(
                    "tunnels add --label {label} --country {country} --interface {interface}: not yet implemented"
                );
                std::process::ExitCode::SUCCESS
            }
            TunnelsCommand::Remove { id } => {
                println!("tunnels remove {id}: not yet implemented");
                std::process::ExitCode::SUCCESS
            }
            TunnelsCommand::Test { id } => match run_tunnel_test(&cli.config, &id, json).await {
                Ok(()) => std::process::ExitCode::SUCCESS,
                Err(e) => {
                    if json {
                        let payload = serde_json::json!({
                            "status": "error",
                            "message": e.to_string(),
                        });
                        println!("{payload}");
                    } else {
                        eprintln!("tunnel test failed: {e}");
                    }
                    std::process::ExitCode::from(1)
                }
            },
        },
        Commands::Update(cmd) => {
            match cmd {
                UpdateCommand::Status => println!("update status: not yet implemented"),
                UpdateCommand::Check => println!("update check: not yet implemented"),
                UpdateCommand::Install { version } => match version {
                    Some(v) => println!("update install --version {v}: not yet implemented"),
                    None => println!("update install: not yet implemented"),
                },
                UpdateCommand::Rollback => println!("update rollback: not yet implemented"),
            }
            std::process::ExitCode::SUCCESS
        }
        Commands::Backup(cmd) => {
            match cmd {
                BackupCommand::Status => println!("backup status: not yet implemented"),
                BackupCommand::Export {
                    out,
                    passphrase_file,
                } => match passphrase_file {
                    Some(p) => {
                        println!(
                            "backup export --out {out} --passphrase-file {p}: not yet implemented"
                        );
                    }
                    None => println!("backup export --out {out}: not yet implemented"),
                },
                BackupCommand::Import {
                    bundle,
                    passphrase_file,
                } => match passphrase_file {
                    Some(p) => {
                        println!(
                            "backup import {bundle} --passphrase-file {p}: not yet implemented"
                        );
                    }
                    None => println!("backup import {bundle}: not yet implemented"),
                },
                BackupCommand::Snapshots => println!("backup snapshots: not yet implemented"),
            }
            std::process::ExitCode::SUCCESS
        }
    }
}

/// Read the daemon URL from `config_path` (falls back to a 127.0.0.1
/// default when the file is absent), POST to `/api/tunnels/{id}/test`,
/// and render the result either as `key: value` lines (default) or raw
/// JSON (`--json`).
async fn run_tunnel_test(config_path: &str, id: &str, json: bool) -> Result<()> {
    let config = ApplicationConfiguration::load(&PathBuf::from(config_path))
        .with_context(|| format!("failed to load config at {config_path}"))?;

    let scheme = "http";
    let host = if config.server.host == "0.0.0.0" {
        "127.0.0.1"
    } else {
        config.server.host.as_str()
    };
    let url = format!(
        "{scheme}://{host}:{port}/api/tunnels/{id}/test",
        port = config.server.port
    );

    // Daemon timeout is 5 s; give the SDK / CLI a slightly larger
    // budget so a healthy probe never gets clipped at the client.
    let mut req = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .context("failed to build http client")?
        .post(&url);

    if let Ok(token) = std::env::var("WARDNET_API_KEY") {
        req = req.bearer_auth(token);
    }

    let resp = req.send().await.context("failed to call daemon")?;
    let status = resp.status();
    let body = resp.text().await.context("failed to read response body")?;

    if !status.is_success() {
        return Err(anyhow!(
            "daemon returned HTTP {status}: {body}",
            body = body.trim()
        ));
    }

    let parsed: TunnelTestResponse =
        serde_json::from_str(&body).context("failed to parse daemon response")?;

    if json {
        println!("{body}");
    } else {
        println!("Tunnel:  {}", parsed.result.tunnel_id);
        println!("Status:  ok");
        println!("Exit IP: {}", parsed.result.exit_ip);
        println!("Country: {}", parsed.result.country_code);
        println!("Latency: {} ms", parsed.result.latency_ms);
    }

    Ok(())
}
