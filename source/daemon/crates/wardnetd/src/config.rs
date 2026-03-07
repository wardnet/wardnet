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

/// Logging configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LoggingConfig {
    pub format: LogFormat,
    pub level: String,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            format: LogFormat::Console,
            level: "info".to_owned(),
        }
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
        assert_eq!(config.network.lan_interface, "eth0");
        assert_eq!(config.network.default_policy, "direct");
        assert_eq!(config.auth.session_expiry_hours, 24);
    }
}
