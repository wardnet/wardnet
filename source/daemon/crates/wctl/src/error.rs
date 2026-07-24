//! Error taxonomy and the exit codes it maps to.
//!
//! Every failure path funnels into [`CliError`]. `main` prints the message to
//! stderr and exits with [`CliError::exit_code`], so scripts can branch on
//! *why* a command failed, not just that it did.

use std::fmt;

use wardnet_common::api::ApiError;

/// A recoverable failure surfaced to the operator.
#[derive(Debug)]
pub enum CliError {
    /// Configuration is missing or unusable (no config file, unparseable
    /// TOML, or no token for a command that needs one).
    Config(String),
    /// The daemon could not be reached at all — connection refused, timeout,
    /// DNS, or TLS failure.
    Connection(String),
    /// The daemon answered with a non-2xx status and an [`ApiError`] body.
    Api { status: u16, body: ApiError },
    /// A local filesystem operation failed (reading a `.conf`, writing a
    /// bundle to `--out`).
    Io(String),
    /// The caller passed something clap could not reject on its own — an
    /// unparseable routing target or ID.
    Usage(String),
    /// The daemon replied but the body was not what this version expects.
    Protocol(String),
}

impl CliError {
    /// Process exit code for this failure.
    ///
    /// The values are stable and documented in the `wctl` help so scripts can
    /// rely on them: `2` usage, `3` config, `4` connection, `5` auth, `6` not
    /// found, `7` I/O, `1` everything else.
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Usage(_) => 2,
            Self::Config(_) => 3,
            Self::Connection(_) => 4,
            Self::Io(_) => 7,
            Self::Protocol(_) => 1,
            Self::Api { status, .. } => match *status {
                401 | 403 => 5,
                404 => 6,
                _ => 1,
            },
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(msg)
            | Self::Connection(msg)
            | Self::Io(msg)
            | Self::Usage(msg)
            | Self::Protocol(msg) => write!(f, "{msg}"),
            Self::Api { status, body } => {
                write!(f, "daemon returned {status}: {}", body.error)?;
                if let Some(detail) = &body.detail {
                    write!(f, " ({detail})")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for CliError {}
