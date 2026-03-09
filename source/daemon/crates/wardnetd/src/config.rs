use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Top-level daemon configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub logging: LoggingConfig,
    pub network: NetworkConfig,
    pub auth: AuthConfig,
    pub tunnel: TunnelConfig,
    pub detection: DetectionConfig,
    pub otel: OtelConfig,
}

/// HTTP server configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_owned(),
            port: 7411,
        }
    }
}

/// Database configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    pub path: PathBuf,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from("./wardnet.db"),
        }
    }
}

/// Log output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogFormat {
    Console,
    Json,
}

/// Log file rotation frequency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogRotation {
    /// Rotate log files every hour.
    Hourly,
    /// Rotate log files every day (default).
    Daily,
    /// Never rotate — single log file.
    Never,
}

/// Logging configuration.
///
/// Log levels are configured per crate, similar to log4j. The `level` field
/// sets the default for all wardnet crates. Third-party crates default to
/// `warn` unless explicitly overridden in the `filters` map.
///
/// Example config:
/// ```toml
/// [logging]
/// level = "debug"
/// [logging.filters]
/// pnet = "trace"
/// sqlx = "debug"
/// h2 = "info"
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    /// Log output format (console or json).
    pub format: LogFormat,
    /// Default log level for wardnet crates (e.g. "info", "debug", "trace").
    pub level: String,
    /// Per-crate log level overrides. Crates not listed here default to `warn`.
    /// Use this to enable verbose logging for specific dependencies.
    pub filters: std::collections::HashMap<String, String>,
    /// Path to the log file. The parent directory is created if it does not exist.
    pub path: PathBuf,
    /// How often to rotate the log file.
    pub rotation: LogRotation,
    /// Maximum number of rotated log files to keep. Oldest files are deleted
    /// when this limit is exceeded. Set to 0 to keep all files.
    pub max_log_files: usize,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            format: LogFormat::Console,
            level: "info".to_owned(),
            filters: std::collections::HashMap::new(),
            path: PathBuf::from("/var/log/wardnet/wardnetd.log"),
            rotation: LogRotation::Daily,
            max_log_files: 7,
        }
    }
}

impl LoggingConfig {
    /// Build an `EnvFilter`-compatible directive string from this config.
    ///
    /// Sets a restrictive default (`warn`) for all crates, then enables the
    /// configured level for wardnet crates, then applies per-crate overrides.
    #[must_use]
    pub fn to_filter_string(&self) -> String {
        use std::fmt::Write;

        // Start with warn for everything, then our crates at the configured level.
        let mut directives = format!(
            "warn,wardnetd={level},wardnet_types={level}",
            level = self.level,
        );

        // Apply per-crate overrides.
        for (crate_name, crate_level) in &self.filters {
            let _ = write!(directives, ",{crate_name}={crate_level}");
        }

        directives
    }
}

/// Network / LAN configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkConfig {
    pub lan_interface: String,
    pub default_policy: String,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            lan_interface: "eth0".to_owned(),
            default_policy: "direct".to_owned(),
        }
    }
}

/// Authentication settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    pub session_expiry_hours: u64,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            session_expiry_hours: 24,
        }
    }
}

/// `WireGuard` tunnel management settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TunnelConfig {
    /// Directory for `WireGuard` private key files.
    pub keys_dir: PathBuf,
    /// Seconds before an idle tunnel is torn down.
    pub idle_timeout_secs: u64,
    /// Seconds between health checks for active tunnels.
    pub health_check_interval_secs: u64,
    /// Seconds between stats collection for active tunnels.
    pub stats_interval_secs: u64,
}

impl Default for TunnelConfig {
    fn default() -> Self {
        Self {
            keys_dir: PathBuf::from("/etc/wardnet/keys"),
            idle_timeout_secs: 600,
            health_check_interval_secs: 10,
            stats_interval_secs: 5,
        }
    }
}

/// Device detection settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DetectionConfig {
    /// Enable passive device detection.
    pub enabled: bool,
    /// Seconds of inactivity before a device is considered gone.
    pub departure_timeout_secs: u64,
    /// Seconds between `last_seen` batch flushes to the database.
    pub batch_flush_interval_secs: u64,
    /// Seconds between departure scans.
    pub departure_scan_interval_secs: u64,
    /// Seconds between active ARP scans of the LAN subnet.
    pub arp_scan_interval_secs: u64,
}

impl Default for DetectionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            departure_timeout_secs: 300,
            batch_flush_interval_secs: 30,
            departure_scan_interval_secs: 60,
            arp_scan_interval_secs: 60,
        }
    }
}

/// OpenTelemetry OTLP export configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OtelConfig {
    /// Enable OpenTelemetry OTLP export of traces and logs.
    pub enabled: bool,
    /// OTLP gRPC endpoint (e.g. `http://10.232.1.189:4317`).
    pub endpoint: String,
    /// Service name reported to the collector as a resource attribute.
    pub service_name: String,
}

impl Default for OtelConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: "http://localhost:4317".to_owned(),
            service_name: "wardnetd".to_owned(),
        }
    }
}

impl Config {
    /// Load configuration from the given TOML file path. If the file does not
    /// exist, returns default configuration.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        if !path.exists() {
            tracing::info!(?path, "config file not found, using defaults");
            return Ok(Self::default());
        }

        let contents = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&contents)?;
        tracing::info!(?path, "loaded configuration");
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_file_missing() {
        let config = Config::load(Path::new("/tmp/wardnet-nonexistent-config.toml"))
            .expect("should return defaults");
        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 7411);
        assert_eq!(config.database.path, PathBuf::from("./wardnet.db"));
        assert_eq!(config.logging.format, LogFormat::Console);
        assert_eq!(config.logging.level, "info");
        assert_eq!(
            config.logging.path,
            PathBuf::from("/var/log/wardnet/wardnetd.log")
        );
        assert!(matches!(config.logging.rotation, LogRotation::Daily));
        assert_eq!(config.logging.max_log_files, 7);
        assert!(config.logging.filters.is_empty());
        assert_eq!(
            config.logging.to_filter_string(),
            "warn,wardnetd=info,wardnet_types=info"
        );
        assert_eq!(config.network.lan_interface, "eth0");
        assert_eq!(config.network.default_policy, "direct");
        assert_eq!(config.auth.session_expiry_hours, 24);
        assert_eq!(config.tunnel.keys_dir, PathBuf::from("/etc/wardnet/keys"));
        assert_eq!(config.tunnel.idle_timeout_secs, 600);
        assert_eq!(config.tunnel.health_check_interval_secs, 10);
        assert_eq!(config.tunnel.stats_interval_secs, 5);
        assert!(config.detection.enabled);
        assert_eq!(config.detection.departure_timeout_secs, 300);
        assert_eq!(config.detection.batch_flush_interval_secs, 30);
        assert_eq!(config.detection.departure_scan_interval_secs, 60);
        assert_eq!(config.detection.arp_scan_interval_secs, 60);
        assert!(!config.otel.enabled);
        assert_eq!(config.otel.endpoint, "http://localhost:4317");
        assert_eq!(config.otel.service_name, "wardnetd");
    }
}
