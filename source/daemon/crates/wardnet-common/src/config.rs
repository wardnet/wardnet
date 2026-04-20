use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Top-level application configuration.
///
/// Loaded from a TOML file by the daemon, or constructed with defaults
/// by the mock server. All sub-crates receive this via dependency injection.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct ApplicationConfiguration {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub logging: LoggingConfig,
    pub network: NetworkConfig,
    pub auth: AuthConfig,
    pub admin: Option<AdminConfig>,
    pub tunnel: TunnelConfig,
    pub detection: DetectionConfig,
    pub otel: OtelConfig,
    pub vpn_providers: VpnProvidersConfig,
    pub pyroscope: PyroscopeConfig,
    pub update: UpdateConfig,
}

impl ApplicationConfiguration {
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

    /// Check whether a VPN provider is enabled. Returns `true` unless the
    /// provider is explicitly set to `false` in the `[vpn_providers.enabled]` table.
    #[must_use]
    pub fn is_vpn_provider_enabled(&self, id: &str) -> bool {
        self.vpn_providers.enabled.get(id).copied().unwrap_or(true)
    }
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

/// Supported database providers.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseProvider {
    #[default]
    Sqlite,
}

/// Database configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    /// Database provider. Only `sqlite` is supported for now.
    pub provider: DatabaseProvider,
    /// Connection string. For `SQLite` this is the file path.
    pub connection_string: String,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            provider: DatabaseProvider::Sqlite,
            connection_string: "./wardnet.db".to_owned(),
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    /// Log output format (console or json).
    pub format: LogFormat,
    /// Default log level for wardnet crates.
    pub level: String,
    /// Per-crate log level overrides.
    pub filters: std::collections::HashMap<String, String>,
    /// Path to the log file.
    pub path: PathBuf,
    /// How often to rotate the log file.
    pub rotation: LogRotation,
    /// Maximum number of rotated log files to keep.
    pub max_log_files: usize,
    /// Maximum number of recent errors kept in the ring buffer.
    pub max_recent_errors: usize,
    /// Channel capacity for the WebSocket log broadcast.
    pub broadcast_capacity: usize,
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
            max_recent_errors: 15,
            broadcast_capacity: 256,
        }
    }
}

impl LoggingConfig {
    /// Build an `EnvFilter`-compatible directive string from this config.
    #[must_use]
    pub fn to_filter_string(&self) -> String {
        use std::fmt::Write;

        let mut directives = format!(
            "warn,wardnetd={level},wardnet_common={level}",
            level = self.level,
        );

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

/// Initial admin account credentials.
///
/// Optional in the TOML file. When present, `bootstrap_admin` uses these
/// instead of generating random credentials.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminConfig {
    pub username: String,
    pub password: String,
}

/// `WireGuard` tunnel management settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TunnelConfig {
    pub keys_dir: PathBuf,
    pub idle_timeout_secs: u64,
    pub health_check_interval_secs: u64,
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
    pub enabled: bool,
    pub departure_timeout_secs: u64,
    pub batch_flush_interval_secs: u64,
    pub departure_scan_interval_secs: u64,
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
    pub enabled: bool,
    pub endpoint: String,
    pub service_name: String,
    pub interval_secs: u64,
    pub traces: OtelTracesConfig,
    pub logs: OtelLogsConfig,
    pub metrics: OtelMetricsConfig,
}

impl Default for OtelConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: "http://localhost:4317".to_owned(),
            service_name: "wardnetd".to_owned(),
            interval_secs: 10,
            traces: OtelTracesConfig::default(),
            logs: OtelLogsConfig::default(),
            metrics: OtelMetricsConfig::default(),
        }
    }
}

/// `OTel` trace export settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OtelTracesConfig {
    pub enabled: bool,
}

impl Default for OtelTracesConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// `OTel` log export settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OtelLogsConfig {
    pub enabled: bool,
}

impl Default for OtelLogsConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// VPN provider enable/disable overrides.
///
/// By default all registered providers are enabled. To disable a provider,
/// set its ID to `false`:
///
/// ```toml
/// [vpn_providers.enabled]
/// nordvpn = false
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VpnProvidersConfig {
    /// Map of provider ID to enabled flag. Providers not listed here are
    /// treated as enabled.
    pub enabled: std::collections::HashMap<String, bool>,
}

/// OpenTelemetry metrics collection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OtelMetricsConfig {
    pub enabled: bool,
    pub enabled_metrics: EnabledMetrics,
}

impl Default for OtelMetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            enabled_metrics: EnabledMetrics::default(),
        }
    }
}

/// Per-metric enable/disable toggles for the metrics collector.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EnabledMetrics {
    pub system_cpu_utilization: bool,
    pub system_memory_usage: bool,
    pub system_temperature: bool,
    pub system_network_io: bool,
    pub wardnet_device_count: bool,
    pub wardnet_tunnel_count: bool,
    pub wardnet_tunnel_active_count: bool,
    pub wardnet_uptime_seconds: bool,
    pub wardnet_db_size_bytes: bool,
}

impl Default for EnabledMetrics {
    fn default() -> Self {
        Self {
            system_cpu_utilization: true,
            system_memory_usage: true,
            system_temperature: true,
            system_network_io: true,
            wardnet_device_count: true,
            wardnet_tunnel_count: true,
            wardnet_tunnel_active_count: true,
            wardnet_uptime_seconds: true,
            wardnet_db_size_bytes: true,
        }
    }
}

/// Auto-update subsystem configuration.
///
/// Runtime behaviour (auto-update on/off, channel) lives in `system_config`
/// so admins can toggle it from the UI without editing the TOML. The values
/// here are the deploy-time knobs: where to fetch releases from, how often
/// to check, and the binary layout paths.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateConfig {
    /// HTTPS base URL for the release manifest server.
    ///
    /// The runner fetches `<manifest_base_url>/<channel>.json`. The default
    /// points at `wardnet.network`, which is the authenticity anchor: TLS
    /// protects the fetch, the embedded signing key protects the payload.
    pub manifest_base_url: String,
    /// Background check interval in seconds. Jittered by ±10% at runtime.
    pub check_interval_secs: u64,
    /// Absolute path to the currently-executing binary. Auto-detected from
    /// `/proc/self/exe` on startup when left at the default sentinel.
    pub live_binary_path: PathBuf,
    /// Directory used to stage downloads and extracted binaries. Must be
    /// writable by the daemon user and on the same filesystem as the live
    /// binary for atomic rename.
    pub staging_dir: PathBuf,
    /// Require a valid minisign signature before swapping the binary.
    /// Production builds must set this to `true`.
    pub require_signature: bool,
    /// HTTP request timeout for manifest/asset fetches, in seconds.
    pub http_timeout_secs: u64,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            manifest_base_url: "https://releases.wardnet.network".to_owned(),
            check_interval_secs: 6 * 60 * 60,
            live_binary_path: PathBuf::from("/usr/local/bin/wardnetd"),
            staging_dir: PathBuf::from("/var/lib/wardnet/updates"),
            require_signature: true,
            http_timeout_secs: 60,
        }
    }
}

/// Pyroscope continuous profiling agent configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PyroscopeConfig {
    pub enabled: bool,
    pub endpoint: String,
}

impl Default for PyroscopeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: "http://localhost:4040".to_owned(),
        }
    }
}
