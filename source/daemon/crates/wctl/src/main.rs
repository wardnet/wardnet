use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "wctl", about = "Wardnet CLI", version)]
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
    }
}
