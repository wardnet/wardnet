use clap::{Parser, Subcommand};

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
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Status => println!("status: not yet implemented"),
        Commands::Devices(cmd) => match cmd {
            DevicesCommand::List => println!("devices list: not yet implemented"),
            DevicesCommand::Show { id } => println!("devices show {id}: not yet implemented"),
            DevicesCommand::SetRule { id, target } => {
                println!("devices set-rule {id} {target}: not yet implemented");
            }
        },
        Commands::Tunnels(cmd) => match cmd {
            TunnelsCommand::List => println!("tunnels list: not yet implemented"),
            TunnelsCommand::Show { id } => println!("tunnels show {id}: not yet implemented"),
            TunnelsCommand::Add {
                label,
                country,
                interface,
            } => {
                println!(
                    "tunnels add --label {label} --country {country} --interface {interface}: not yet implemented"
                );
            }
            TunnelsCommand::Remove { id } => {
                println!("tunnels remove {id}: not yet implemented");
            }
        },
        Commands::Update(cmd) => match cmd {
            UpdateCommand::Status => println!("update status: not yet implemented"),
            UpdateCommand::Check => println!("update check: not yet implemented"),
            UpdateCommand::Install { version } => match version {
                Some(v) => println!("update install --version {v}: not yet implemented"),
                None => println!("update install: not yet implemented"),
            },
            UpdateCommand::Rollback => println!("update rollback: not yet implemented"),
        },
        Commands::Backup(cmd) => match cmd {
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
                    println!("backup import {bundle} --passphrase-file {p}: not yet implemented");
                }
                None => println!("backup import {bundle}: not yet implemented"),
            },
            BackupCommand::Snapshots => println!("backup snapshots: not yet implemented"),
        },
    }
}
