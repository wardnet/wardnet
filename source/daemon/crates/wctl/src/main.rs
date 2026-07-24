//! `wctl` — the Wardnet command-line client.
//!
//! A thin front-end over the daemon's HTTP API: it reads the daemon URL and an
//! API token from `~/.config/wardnet/wctl.toml` (see [`config`]) and drives the
//! same endpoints the web UI uses, so the gateway can be scripted from a shell.
//!
//! Every command exits non-zero on failure; the codes are documented on
//! [`error::CliError::exit_code`].

mod client;
mod commands;
mod config;
mod error;
mod output;

#[cfg(test)]
mod tests;

use clap::{Parser, Subcommand};

use crate::client::Client;
use crate::config::Config;
use crate::error::CliError;

#[derive(Parser)]
#[command(name = "wctl", about = "Wardnet CLI", version = env!("WARDNET_VERSION"))]
struct Cli {
    /// Output as JSON
    #[arg(long, global = true)]
    json: bool,

    /// Path to the wctl config file
    /// (default: `$XDG_CONFIG_HOME/wardnet/wctl.toml`)
    #[arg(long, global = true)]
    config: Option<String>,

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
    /// Add a new tunnel by importing a `WireGuard` `.conf` file
    Add {
        /// Tunnel label
        #[arg(long)]
        label: String,
        /// Country code (e.g., US, DE)
        #[arg(long)]
        country: String,
        /// Path to the `WireGuard` `.conf` file to import
        #[arg(long)]
        conf: String,
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
async fn main() {
    let cli = Cli::parse();
    if let Err(err) = run(cli).await {
        eprintln!("error: {err}");
        std::process::exit(err.exit_code());
    }
}

async fn run(cli: Cli) -> Result<(), CliError> {
    let config = Config::resolve(cli.config.as_deref())?;
    // Every admin endpoint requires a bearer token, so resolve it up front and
    // fail with a clear message rather than a 401 from each command.
    let client = Client::new(&config.daemon_url, config.token()?)?;
    let json = cli.json;

    match cli.command {
        Commands::Status => commands::status::run(&client, json).await,
        Commands::Devices(cmd) => match cmd {
            DevicesCommand::List => commands::devices::list(&client, json).await,
            DevicesCommand::Show { id } => commands::devices::show(&client, json, &id).await,
            DevicesCommand::SetRule { id, target } => {
                commands::devices::set_rule(&client, json, &id, &target).await
            }
        },
        Commands::Tunnels(cmd) => match cmd {
            TunnelsCommand::List => commands::tunnels::list(&client, json).await,
            TunnelsCommand::Show { id } => commands::tunnels::show(&client, json, &id).await,
            TunnelsCommand::Add {
                label,
                country,
                conf,
            } => commands::tunnels::add(&client, json, &label, &country, &conf).await,
            TunnelsCommand::Remove { id } => commands::tunnels::remove(&client, json, &id).await,
            TunnelsCommand::Test { id } => commands::tunnels::test(&client, json, &id).await,
        },
        Commands::Update(cmd) => match cmd {
            UpdateCommand::Status => commands::update::status(&client, json).await,
            UpdateCommand::Check => commands::update::check(&client, json).await,
            UpdateCommand::Install { version } => {
                commands::update::install(&client, json, version.as_deref()).await
            }
            UpdateCommand::Rollback => commands::update::rollback(&client, json).await,
        },
        Commands::Backup(cmd) => match cmd {
            BackupCommand::Status => commands::backup::status(&client, json).await,
            BackupCommand::Export {
                out,
                passphrase_file,
            } => commands::backup::export(&client, json, &out, passphrase_file.as_deref()).await,
            BackupCommand::Import {
                bundle,
                passphrase_file,
            } => commands::backup::import(&client, json, &bundle, passphrase_file.as_deref()).await,
            BackupCommand::Snapshots => commands::backup::snapshots(&client, json).await,
        },
    }
}
