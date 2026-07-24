//! Client configuration: where the daemon lives and how to authenticate.
//!
//! Resolved from, in order of precedence:
//! 1. `WARDNET_DAEMON_URL` / `WARDNET_TOKEN` environment variables,
//! 2. the TOML file (`--config`, else `$XDG_CONFIG_HOME/wardnet/wctl.toml`,
//!    else `~/.config/wardnet/wctl.toml`),
//! 3. the built-in default URL.
//!
//! ```toml
//! # ~/.config/wardnet/wctl.toml
//! daemon_url = "http://127.0.0.1:7411"
//! token = "<session token or API key>"
//! ```

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::CliError;

/// The daemon's default plain-HTTP LAN admin port (`ServerConfig::port`).
pub const DEFAULT_DAEMON_URL: &str = "http://127.0.0.1:7411";

/// Raw TOML shape — every field optional so a partial (or absent) file is
/// still valid and env/defaults can fill the gaps.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct RawConfig {
    pub(crate) daemon_url: Option<String>,
    pub(crate) token: Option<String>,
}

/// Fully-resolved client configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Base URL of the daemon, without a trailing slash.
    pub daemon_url: String,
    /// Bearer credential, if one was configured. Read-only commands work
    /// without it against an unauthenticated daemon, but every admin endpoint
    /// needs it — [`Config::token`] surfaces a clear error when it is missing.
    pub(crate) token: Option<String>,
}

impl Config {
    /// Resolve configuration, honouring an explicit `--config` path.
    ///
    /// A missing file is not an error: it is treated as an empty file so the
    /// daemon URL falls back to the default and the token comes from the
    /// environment (or is absent). An explicit `--config` that does not exist
    /// *is* an error, because the operator clearly meant to use it.
    ///
    /// # Errors
    /// Returns [`CliError::Config`] if the file exists but cannot be read or
    /// parsed, or if an explicit `--config` path is missing.
    pub fn resolve(config_flag: Option<&str>) -> Result<Self, CliError> {
        let raw = match config_flag {
            Some(path) => {
                let path = Path::new(path);
                if !path.exists() {
                    return Err(CliError::Config(format!(
                        "config file not found: {}",
                        path.display()
                    )));
                }
                load_file(path)?
            }
            None => match default_config_path() {
                Some(path) if path.exists() => load_file(&path)?,
                _ => RawConfig::default(),
            },
        };

        let daemon_url = std::env::var("WARDNET_DAEMON_URL")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .or(raw.daemon_url)
            .unwrap_or_else(|| DEFAULT_DAEMON_URL.to_owned());

        let token = std::env::var("WARDNET_TOKEN")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .or(raw.token);

        Ok(Self {
            daemon_url: daemon_url.trim_end_matches('/').to_owned(),
            token,
        })
    }

    /// The configured bearer token, or a [`CliError::Config`] pointing the
    /// operator at where to set one.
    ///
    /// # Errors
    /// Returns [`CliError::Config`] when no token is configured.
    pub fn token(&self) -> Result<&str, CliError> {
        self.token.as_deref().ok_or_else(|| {
            CliError::Config(
                "no API token configured — set `token` in wctl.toml or the \
                 WARDNET_TOKEN environment variable"
                    .to_owned(),
            )
        })
    }
}

/// Read and parse a TOML config file.
fn load_file(path: &Path) -> Result<RawConfig, CliError> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| CliError::Config(format!("cannot read {}: {e}", path.display())))?;
    toml::from_str(&contents)
        .map_err(|e| CliError::Config(format!("invalid config {}: {e}", path.display())))
}

/// The default config path: `$XDG_CONFIG_HOME/wardnet/wctl.toml`, falling back
/// to `$HOME/.config/wardnet/wctl.toml`. `None` when neither variable is set.
fn default_config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("wardnet").join("wctl.toml"))
}
